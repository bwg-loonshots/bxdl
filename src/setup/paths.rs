//! Filesystem-aware guards for setup storage, exports, and customer references.
//! Existing names are compared by device/inode. On macOS, uncreated suffixes
//! also use conservative Unicode/case equivalence; this can reject distinct
//! names on case-sensitive volumes. These checks do not reserve a path or
//! isolate the caller from other processes running with the same UID.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

#[cfg(target_os = "macos")]
use unicode_normalization::UnicodeNormalization as _;

use crate::error::{BxdlError, Result};

const MAX_PATH_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    device: u64,
    inode: u64,
}
impl Identity {
    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

struct ObservedPath {
    // names[n] is below identities[n], where identity[0] represents `/`.
    names: Vec<String>,
    identities: Vec<Identity>,
}

/// True for the same path or either path containing the other. Missing names
/// are considered as well, so creating a setup directory cannot consume a
/// currently missing credential path or data namespace.
pub fn overlaps(a: &Path, b: &Path) -> Result<bool> {
    compare(a, b, |a, b| a.starts_with(b) || b.starts_with(a))
}

/// True for the same existing inode or equivalent names below an existing
/// shared ancestor. macOS equivalence intentionally errs toward refusal.
pub fn same(a: &Path, b: &Path) -> Result<bool> {
    compare(a, b, |a, b| a == b)
}

fn compare(
    a: &Path,
    b: &Path,
    suffix_matches: impl Fn(&[String], &[String]) -> bool,
) -> Result<bool> {
    // Inspect both paths even when their strings match: a symlink or an
    // inaccessible path must not be accepted by an early lexical comparison.
    let a = observe(a)?;
    let b = observe(b)?;
    let mut anchors = BTreeMap::<Identity, Vec<usize>>::new();
    for (index, identity) in b.identities.iter().enumerate() {
        anchors.entry(*identity).or_default().push(index);
    }
    for (a_index, identity) in a.identities.iter().enumerate().rev() {
        if let Some(b_indices) = anchors.get(identity) {
            for &b_index in b_indices {
                if suffix_matches(&a.names[a_index..], &b.names[b_index..]) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn observe(path: &Path) -> Result<ObservedPath> {
    let text = path.to_str().ok_or_else(invalid_path)?;
    if !path.is_absolute() || text.len() > MAX_PATH_BYTES || text.chars().any(char::is_control) {
        return Err(invalid_path());
    }
    let mut original_names = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => original_names.push(name),
            // The caller must supply absolute, lexically normalized inputs.
            _ => return Err(invalid_path()),
        }
    }
    let mut current = PathBuf::from("/");
    let root = inspect(&current)?.ok_or_else(inaccessible_path)?;
    if !root.is_dir() {
        return Err(invalid_path());
    }
    let mut identities = vec![Identity::of(&root)];
    let mut names = Vec::with_capacity(original_names.len());
    let mut missing = false;
    for (index, name) in original_names.iter().enumerate() {
        let text = name.to_str().ok_or_else(invalid_path)?;
        names.push(equivalent_name(text));
        current.push(name);
        if missing {
            continue;
        }
        match inspect(&current)? {
            Some(metadata) => {
                if !metadata.is_dir() && index + 1 != original_names.len() {
                    return Err(invalid_path());
                }
                identities.push(Identity::of(&metadata));
            }
            None => missing = true,
        }
    }
    Ok(ObservedPath { names, identities })
}

fn inspect(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
                return Err(invalid_path());
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        // Permission, I/O and non-directory errors are not evidence of absence.
        Err(_) => Err(inaccessible_path()),
    }
}

#[cfg(target_os = "macos")]
fn equivalent_name(name: &str) -> String {
    name.nfd()
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .nfd()
        .collect()
}
#[cfg(not(target_os = "macos"))]
fn equivalent_name(name: &str) -> String {
    name.to_owned()
}

fn invalid_path() -> BxdlError {
    BxdlError::new(
        "SETUP_PATH_INVALID",
        "경로에 링크 또는 올바르지 않은 구성요소가 있습니다. 실제 절대 경로를 확인하세요.",
    )
}
fn inaccessible_path() -> BxdlError {
    BxdlError::new(
        "SETUP_PATH_UNAVAILABLE",
        "경로를 검사할 수 없습니다. 상위 폴더와 접근 권한을 확인하세요.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        (temporary, root)
    }

    #[test]
    fn existing_and_missing_descendants_are_symmetric_but_siblings_are_distinct() {
        let (_temporary, root) = fixture();
        let data = root.join("data");
        fs::create_dir(&data).unwrap();
        let descendant = data.join("missing/deeper/config.json");
        assert!(overlaps(&data, &descendant).unwrap());
        assert!(overlaps(&descendant, &data).unwrap());
        assert!(!same(&data, &descendant).unwrap());
        assert!(!overlaps(&data, &root.join("database/config.json")).unwrap());
        assert!(!overlaps(&data.join("one"), &data.join("two")).unwrap());
        let future_key = root.join("missing-key.pem");
        assert!(overlaps(&future_key, &future_key.join("setup")).unwrap());
        assert!(same(&descendant, &descendant).unwrap());
    }

    #[test]
    fn existing_hardlink_alias_is_the_same_identity() {
        let (_temporary, root) = fixture();
        let original = root.join("original");
        let alias = root.join("unrelated-name");
        fs::write(&original, b"test fixture").unwrap();
        fs::hard_link(&original, &alias).unwrap();
        assert!(same(&original, &alias).unwrap());
        assert!(overlaps(&original, &alias).unwrap());
        fs::write(root.join("separate"), b"test fixture").unwrap();
        assert!(!same(&original, &root.join("separate")).unwrap());
    }

    #[test]
    fn symlinks_and_file_parents_are_errors_even_for_equal_paths() {
        let (_temporary, root) = fixture();
        let directory = root.join("directory");
        fs::create_dir(&directory).unwrap();
        let link = root.join("alias");
        symlink(&directory, &link).unwrap();
        assert!(same(&link, &link).is_err());
        assert!(overlaps(&link.join("new"), &directory).is_err());
        let dangling = root.join("dangling");
        symlink(root.join("absent"), &dangling).unwrap();
        assert!(same(&dangling, &dangling).is_err());
        let file = root.join("regular");
        fs::write(&file, b"fixture").unwrap();
        assert!(overlaps(&file.join("child"), &root).is_err());
        assert!(same(Path::new("relative"), &root).is_err());
        assert!(same(&root.join(".."), &root).is_err());
    }

    #[test]
    fn permission_errors_do_not_become_missing_paths_or_expose_input() {
        let (_temporary, root) = fixture();
        let blocked = root.join("PRIVATE_PATH_CANARY");
        fs::create_dir(&blocked).unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = overlaps(&blocked.join("missing"), &root.join("other"));
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.code, "SETUP_PATH_UNAVAILABLE");
        assert!(!error.message.contains("CANARY"));
        assert!(!error.message.contains(root.to_str().unwrap()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_existing_directory_alias_cannot_escape_the_workspace() {
        let (_temporary, root) = fixture();
        let workspace = root.join("setup");
        fs::create_dir(&workspace).unwrap();
        let alias = root.join("SETUP");
        assert!(same(&workspace, &alias).unwrap());
        assert!(overlaps(&workspace, &alias.join("revision00000002.json")).unwrap());
        assert!(
            same(
                &workspace.join("instance.json"),
                &alias.join("INSTANCE.JSON")
            )
            .unwrap()
        );
        // On a case-insensitive volume the OS also proves this equivalence.
        if let Ok(metadata) = fs::symlink_metadata(&alias) {
            assert_eq!(
                Identity::of(&fs::symlink_metadata(&workspace).unwrap()),
                Identity::of(&metadata)
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_missing_suffixes_fold_unicode_normalization_and_case() {
        let (_temporary, root) = fixture();
        assert!(same(&root.join("é/file"), &root.join("e\u{301}/FILE")).unwrap());
        assert!(same(&root.join("E/new"), &root.join("e/NEW")).unwrap());
        assert!(same(&root.join("Keys/new"), &root.join("keys/new")).unwrap());
        assert!(
            overlaps(
                &root.join("É/data"),
                &root.join("e\u{301}/DATA/config.json")
            )
            .unwrap()
        );
        assert!(!same(&root.join("é/file"), &root.join("e/file")).unwrap());
        assert!(!root.join("é").exists());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn other_platform_missing_components_keep_exact_spelling() {
        let (_temporary, root) = fixture();
        assert!(!same(&root.join("E/new"), &root.join("e/new")).unwrap());
        assert!(!same(&root.join("é/file"), &root.join("e\u{301}/file")).unwrap());
    }
}
