//! Bind a saved product draft to an explicitly supplied native QBFT config.
//! Missing network membership, credentials and policy are never synthesized.
use super::{ColdResult, Options, Report, execute, fail, files, json};
use crate::{config, error::Result, setup::paths};
use serde::Serialize;
use serde_json::Value;
use std::{ffi::OsString, fs, net::IpAddr, os::unix::fs::MetadataExt, path::Path};

const QBFT: &str = "nigo.protocol.consensus.qbft.node.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductReport {
    pub outcome: &'static str,
    pub product: config::Report,
    pub configuration_binding: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<Report>,
}

pub fn preflight_product(
    options: &Options,
    instance: &Path,
    native_config: &Path,
) -> Result<ProductReport> {
    let product = ProductInput::load(instance)?;
    if product.local.outcome == "FAIL" {
        product.file.recheck()?;
        return Ok(ProductReport {
            outcome: "FAIL",
            product: product.local,
            configuration_binding: "NOT_CHECKED",
            engine: None,
        });
    }
    let engine = execute(options, Some(native_config), Some(&product))?;
    Ok(ProductReport {
        outcome: "INCOMPLETE",
        product: product.local,
        configuration_binding: "MATCHED",
        engine: Some(engine),
    })
}

pub(super) struct ProductInput {
    file: files::Input,
    document: Value,
    local: config::Report,
}
impl ProductInput {
    fn load(path: &Path) -> Result<Self> {
        let file = files::Input::read(path, 262_144, false)?;
        let normalized = config::normalize_bytes(&file.raw, &file.path)?;
        let document: Value = json::decode(&normalized).map_err(|_| mismatch())?;
        // The metadata report must describe these same product bytes. The
        // stronger Input guard is retained across every external invocation.
        let local = config::preflight(&file.path)?;
        if local.config_sha256 != files::digest(&file.raw) {
            return Err(changed());
        }
        file.recheck()?;
        Ok(Self {
            file,
            document,
            local,
        })
    }

    pub(super) fn recheck(&self) -> Result<()> {
        self.file.recheck()?;
        let local = config::preflight(&self.file.path)?;
        if local.config_sha256 != self.local.config_sha256 || local.outcome == "FAIL" {
            return Err(changed());
        }
        Ok(())
    }

    pub(super) fn check(&self, native: &files::NativeInput) -> Result<()> {
        self.recheck()?;
        let p = &self.document;
        let node = &native.value.node;
        let get = |key: &str| node.get(key).ok_or_else(mismatch);
        let chain: Value = json::decode(&native.chain.raw).map_err(|_| mismatch())?;
        if chain["nigo.protocol.consensus.protocol"] != "QBFT"
            || p["role"] != "validator"
            || text(get(&format!("{QBFT}role"))?)? != "VALIDATOR"
            || text(get(&format!("{QBFT}transport-security-scheme"))?)? != "MTLS"
            || text(&p["storage"]["backend"])? != native.value.backend
            || !same_path(&p["chainDescription"], &native.chain.path)?
            || !same_path(&p["storage"]["dataDirectory"], &native.data)?
            || paths::overlaps(&self.file.path, &native.data).map_err(|_| mismatch())?
        {
            return Err(mismatch());
        }
        let expected_id = canonical_hex(text(&p["nodeId"])?, 64)?;
        if expected_id != canonical_hex(text(get(&format!("{QBFT}node-id"))?)?, 64)? {
            return Err(mismatch());
        }
        for (section, address, port) in [
            (
                "http",
                "server.address".to_owned(),
                "server.port".to_owned(),
            ),
            (
                "p2p",
                format!("{QBFT}listen-host"),
                format!("{QBFT}listen-port"),
            ),
        ] {
            if ip(&p[section]["address"])? != ip(get(&address)?)?
                || port_value(&p[section]["port"])? != port_value(get(&port)?)?
            {
                return Err(mismatch());
            }
        }
        for (product, property) in [
            ("validatorKeystore", "keystore-path"),
            ("validatorPasswordFile", "keystore-password-file"),
            ("tlsKeyStore", "mtls-key-store-path"),
            ("tlsKeyPasswordFile", "mtls-key-store-password-file"),
            ("tlsTrustStore", "mtls-trust-store-path"),
            ("tlsTrustPasswordFile", "mtls-trust-store-password-file"),
        ] {
            // NativeInput already resolves these against the native config's
            // own directory; product normalization used its separate base.
            let path = Path::new(text(get(&format!("{QBFT}{property}"))?)?);
            if !same_path(&p["secrets"][product], path)? {
                return Err(mismatch());
            }
        }
        Ok(())
    }

    pub(super) fn check_result(&self, result: &ColdResult) -> Result<()> {
        let Some((node, validator)) = result.node_identity.split_once(':') else {
            return Err(mismatch());
        };
        if canonical_hex(node, 64)? != canonical_hex(text(&self.document["nodeId"])?, 64)?
            || canonical_hex(validator, 40).is_err()
            || result.backend != text(&self.document["storage"]["backend"])?
        {
            return Err(mismatch());
        }
        Ok(())
    }
}

fn text(value: &Value) -> Result<&str> {
    value.as_str().ok_or_else(mismatch)
}
fn same_path(value: &Value, path: &Path) -> Result<bool> {
    let product = Path::new(text(value)?);
    if !paths::same(product, path).map_err(|_| mismatch())? {
        return Ok(false);
    }
    // The setup guard conservatively folds Mac names to reject collisions.
    // A possible collision is not proof of identity: existing files must have
    // the same inode, and missing data suffixes must match exactly beneath it.
    Ok(path_identity(product)? == path_identity(path)?)
}
fn path_identity(path: &Path) -> Result<(u64, u64, Vec<OsString>)> {
    let mut current = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) if !metadata.file_type().is_symlink() => {
                if !missing.is_empty() && !metadata.is_dir() {
                    return Err(mismatch());
                }
                return Ok((metadata.dev(), metadata.ino(), missing));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.file_name().ok_or_else(mismatch)?.to_owned());
                current = current.parent().ok_or_else(mismatch)?;
            }
            _ => return Err(mismatch()),
        }
    }
}
fn ip(value: &Value) -> Result<IpAddr> {
    text(value)?.parse().map_err(|_| mismatch())
}
fn port_value(value: &Value) -> Result<u16> {
    let port = if let Some(value) = value.as_str() {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(mismatch());
        }
        value.parse::<u16>().map_err(|_| mismatch())?
    } else {
        u16::try_from(value.as_u64().ok_or_else(mismatch)?).map_err(|_| mismatch())?
    };
    if port == 0 {
        return Err(mismatch());
    }
    Ok(port)
}
fn canonical_hex(value: &str, length: usize) -> Result<String> {
    let raw = value.strip_prefix("0x").ok_or_else(mismatch)?;
    if raw.len() != length || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(mismatch());
    }
    Ok(raw.to_ascii_lowercase())
}
fn mismatch() -> crate::error::BxdlError {
    fail(
        "ENGINE_PRODUCT_MISMATCH",
        "제품 설정과 명시적인 QBFT validator 설정의 ID·경로·주소·포트·키 참조를 확인하세요.",
    )
}
fn changed() -> crate::error::BxdlError {
    fail(
        "ENGINE_INPUT_CHANGED",
        "검사 중 제품 설정 또는 참조 자료의 상태가 변경되었습니다.",
    )
}
