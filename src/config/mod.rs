//! Proposed BXDL product configuration checks. This module neither implements
//! NIGO rules nor persists an instance or establishes runtime readiness.

mod preflight;
pub use preflight::{preflight, preflight_bytes};

use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Component, Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub instance_id: String,
    pub outcome: String,
    pub checks: Vec<Check>,
    pub config_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub status: String,
    pub reason_code: String,
    pub message: String,
}

impl Check {
    fn new(name: &str, status: &str, reason_code: &str, message: &str) -> Self {
        Self {
            name: name.into(),
            status: status.into(),
            reason_code: reason_code.into(),
            message: message.into(),
        }
    }
}

// Intentionally no Debug or public fields: credential references are private
// input. Serialize is used only to create normalized config bytes for storage,
// never to embed configuration or credential paths in public reports.
// Every field is required, including nested objects. Serde rejects duplicate
// known fields as well as aliases, unknown names, nulls, and incorrect types.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Instance {
    schema_version: i64,
    instance_id: String,
    role: String,
    node_id: String,
    chain_description: String,
    storage: Storage,
    secrets: Secrets,
    http: Address,
    p2p: Address,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Storage {
    backend: String,
    data_directory: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Secrets {
    validator_keystore: String,
    validator_password_file: String,
    tls_key_store: String,
    tls_key_password_file: String,
    tls_trust_store: String,
    tls_trust_password_file: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Address {
    address: String,
    port: i64,
}

/// Reads only the product JSON itself. Referenced file contents and filesystem
/// metadata are not inspected by this operation.
pub fn validate_file(path: &Path) -> Result<Report> {
    let (config, raw, _) = load(path)?;
    Ok(report(&config, &raw, "VALIDATED_PRODUCT_CONFIG"))
}

/// Validate unsaved product JSON and return a stable config document with
/// absolute references. `source` is the intended config location, not a file to
/// open. Neither it nor any referenced path is read, created, or changed.
/// Returned bytes contain private path references and must not be put in reports.
pub fn normalize_bytes(raw: &[u8], source: &Path) -> Result<Vec<u8>> {
    let source = resolve_source(source)?;
    let config = parse_bytes(raw, &source)?;
    normalized(&config, &source)
}

/// Import a bounded regular config using the same guarded read as validation.
/// Relative references retain the imported file's directory as their base.
/// Only the config is read; referenced contents and metadata are not inspected.
pub fn normalized_file(path: &Path) -> Result<Vec<u8>> {
    let (config, _, source) = load(path)?;
    normalized(&config, &source)
}

fn normalized(config: &Instance, source: &Path) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(config).map_err(|_| {
        error(
            "CONFIG_SCHEMA_INVALID",
            "Config cannot be represented as normalized product JSON.",
        )
    })?;
    bytes.push(b'\n');
    // Absolutizing a bounded relative reference can make it longer. Do not
    // return a document that the same product validator would reject on reload.
    parse_bytes(&bytes, source)?;
    Ok(bytes)
}

fn report(config: &Instance, raw: &[u8], outcome: &str) -> Report {
    Report {
        schema_version: 1,
        instance_id: config.instance_id.clone(),
        outcome: outcome.into(),
        config_sha256: hex::encode(Sha256::digest(raw)),
        checks: vec![Check::new(
            "productSchema",
            "PASS",
            "PRODUCT_SCHEMA_VALID",
            "Proposed BXDL product schema only; no NIGO validation or instance persistence performed.",
        )],
    }
}

fn resolve_source(path: &Path) -> Result<PathBuf> {
    if !path.to_str().is_some_and(valid_path_text) {
        return Err(error("CONFIG_PATH_INVALID", "Config path is invalid."));
    }
    Ok(if path.is_absolute() {
        clean_absolute(path)
    } else {
        let base = std::env::current_dir()
            .map_err(|_| error("CONFIG_PATH_INVALID", "Config path cannot be resolved."))?;
        clean_absolute(&base.join(path))
    })
}

fn load(path: &Path) -> Result<(Instance, Vec<u8>, PathBuf)> {
    let source = resolve_source(path)?;
    let before = fs::symlink_metadata(&source)
        .map_err(|_| error("CONFIG_READ_FAILED", "Config file cannot be inspected."))?;
    if !before.is_file() {
        return Err(error(
            "CONFIG_NOT_REGULAR",
            "Config must be a regular file, not a symlink or special file.",
        ));
    }
    if before.len() > MAX_CONFIG_BYTES as u64 {
        return Err(error("CONFIG_TOO_LARGE", "Config exceeds the size limit."));
    }
    let mut file = File::open(&source)
        .map_err(|_| error("CONFIG_READ_FAILED", "Config file cannot be read."))?;
    let opened = file
        .metadata()
        .map_err(|_| error("CONFIG_CHANGED", "Config changed while being opened."))?;
    if !opened.is_file() || !same_file(&before, &opened) {
        return Err(error(
            "CONFIG_CHANGED",
            "Config changed while being opened.",
        ));
    }
    let mut raw = Vec::new();
    (&mut file)
        .take((MAX_CONFIG_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|_| error("CONFIG_READ_FAILED", "Config file cannot be read."))?;
    if raw.len() > MAX_CONFIG_BYTES {
        return Err(error("CONFIG_TOO_LARGE", "Config exceeds the size limit."));
    }
    let after = file
        .metadata()
        .map_err(|_| error("CONFIG_CHANGED", "Config changed while being read."))?;
    if after.len() != opened.len() || after.modified().ok() != opened.modified().ok() {
        return Err(error("CONFIG_CHANGED", "Config changed while being read."));
    }
    let config = parse_bytes(&raw, &source)?;
    Ok((config, raw, source))
}

fn parse_bytes(raw: &[u8], source: &Path) -> Result<Instance> {
    if raw.len() > MAX_CONFIG_BYTES {
        return Err(error("CONFIG_TOO_LARGE", "Config exceeds the size limit."));
    }
    if std::str::from_utf8(raw).is_err() || !strict_json_shape(raw) {
        return Err(error(
            "CONFIG_JSON_INVALID",
            "Config must be one valid JSON document without duplicate or unknown fields.",
        ));
    }
    let mut config: Instance = serde_json::from_slice(raw).map_err(|_| {
        // Schema failures and strict-JSON failures retain their prior distinct
        // reason codes. Never expose the deserializer's input-bearing errors.
        error(
            "CONFIG_SCHEMA_INVALID",
            "Config contains missing, null, misplaced, or invalid field types.",
        )
    })?;
    validate(&mut config, source)?;
    Ok(config)
}

fn validate(config: &mut Instance, source: &Path) -> Result<()> {
    if config.schema_version != 1
        || config.role != "validator"
        || config.storage.backend != "rocksdb"
    {
        return Err(error(
            "CONFIG_SCHEMA_INVALID",
            "Explicit schemaVersion 1, validator role, and rocksdb backend are required.",
        ));
    }
    let instance = config.instance_id.as_bytes();
    if instance.is_empty()
        || instance.len() > 32
        || !instance[0].is_ascii_lowercase()
        || !instance
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return Err(error(
            "INSTANCE_ID_INVALID",
            "Instance ID must be 1-32 lowercase letters, digits, or hyphens and start with a letter.",
        ));
    }
    let node = config.node_id.as_bytes();
    if node.is_empty()
        || node.len() > 128
        || !node[0].is_ascii_alphanumeric()
        || !node
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(byte))
    {
        return Err(error(
            "NODE_ID_INVALID",
            "Node ID must use the bounded product identifier syntax; canonical identity remains unchecked.",
        ));
    }
    if config.http.address != "127.0.0.1" && config.http.address != "::1" {
        return Err(error(
            "HTTP_ADDRESS_INVALID",
            "HTTP address must be an explicit loopback address.",
        ));
    }
    if !valid_p2p_address(&config.p2p.address) {
        return Err(error(
            "P2P_ADDRESS_INVALID",
            "P2P address must be a non-wildcard unicast IP address.",
        ));
    }
    if !(1..=65535).contains(&config.http.port)
        || !(1..=65535).contains(&config.p2p.port)
        || config.http.port == config.p2p.port
    {
        return Err(error(
            "PORTS_INVALID",
            "HTTP and P2P require distinct explicit ports in the range 1-65535.",
        ));
    }
    let base = source
        .parent()
        .ok_or_else(|| error("CONFIG_PATH_INVALID", "Config path cannot be resolved."))?;
    for value in [
        &mut config.chain_description,
        &mut config.storage.data_directory,
        &mut config.secrets.validator_keystore,
        &mut config.secrets.validator_password_file,
        &mut config.secrets.tls_key_store,
        &mut config.secrets.tls_key_password_file,
        &mut config.secrets.tls_trust_store,
        &mut config.secrets.tls_trust_password_file,
    ] {
        if !valid_path_text(value) {
            return Err(error(
                "REFERENCE_PATH_INVALID",
                "Every file and data reference must be an explicit bounded path without control characters.",
            ));
        }
        let path = Path::new(value);
        let normalized = clean_absolute(&if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        });
        if normalized.parent().is_none() || normalized == source {
            return Err(error(
                "REFERENCE_PATH_INVALID",
                "Reference paths cannot identify the filesystem root or the product config itself.",
            ));
        }
        *value = normalized
            .to_str()
            .ok_or_else(|| {
                error(
                    "REFERENCE_PATH_INVALID",
                    "Reference path cannot be represented.",
                )
            })?
            .to_owned();
    }
    if std::iter::once(config.chain_description.as_str())
        .chain(secret_paths(config))
        .any(|reference| reference == config.storage.data_directory)
    {
        return Err(error(
            "REFERENCE_PATH_CONFLICT",
            "Data directory and referenced file paths must be distinct.",
        ));
    }
    Ok(())
}

fn valid_p2p_address(text: &str) -> bool {
    match text.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            !ip.is_unspecified() && !ip.is_multicast() && ip != Ipv4Addr::BROADCAST
        }
        Ok(IpAddr::V6(ip)) => match ip.to_ipv4_mapped() {
            Some(mapped) => {
                !mapped.is_unspecified() && !mapped.is_multicast() && mapped != Ipv4Addr::BROADCAST
            }
            None => !ip.is_unspecified() && !ip.is_multicast(),
        },
        Err(_) => false,
    }
}

fn valid_path_text(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('~')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
}

// Lexical normalization only: no canonicalize, reads, or symlink traversal for
// referenced paths during product validation. Parent components clamp at root.
fn clean_absolute(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn secret_paths(config: &Instance) -> [&str; 6] {
    [
        &config.secrets.validator_keystore,
        &config.secrets.validator_password_file,
        &config.secrets.tls_key_store,
        &config.secrets.tls_key_password_file,
        &config.secrets.tls_trust_store,
        &config.secrets.tls_trust_password_file,
    ]
}

#[cfg(unix)]
fn same_file(left: &Metadata, right: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file(_left: &Metadata, _right: &Metadata) -> bool {
    // Non-Unix platforms have no supported instance validation contract yet.
    false
}

fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}

// Preserve the v1 product contract's two validation phases: exact decoded key
// names / duplicates / JSON shape first, then typed required fields. Numeric
// tokens are checked lexically so valid numbers outside i64/f64 remain schema
// failures. Strings (including escape and surrogate handling) use serde_json.
fn strict_json_shape(raw: &[u8]) -> bool {
    let mut scan = JsonShape { raw, position: 0 };
    scan.value(0).is_some() && {
        scan.whitespace();
        scan.position == raw.len()
    }
}

struct JsonShape<'a> {
    raw: &'a [u8],
    position: usize,
}

impl JsonShape<'_> {
    fn whitespace(&mut self) {
        while self
            .raw
            .get(self.position)
            .is_some_and(|byte| b" \t\r\n".contains(byte))
        {
            self.position += 1;
        }
    }

    fn take(&mut self, byte: u8) -> Option<()> {
        self.whitespace();
        if self.raw.get(self.position) != Some(&byte) {
            return None;
        }
        self.position += 1;
        Some(())
    }

    fn value(&mut self, depth: usize) -> Option<()> {
        if depth > 8 {
            return None;
        }
        self.whitespace();
        match self.raw.get(self.position)? {
            b'{' => {
                self.position += 1;
                self.whitespace();
                if self.raw.get(self.position) == Some(&b'}') {
                    self.position += 1;
                    return Some(());
                }
                let mut seen = HashSet::new();
                loop {
                    self.whitespace();
                    let key = self.string()?;
                    if !known_json_key(&key) || !seen.insert(key) {
                        return None;
                    }
                    self.take(b':')?;
                    self.value(depth + 1)?;
                    self.whitespace();
                    if self.raw.get(self.position) == Some(&b'}') {
                        self.position += 1;
                        return Some(());
                    }
                    self.take(b',')?;
                }
            }
            b'"' => {
                self.string()?;
                Some(())
            }
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            b'n' => self.literal(b"null"),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None, // Arrays were never part of the v1 product JSON shape.
        }
    }

    fn string(&mut self) -> Option<String> {
        let start = self.position;
        if self.raw.get(self.position) != Some(&b'"') {
            return None;
        }
        self.position += 1;
        while let Some(byte) = self.raw.get(self.position) {
            match byte {
                b'\\' => self.position += 2,
                b'"' => {
                    self.position += 1;
                    return serde_json::from_slice(&self.raw[start..self.position]).ok();
                }
                _ => self.position += 1,
            }
        }
        None
    }

    fn literal(&mut self, expected: &[u8]) -> Option<()> {
        if !self.raw.get(self.position..)?.starts_with(expected) {
            return None;
        }
        self.position += expected.len();
        Some(())
    }

    fn number(&mut self) -> Option<()> {
        if self.raw.get(self.position) == Some(&b'-') {
            self.position += 1;
        }
        match self.raw.get(self.position)? {
            b'0' => self.position += 1,
            b'1'..=b'9' => self.digits()?,
            _ => return None,
        }
        if self.raw.get(self.position) == Some(&b'.') {
            self.position += 1;
            self.digits()?;
        }
        if self
            .raw
            .get(self.position)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.position += 1;
            if self
                .raw
                .get(self.position)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.position += 1;
            }
            self.digits()?;
        }
        Some(())
    }

    fn digits(&mut self) -> Option<()> {
        let start = self.position;
        while self.raw.get(self.position).is_some_and(u8::is_ascii_digit) {
            self.position += 1;
        }
        (self.position > start).then_some(())
    }
}

fn known_json_key(name: &str) -> bool {
    matches!(
        name,
        "schemaVersion"
            | "instanceId"
            | "role"
            | "nodeId"
            | "chainDescription"
            | "storage"
            | "backend"
            | "dataDirectory"
            | "secrets"
            | "validatorKeystore"
            | "validatorPasswordFile"
            | "tlsKeyStore"
            | "tlsKeyPasswordFile"
            | "tlsTrustStore"
            | "tlsTrustPasswordFile"
            | "http"
            | "p2p"
            | "address"
            | "port"
    )
}

#[cfg(test)]
mod tests;
