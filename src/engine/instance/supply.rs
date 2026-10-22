//! Private, instance-owned copies of package authentication inputs. Original
//! paths remain provenance; V2 runtime verification never opens those paths.
use super::{Binding, Pin, RegisterOptions, binding_schema_valid, corrupt, files, precondition};
use crate::{artifact, engine::instance_fs::Control, engine::setup_plan, error::Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, os::unix::fs::MetadataExt, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Provenance {
    pub(super) archive: Pin,
    pub(super) lock: Pin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) public_key: Option<Pin>,
}

/// Run only after all original inputs have passed cold verification and after
/// this operation has durably claimed a fresh control root. Never upgrades V1.
pub(super) fn retain(
    root: &Control,
    binding: &mut Binding,
    native: &files::NativeInput,
) -> Result<()> {
    if binding.schema_version != 1 || binding.supply.is_some() {
        return Err(corrupt());
    }
    let pin = |path: &Path| -> Result<Pin> {
        let mut matching = binding.pins.iter().filter(|pin| pin.path == path);
        let result = matching.next().ok_or_else(corrupt)?.clone();
        if matching.next().is_some() {
            return Err(corrupt());
        }
        Ok(result)
    };
    let provenance = Provenance {
        archive: Pin {
            path: binding.archive.clone(),
            sha256: binding.archive_sha256.clone(),
        },
        lock: pin(&binding.lock)?,
        public_key: binding.public_key.as_deref().map(pin).transpose()?,
    };
    // A supply reference must not double as an operational config/credential.
    // Those references intentionally remain live and are not relocated here.
    for source in [&provenance.archive, &provenance.lock]
        .into_iter()
        .chain(provenance.public_key.iter())
    {
        if [&binding.product, &native.config.path, &native.chain.path]
            .into_iter()
            .chain(native.references.iter())
            .any(|path| *path == source.path)
        {
            return Err(super::fail(
                "INSTANCE_SUPPLY_REFERENCE_OVERLAP",
                "배포 자료와 운용 설정·자격 증명은 서로 다른 파일로 제공하세요.",
            ));
        }
    }
    let mut inputs = vec![
        (
            "package.tar.gz",
            provenance.archive.path.as_path(),
            provenance.archive.sha256.as_str(),
            artifact::MAX_ARCHIVE_BYTES,
        ),
        (
            "engine-lock.json",
            provenance.lock.path.as_path(),
            provenance.lock.sha256.as_str(),
            65_536,
        ),
    ];
    if let Some(key) = &provenance.public_key {
        inputs.push(("public-key.pem", &key.path, &key.sha256, 65_536));
    }
    root.retain_supply(&inputs)?;
    let trust = root.path().join("trust");
    binding.archive = trust.join("package.tar.gz");
    binding.lock = trust.join("engine-lock.json");
    binding.public_key = provenance
        .public_key
        .as_ref()
        .map(|_| trust.join("public-key.pem"));
    for pin in &mut binding.pins {
        if pin.path == provenance.lock.path {
            pin.path = binding.lock.clone();
        }
        if provenance
            .public_key
            .as_ref()
            .is_some_and(|key| pin.path == key.path)
        {
            pin.path = binding.public_key.clone().ok_or_else(corrupt)?;
        }
    }
    binding.schema_version = 2;
    binding.supply = Some(provenance);
    validate_layout(root.path(), binding)?;
    check_files(binding)?;
    binding.check_pins()?;
    root.recheck()
}

/// Pure metadata validation: do not stat or open any original source path.
/// It is safe to call this while inspecting an unavailable original source.
pub(super) fn validate_layout(instance: &Path, binding: &Binding) -> Result<()> {
    if !binding_schema_valid(binding) {
        return Err(corrupt());
    }
    let Some(origin) = &binding.supply else {
        return Ok(());
    };
    let trust = instance.join("trust");
    if binding.pins.is_empty() || binding.pins.len() > 128 {
        return Err(corrupt());
    }
    if binding.archive != trust.join("package.tar.gz")
        || binding.lock != trust.join("engine-lock.json")
        || binding.public_key
            != origin
                .public_key
                .as_ref()
                .map(|_| trust.join("public-key.pem"))
        || origin.archive.sha256 != binding.archive_sha256
        || !super::super::hash(&binding.manifest_sha256)
    {
        return Err(corrupt());
    }
    let mut origin_paths = BTreeSet::new();
    for pin in [&origin.archive, &origin.lock]
        .into_iter()
        .chain(origin.public_key.iter())
    {
        if !valid_pin(pin)
            || !origin_paths.insert(&pin.path)
            || pin.path.starts_with(instance)
            || instance.starts_with(&pin.path)
            || pin.path.starts_with(&binding.package)
            || binding.package.starts_with(&pin.path)
            || pin.path.starts_with(&binding.data_directory)
            || binding.data_directory.starts_with(&pin.path)
        {
            return Err(corrupt());
        }
    }
    let mut actual_paths = BTreeSet::new();
    for pin in &binding.pins {
        if !valid_pin(pin) || !actual_paths.insert(&pin.path) || origin_paths.contains(&pin.path) {
            return Err(corrupt());
        }
    }
    for (path, sha) in std::iter::once((&binding.lock, &origin.lock.sha256)).chain(
        binding
            .public_key
            .iter()
            .zip(origin.public_key.iter().map(|p| &p.sha256)),
    ) {
        if !binding
            .pins
            .iter()
            .any(|pin| &pin.path == path && &pin.sha256 == sha)
        {
            return Err(corrupt());
        }
    }
    Ok(())
}
fn valid_pin(pin: &Pin) -> bool {
    pin.path.is_absolute()
        && files::absolute(&pin.path).ok().as_ref() == Some(&pin.path)
        && super::super::hash(&pin.sha256)
}

pub(super) fn is_managed(binding: &Binding, path: &Path) -> bool {
    binding.supply.is_some()
        && (path == binding.archive
            || path == binding.lock
            || binding.public_key.as_deref() == Some(path))
}

/// The three fixed copies must remain private, regular and unlinked. Validation
/// of their hashes/authentication follows in check_pins and artifact::verify.
pub(super) fn check_files(binding: &Binding) -> Result<()> {
    if binding.supply.is_none() {
        return Ok(());
    }
    let trust = binding.archive.parent().ok_or_else(corrupt)?;
    let instance = trust.parent().ok_or_else(corrupt)?;
    validate_layout(instance, binding)?;
    files::check_path(trust, false)?;
    let directory = fs::symlink_metadata(trust).map_err(|_| corrupt())?;
    if !directory.is_dir() || directory.mode() & 0o7777 != 0o700 {
        return Err(corrupt());
    }
    let expected_names: BTreeSet<_> = ["package.tar.gz", "engine-lock.json"]
        .into_iter()
        .chain(binding.public_key.as_ref().map(|_| "public-key.pem"))
        .map(std::ffi::OsString::from)
        .collect();
    let actual_names: BTreeSet<_> = fs::read_dir(trust)
        .map_err(|_| corrupt())?
        .take(expected_names.len() + 1)
        .map(|entry| entry.map(|entry| entry.file_name()).map_err(|_| corrupt()))
        .collect::<Result<_>>()?;
    if actual_names != expected_names {
        return Err(corrupt());
    }
    for path in [&binding.archive, &binding.lock]
        .into_iter()
        .chain(binding.public_key.iter())
    {
        files::check_path(path, false)?;
        let metadata = fs::symlink_metadata(path).map_err(|_| corrupt())?;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.mode() & 0o7777 != 0o600 {
            return Err(corrupt());
        }
    }
    Ok(())
}

/// Request identity is lexical: original materials may be absent after a V2
/// registration. Only copies and live configuration paths are actually opened.
pub(super) fn check_request(binding: &Binding, options: &RegisterOptions) -> Result<()> {
    let (archive, lock, key) = match &binding.supply {
        Some(origin) => (
            &origin.archive.path,
            &origin.lock.path,
            origin.public_key.as_ref().map(|p| &p.path),
        ),
        None => (&binding.archive, &binding.lock, binding.public_key.as_ref()),
    };
    if binding.package != files::absolute(&options.package)?
        || *archive != files::absolute(&options.archive)?
        || *lock != files::absolute(&options.lock)?
        || key.cloned()
            != options
                .public_key
                .as_deref()
                .map(files::absolute)
                .transpose()?
        || binding.allow_unsigned_development != options.allow_unsigned_development
        || binding.product != files::absolute(&options.product)?
        || binding.native != files::absolute(&options.native)?
    {
        return Err(precondition());
    }
    Ok(())
}

/// Called only after check_pins read/hash-checked actual managed bytes. A V2
/// source pin is projected from that verified pin, not trusted provenance text.
pub(super) fn source_pins(binding: &Binding) -> Result<Vec<setup_plan::Pin>> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for actual in &binding.pins {
        let source = match &binding.supply {
            Some(origin) if actual.path == binding.lock => &origin.lock,
            Some(origin) if binding.public_key.as_ref() == Some(&actual.path) => {
                origin.public_key.as_ref().ok_or_else(corrupt)?
            }
            _ => actual,
        };
        if actual.sha256 != source.sha256 || !valid_pin(source) || !seen.insert(source.path.clone())
        {
            return Err(corrupt());
        }
        result.push(setup_plan::Pin {
            path: source.path.clone(),
            sha256: actual.sha256.clone(),
        });
    }
    Ok(result)
}
