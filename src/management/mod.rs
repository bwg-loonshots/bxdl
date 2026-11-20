//! User-scoped management CLI installation; independent of node packages/data.
pub mod bundle;
mod fs;
mod probe;
mod store;

use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Locations {
    pub root: PathBuf,
    pub bin_dir: PathBuf,
}
#[derive(Clone, Debug)]
pub struct InstallOptions {
    pub locations: Locations,
    pub from: Option<PathBuf>,
    pub trust: bundle::VerifyOptions,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseInfo {
    pub release_id: String,
    pub version: String,
    pub revision: String,
    pub target: String,
    pub manifest_sha256: String,
    pub authenticity: String,
}
impl ReleaseInfo {
    fn from_bundle(b: &bundle::Bundle) -> Self {
        Self {
            release_id: b.release_id(),
            version: b.manifest.version.clone(),
            revision: b.manifest.revision.clone(),
            target: b.manifest.target.clone(),
            manifest_sha256: b.manifest_sha256.clone(),
            authenticity: b.authenticity.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub action: String,
    pub status: String,
    pub changed: bool,
    pub root: PathBuf,
    pub bin_dir: PathBuf,
    pub executable: PathBuf,
    pub current: Option<ReleaseInfo>,
    pub previous: Option<ReleaseInfo>,
    pub invoked_executable: Option<PathBuf>,
    pub path_status: String,
    pub shell_setup: Vec<String>,
    pub activation: Option<String>,
}

pub fn default_locations(root: Option<&Path>, bin_dir: Option<&Path>) -> Result<Locations> {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| {
                error(
                    "CLI_LOCATION_REQUIRED",
                    "사용자 홈을 확인할 수 없습니다. --root와 --bin-dir을 지정하세요.",
                )
            })
    };
    let root = match root {
        Some(p) => crate::setup::absolute(p)?,
        None => default_root(&home()?, host_target())?,
    };
    let bin_dir = match bin_dir {
        Some(p) => crate::setup::absolute(p)?,
        None => match store::recorded_bin_dir(&root)? {
            Some(p) => p,
            None => home()?.join(".local/bin"),
        },
    };
    Ok(Locations { root, bin_dir })
}
pub fn install(o: &InstallOptions) -> Result<Report> {
    platform()?;
    store::install(o, false, probe::check)
}
pub fn update(o: &InstallOptions) -> Result<Report> {
    platform()?;
    if o.from.is_none() {
        return Err(error(
            "INVALID_ARGUMENTS",
            "update에는 --from 배포본을 지정하세요.",
        ));
    }
    store::install(o, true, probe::check)
}
pub fn info(l: &Locations) -> Result<Report> {
    store::info(l)
}
pub fn rollback(l: &Locations) -> Result<Report> {
    platform()?;
    store::rollback(l, probe::check)
}
/// Supported native management-binary targets; this is not node/service acceptance.
pub fn host_target() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("darwin-arm64")
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        Some("linux-amd64")
    } else {
        None
    }
}
fn default_root(home: &Path, target: Option<&str>) -> Result<PathBuf> {
    match target {
        Some("darwin-arm64") => Ok(home.join("Library/Application Support/BXDL-CLI")),
        Some("linux-amd64") => Ok(home.join(".local/share/BXDL-CLI")),
        _ => Err(error(
            "CLI_LOCATION_REQUIRED",
            "이 플랫폼에서는 --root와 --bin-dir을 지정하세요.",
        )),
    }
}
fn platform() -> Result<()> {
    host_target().map(|_| ()).ok_or_else(|| {
        error(
            "CLI_PLATFORM_UNSUPPORTED",
            "관리 CLI 설치·업데이트·복귀는 macOS arm64 또는 Linux x86_64/glibc에서 지원합니다.",
        )
    })
}
fn require_target(target: &str) -> Result<()> {
    if host_target() == Some(target) {
        Ok(())
    } else {
        Err(error(
            "CLI_TARGET_MISMATCH",
            "배포본의 OS·CPU 대상이 현재 관리 CLI와 다릅니다. 기존 선택을 유지했습니다.",
        ))
    }
}

fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn shell_path(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}
fn report(
    l: &Locations,
    action: &str,
    status: &str,
    changed: bool,
    current: Option<ReleaseInfo>,
    previous: Option<ReleaseInfo>,
    activation: Option<String>,
) -> Report {
    let executable = l.bin_dir.join("bxdl");
    let selected = std::env::var_os("PATH")
        .filter(|s| s.len() <= 65536)
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .take(256)
                .map(|p| p.join("bxdl"))
                .find(|p| p.is_file())
        });
    let path_status = match selected {
        None => "NOT_ON_PATH",
        Some(p)
            if std::fs::canonicalize(&p).ok() == std::fs::canonicalize(&executable).ok()
                && executable.is_file() =>
        {
            "SELECTED"
        }
        Some(_) => "SHADOWED",
    };
    let shell_setup = if host_target() == Some("linux-amd64") {
        vec![
            format!("export PATH={}:\"$PATH\"", shell_path(&l.bin_dir)),
            "hash -r".into(),
            "command -v bxdl".into(),
            "bxdl version --json".into(),
        ]
    } else {
        vec![
            format!("export PATH={}:\"$PATH\"", shell_path(&l.bin_dir)),
            format!(
                "fpath=({} $fpath)",
                shell_path(&l.root.join("current/share/zsh/site-functions"))
            ),
            "autoload -Uz compinit".into(),
            "compinit".into(),
            "rehash".into(),
            "command -v bxdl".into(),
            "bxdl version --json".into(),
            "source <(bxdl completion zsh)".into(),
        ]
    };
    Report {
        action: action.into(),
        status: status.into(),
        changed,
        root: l.root.clone(),
        bin_dir: l.bin_dir.clone(),
        executable,
        current,
        previous,
        invoked_executable: std::env::current_exe().ok(),
        path_status: path_status.into(),
        shell_setup,
        activation,
    }
}

#[cfg(test)]
fn test_binary(target: &str, marker: u8) -> Vec<u8> {
    if target == "darwin-arm64" {
        let mut b = vec![0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1];
        b.resize(256, marker);
        return b;
    }
    let interpreter = b"/lib64/ld-linux-x86-64.so.2\0";
    let mut b = vec![0; 256];
    b[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
    b[16..18].copy_from_slice(&3_u16.to_le_bytes());
    b[18..20].copy_from_slice(&62_u16.to_le_bytes());
    b[20..24].copy_from_slice(&1_u32.to_le_bytes());
    b[24..32].copy_from_slice(&0x1000_u64.to_le_bytes());
    b[32..40].copy_from_slice(&64_u64.to_le_bytes());
    b[52..54].copy_from_slice(&64_u16.to_le_bytes());
    b[54..56].copy_from_slice(&56_u16.to_le_bytes());
    b[56..58].copy_from_slice(&1_u16.to_le_bytes());
    b[64..68].copy_from_slice(&3_u32.to_le_bytes());
    b[72..80].copy_from_slice(&120_u64.to_le_bytes());
    b[96..104].copy_from_slice(&(interpreter.len() as u64).to_le_bytes());
    b[120..120 + interpreter.len()].copy_from_slice(interpreter);
    b[255] = marker;
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_are_platform_specific_and_unsupported_defaults_are_rejected() {
        let home = Path::new("/home/test-user");
        assert_eq!(
            default_root(home, Some("linux-amd64")).unwrap(),
            home.join(".local/share/BXDL-CLI")
        );
        assert_eq!(
            default_root(home, Some("darwin-arm64")).unwrap(),
            home.join("Library/Application Support/BXDL-CLI")
        );
        assert!(default_root(home, Some("linux-arm64")).is_err());
        assert!(default_root(home, None).is_err());
    }
}
