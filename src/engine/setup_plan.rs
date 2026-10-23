//! Read-only binding of an unsaved product draft to supplied development inputs.
//! These private workflow pins are not engine validation or runtime readiness.
use super::{Identity, Lock, fail, files, json, product, validate_lock};
use crate::{artifact, config, error::Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const MAX_REFERENCE: u64 = 4 * 1024 * 1024;
const MAX_PINS: usize = 128;
const ISSUER_PREFIX: &str = "nigo.protocol.native-assets.allowed-issuers[";

/// Private workflow storage only: never print reference paths or credential hashes.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prepared {
    pub identity: Identity,
    pub data_directory: PathBuf,
    pub backend: String,
    pub pins: Vec<Pin>,
    pub automatic_gc: bool,
    /// Distinct explicitly configured addresses, not proof of on-chain authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_count: Option<usize>,
}

/// Display-only input metadata. Kept out of Prepared so persisted plan hashes
/// and legacy workflow journals keep their original representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationMode {
    Open,
    Allowlisted,
    Unspecified,
    Unrecognized,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IssuerInput {
    Count(usize),
    Duplicate,
    Unrecognized,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RegistrationInput {
    pub mode: RegistrationMode,
    pub issuers: IssuerInput,
}

/// Describe only the bytes already pinned by the plan. This does not validate
/// policy eligibility, infer an engine capability, or persist new journal fields.
pub(crate) fn registration_input(native_path: &Path, pins: &[Pin]) -> Result<RegistrationInput> {
    let native = files::NativeInput::load(native_path)?;
    for input in [&native.config, &native.chain] {
        let matches = pins
            .iter()
            .filter(|pin| pin.path == input.path)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].sha256 != files::digest(&input.raw) {
            return Err(fail(
                "ENGINE_INPUT_CHANGED",
                "준비한 등록 정책 입력이 변경되었습니다.",
            ));
        }
    }
    let chain: Value = json::decode(&native.chain.raw).map_err(|_| invalid())?;
    let mode = match chain.get("nigo.protocol.native-assets.registration-mode") {
        None => RegistrationMode::Unspecified,
        Some(Value::String(value)) if value == "OPEN" => RegistrationMode::Open,
        Some(Value::String(value)) if value == "ALLOWLISTED" => RegistrationMode::Allowlisted,
        _ => RegistrationMode::Unrecognized,
    };
    let issuers = match configured_issuers(&chain) {
        None => IssuerInput::Unrecognized,
        Some(count) => {
            let supplied = chain
                .as_object()
                .ok_or_else(invalid)?
                .keys()
                .filter(|key| key.starts_with(ISSUER_PREFIX))
                .count();
            if supplied != count {
                IssuerInput::Duplicate
            } else {
                IssuerInput::Count(count)
            }
        }
    };
    native.config.recheck()?;
    native.chain.recheck()?;
    Ok(RegistrationInput { mode, issuers })
}

/// `package` must originate from `artifact::verify` with the caller's explicit
/// trust policy. A caller-constructed Report cannot authenticate any package.
/// The proposed product file is not opened or created. The caller owns its bytes,
/// local metadata preflight, archive/key pins, destination checks and consent.
pub fn prepare(
    product_raw: &[u8],
    product_path: &Path,
    native_path: &Path,
    lock_path: &Path,
    package: &artifact::Report,
) -> Result<Prepared> {
    let source = files::absolute(product_path)?;
    files::check_path(&source, true)?;
    let normalized = config::normalize_bytes(product_raw, &source)?;
    let document: Value = json::decode(&normalized).map_err(|_| invalid())?;
    let native = files::NativeInput::load(native_path)?;
    product::check_document(&document, &source, &native)?;
    let lock_input = files::Input::read(lock_path, 65_536, false)?;
    let lock: Lock = json::decode(&lock_input.raw)
        .map_err(|_| fail("ENGINE_LOCK_INVALID", "개발 엔진 lock 형식을 확인하세요."))?;
    validate_lock(&lock)?;
    check_package(package, &lock)?;
    if native.value.backend != "rocksdb" || native.references.len() > MAX_PINS - 3 {
        return Err(invalid());
    }
    let automatic_gc = match native.value.node.get("nigo.storage.gc.automatic.enabled") {
        None | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(Value::String(value)) if value == "false" => false,
        Some(Value::String(value)) if value == "true" => true,
        _ => return Err(invalid()),
    };
    let chain: Value = json::decode(&native.chain.raw).map_err(|_| invalid())?;
    let issuer_count = configured_issuers(&chain);
    let references = native
        .references
        .iter()
        .map(|path| files::Input::read(path, MAX_REFERENCE, false))
        .collect::<Result<Vec<_>>>()?;
    let inputs = [&lock_input, &native.config, &native.chain]
        .into_iter()
        .chain(references.iter())
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut pins = Vec::new();
    for input in &inputs {
        if seen.insert(input.path.clone()) {
            pins.push(Pin {
                path: input.path.clone(),
                sha256: files::digest(&input.raw),
            });
        }
    }
    // Keep all original observations until every reference has been read.
    for input in inputs {
        input.recheck()?;
    }
    native.recheck()?;
    Ok(Prepared {
        identity: lock.expected,
        data_directory: native.data,
        backend: native.value.backend,
        pins,
        automatic_gc,
        issuer_count,
    })
}

pub(crate) fn pin(path: &Path) -> Result<Pin> {
    let input = files::Input::read(path, MAX_REFERENCE, false)?;
    Ok(Pin {
        path: input.path,
        sha256: files::digest(&input.raw),
    })
}

/// Recheck private pins without opening a DB, running Java or repairing inputs.
pub fn recheck(pins: &[Pin]) -> Result<()> {
    if pins.is_empty() || pins.len() > MAX_PINS {
        return Err(invalid_pins());
    }
    let mut seen = BTreeSet::new();
    let mut inputs = Vec::new();
    for pin in pins {
        if !pin.path.is_absolute()
            || files::absolute(&pin.path)? != pin.path
            || !super::hash(&pin.sha256)
            || !seen.insert(pin.path.clone())
        {
            return Err(invalid_pins());
        }
        let input = files::Input::read(&pin.path, MAX_REFERENCE, false)?;
        if files::digest(&input.raw) != pin.sha256 {
            return Err(fail(
                "ENGINE_INPUT_CHANGED",
                "준비한 설정·신뢰 자료 또는 키 참조가 변경되었습니다.",
            ));
        }
        inputs.push(input);
    }
    for input in inputs {
        input.recheck()?;
    }
    Ok(())
}

/// Require either an accepted external executor profile or the exact current
/// CLI. This planning check never executes the package worker.
pub(crate) fn check_cli(package: &artifact::Report) -> Result<()> {
    let mismatch = || {
        fail(
            "SETUP_CLI_MISMATCH",
            "현재 BXDL과 같은 실행 파일이 포함된 패키지를 선택하세요.",
        )
    };
    artifact::validate_manifest(&package.manifest, true).map_err(|_| mismatch())?;
    // A reviewed legacy executor owns its own worker/journal contract. The
    // management CLI need not be the bytes shipped inside that immutable bundle.
    if super::instance::executor::supports_package(package) {
        return Ok(());
    }
    let entry = package
        .manifest
        .files
        .iter()
        .find(|entry| entry.path == "bin/bxdl")
        .ok_or_else(mismatch)?;
    // Match the service worker's existing bounded executable input contract.
    if entry.size > 64 * 1024 * 1024 {
        return Err(mismatch());
    }
    let current = std::env::current_exe().map_err(|_| mismatch())?;
    let binary = files::Binary::open(&current, &entry.sha256).map_err(|_| mismatch())?;
    if fs::symlink_metadata(&current)
        .map_err(|_| mismatch())?
        .len()
        != entry.size
    {
        return Err(mismatch());
    }
    binary.recheck().map_err(|_| mismatch())
}

fn check_package(package: &artifact::Report, lock: &Lock) -> Result<()> {
    let mismatch = || {
        fail(
            "ENGINE_SETUP_PACKAGE_MISMATCH",
            "패키지의 엔진·Java·계약 identity가 신뢰 lock과 일치하지 않습니다.",
        )
    };
    let m = &package.manifest;
    artifact::validate_manifest(m, true).map_err(|_| mismatch())?;
    let jar = m
        .files
        .iter()
        .find(|entry| entry.path == "engine/nigo-node.jar")
        .ok_or_else(mismatch)?;
    if m.platform.os != "darwin"
        || m.platform.arch != "arm64"
        || m.platform.java_major != lock.expected.java.required_major
        || m.engine.revision != lock.expected.source.commit
        || m.engine.contract_revision != lock.expected.contract.fingerprint
        || m.engine.jar_sha256 != lock.jar_sha256
        || jar.size != lock.jar_size_bytes
        || m.runtime.java_sha256 != lock.java_sha256
        || !matches!(
            package.authenticity.as_str(),
            "verified-external-ed25519" | "unsigned-development"
        )
        || !super::hash(&package.archive_sha256)
        || !super::hash(&package.manifest_sha256)
        || package.files_verified != m.files.len()
        || package.bytes_verified != m.files.iter().map(|entry| entry.size).sum::<u64>()
    {
        return Err(mismatch());
    }
    Ok(())
}

fn configured_issuers(chain: &Value) -> Option<usize> {
    let object = chain.as_object()?;
    let mut indices = BTreeSet::new();
    let mut addresses = BTreeSet::new();
    for (key, value) in object {
        if !key.starts_with("nigo.protocol.native-assets.allowed-issuers") {
            continue;
        }
        let index = key.strip_prefix(ISSUER_PREFIX)?.strip_suffix(']')?;
        let number = index.parse::<usize>().ok()?;
        if number.to_string() != index || !indices.insert(number) {
            return None;
        }
        let address = value.as_str()?.strip_prefix("0x")?;
        if address.len() != 40
            || !address.bytes().all(|b| b.is_ascii_hexdigit())
            || address.bytes().all(|b| b == b'0')
        {
            return None;
        }
        addresses.insert(address.to_ascii_lowercase());
    }
    if indices.iter().copied().ne(0..indices.len()) {
        return None;
    }
    Some(addresses.len())
}

fn invalid() -> crate::error::BxdlError {
    fail(
        "ENGINE_CONFIG_INVALID",
        "명시적인 native 설정과 참조 자료를 확인하세요.",
    )
}
fn invalid_pins() -> crate::error::BxdlError {
    fail(
        "ENGINE_SETUP_PINS_INVALID",
        "저장된 입력 고정 기록이 유효하지 않습니다.",
    )
}

#[cfg(test)]
mod tests;
