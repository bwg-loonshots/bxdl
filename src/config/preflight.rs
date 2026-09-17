use super::{Check, Instance, Report, load, parse_bytes, report, resolve_source, secret_paths};
use crate::error::Result;
use std::fs::{self, Metadata};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Metadata-only observations, never a DB/key content read, process launch,
/// network probe, or filesystem mutation. Local failures remain report checks.
pub fn preflight(path: &Path) -> Result<Report> {
    let (config, raw, source) = load(path)?;
    Ok(inspect(
        &config,
        &raw,
        file_check("configMetadata", &source, false),
    ))
}

/// Inspect references in an unsaved candidate without opening or inspecting
/// its intended config file. The report hashes the supplied raw bytes exactly.
/// Existing-file collisions belong to the setup model, not this observation.
pub fn preflight_bytes(raw: &[u8], source: &Path) -> Result<Report> {
    let source = resolve_source(source)?;
    let config = parse_bytes(raw, &source)?;
    Ok(inspect(
        &config,
        raw,
        Check::new(
            "configMetadata",
            "NOT_CHECKED",
            "DRAFT_NOT_WRITTEN",
            "Candidate config is not written; config file metadata was not inspected.",
        ),
    ))
}

fn inspect(config: &Instance, raw: &[u8], config_metadata: Check) -> Report {
    let mut result = report(config, raw, "INCOMPLETE");
    result.checks.push(config_metadata);
    result.checks.push(file_check(
        "chainDescriptionMetadata",
        Path::new(&config.chain_description),
        false,
    ));
    for (name, path) in [
        "validatorKeystoreMetadata",
        "validatorPasswordMetadata",
        "tlsKeyStoreMetadata",
        "tlsKeyPasswordMetadata",
        "tlsTrustStoreMetadata",
        "tlsTrustPasswordMetadata",
    ]
    .into_iter()
    .zip(secret_paths(config))
    {
        result.checks.push(file_check(name, Path::new(path), true));
    }
    result
        .checks
        .push(data_check(Path::new(&config.storage.data_directory)));
    for (name, reason, message) in [
        (
            "nigoCanonicalConfiguration",
            "NIGO_CONTRACT_UNAVAILABLE",
            "Canonical chain, genesis, profile, and validator rules require the NIGO contract.",
        ),
        (
            "keyAndCertificateIdentity",
            "KEY_CONTENT_NOT_READ",
            "Credential contents, identity, expiration, and trust were not inspected.",
        ),
        (
            "databaseIntegrity",
            "DB_NOT_OPENED",
            "Database initialization, integrity, WAL state, and exclusive access were not inspected.",
        ),
        (
            "networkAvailability",
            "NETWORK_NOT_PROBED",
            "No address, port, peer, or network connection was probed.",
        ),
        (
            "runtimeAndNativeSupport",
            "RUNTIME_NOT_EXECUTED",
            "Java, native libraries, and host support were not tested.",
        ),
        (
            "hostAndServiceManager",
            "HOST_PLATFORM_NOT_VERIFIED",
            "Host platform support, service manager availability, and startup/stop behavior were not inspected.",
        ),
        (
            "effectiveServicePermissions",
            "SERVICE_IDENTITY_NOT_VERIFIED",
            "Ownership, ACLs, mount policy, and effective service access require the target-platform installation gate.",
        ),
        (
            "instancePersistence",
            "INSTANCE_NOT_PERSISTED",
            "No installed instance, identity manifest, or service manager state was created or verified.",
        ),
    ] {
        result
            .checks
            .push(Check::new(name, "NOT_CHECKED", reason, message));
    }
    if result.checks.iter().any(|check| check.status == "FAIL") {
        result.outcome = "FAIL".into();
    }
    result
}

fn file_check(name: &str, path: &Path, secret: bool) -> Check {
    let metadata = match inspect_path(path) {
        Ok(metadata) => metadata,
        Err(reason) => {
            return Check::new(
                name,
                "FAIL",
                reason,
                "Referenced path metadata is unavailable or unsafe; contents were not inspected.",
            );
        }
    };
    if !metadata.is_file() {
        return Check::new(
            name,
            "FAIL",
            "REFERENCE_NOT_REGULAR",
            "Reference must identify a regular file.",
        );
    }
    let Some(mode) = permission_bits(&metadata) else {
        return Check::new(
            name,
            "NOT_CHECKED",
            "POSIX_PERMISSIONS_UNAVAILABLE",
            "POSIX file permission metadata is unavailable on this platform.",
        );
    };
    let forbidden = if secret { 0o137 } else { 0o022 };
    if mode & forbidden != 0 {
        return Check::new(
            name,
            "FAIL",
            "REFERENCE_PERMISSIONS_UNSAFE",
            "Referenced file permissions exceed the permitted metadata policy.",
        );
    }
    if secret {
        let parent = path
            .parent()
            .and_then(|parent| fs::symlink_metadata(parent).ok());
        if !parent.as_ref().is_some_and(|info| {
            info.is_dir() && permission_bits(info).is_some_and(|mode| mode & 0o027 == 0)
        }) {
            return Check::new(
                name,
                "FAIL",
                "SECRET_DIRECTORY_PERMISSIONS_UNSAFE",
                "Secret parent directory must not be group-writable or accessible to other users.",
            );
        }
    }
    Check::new(
        name,
        "PASS",
        "REFERENCE_METADATA_VALID",
        "File type and basic permission metadata passed; contents and effective access remain unchecked.",
    )
}

fn data_check(path: &Path) -> Check {
    let name = "dataDirectoryMetadata";
    let metadata = match inspect_path(path) {
        Ok(metadata) => metadata,
        Err("REFERENCE_MISSING") => {
            return Check::new(
                name,
                "NOT_CHECKED",
                "DATA_NOT_INITIALIZED",
                "Data directory is missing; initialization is unavailable and no directory was created.",
            );
        }
        Err(reason) => {
            return Check::new(
                name,
                "FAIL",
                reason,
                "Data path metadata is unavailable or unsafe; no database was opened.",
            );
        }
    };
    if !metadata.is_dir() {
        return Check::new(
            name,
            "FAIL",
            "DATA_NOT_DIRECTORY",
            "Data path must identify a directory.",
        );
    }
    let Some(mode) = permission_bits(&metadata) else {
        return Check::new(
            name,
            "NOT_CHECKED",
            "POSIX_PERMISSIONS_UNAVAILABLE",
            "POSIX directory permission metadata is unavailable on this platform.",
        );
    };
    if mode & 0o027 != 0 {
        return Check::new(
            name,
            "FAIL",
            "DATA_PERMISSIONS_UNSAFE",
            "Data directory must not be group-writable or accessible to other users.",
        );
    }
    Check::new(
        name,
        "PASS",
        "DATA_DIRECTORY_METADATA_VALID",
        "Directory metadata passed; its contents, initialization, integrity, and lock were not inspected.",
    )
}

// Reject symlinks in every component. These observations do not lock paths or
// promise protection against a concurrent filesystem change.
fn inspect_path(path: &Path) -> std::result::Result<Metadata, &'static str> {
    let components: Vec<_> = path.components().collect();
    let mut current = PathBuf::new();
    let mut last = None;
    for (index, part) in components.iter().enumerate() {
        current.push(part.as_os_str());
        let metadata = fs::symlink_metadata(&current).map_err(|failure| {
            if failure.kind() == ErrorKind::NotFound {
                "REFERENCE_MISSING"
            } else {
                "REFERENCE_METADATA_UNAVAILABLE"
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err("REFERENCE_SYMLINK");
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err("REFERENCE_PARENT_NOT_DIRECTORY");
        }
        last = Some(metadata);
    }
    last.ok_or("REFERENCE_METADATA_UNAVAILABLE")
}

#[cfg(unix)]
fn permission_bits(metadata: &Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn permission_bits(_metadata: &Metadata) -> Option<u32> {
    None
}
