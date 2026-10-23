//! Explicit public-network to RC1 native configuration preparation.
//! Never reads credential contents, starts Java, or claims runtime readiness.
use super::{parse_bytes, secret_paths};
use crate::{
    error::{BxdlError, Result},
    setup::paths,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    net::IpAddr,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const LIMIT: usize = 262_144;
const PREFIX: &str = "nigo.protocol.consensus.qbft.node.";
const MEMBERS: &str = "nigo.protocol.consensus.qbft.validators";
#[cfg(target_os = "macos")]
const FLAGS: i32 = 0x100 | 0x4;
#[cfg(target_os = "linux")]
const FLAGS: i32 = 0x20000 | 0x800;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StoreType {
    #[serde(rename = "PKCS12")]
    Pkcs12,
    #[serde(rename = "JKS")]
    Jks,
}
impl StoreType {
    fn text(self) -> &'static str {
        match self {
            Self::Pkcs12 => "PKCS12",
            Self::Jks => "JKS",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum NetworkMode {
    #[serde(rename = "new-network")]
    NewNetwork,
    #[serde(rename = "join-network")]
    JoinNetwork,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOptions {
    pub key_store_type: StoreType,
    pub trust_store_type: StoreType,
    pub mode: NetworkMode,
    pub consensus_auto_start: bool,
    pub archive_enabled: bool,
}
impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            key_store_type: StoreType::Pkcs12,
            trust_store_type: StoreType::Pkcs12,
            mode: NetworkMode::NewNetwork,
            consensus_auto_start: true,
            archive_enabled: false,
        }
    }
}
impl RuntimeOptions {
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub schema_version: u32,
    pub outcome: &'static str,
    pub instance_id: String,
    pub node_id: String,
    pub validator_id: String,
    pub validator_count: usize,
    pub peer_count: usize,
    pub http_address: String,
    pub http_port: u16,
    pub p2p_address: String,
    pub p2p_port: u16,
    pub transaction_permission: String,
    pub options: RuntimeOptions,
    pub key_and_certificate_identity: &'static str,
    pub network_and_quorum: &'static str,
    pub engine_configuration: &'static str,
}
// Private checkpoint input: these fields must never be included in user-facing reports.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputPin {
    pub path: PathBuf,
    sha256: String,
    stamp: Stamp,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Stamp {
    fn of(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            mode: m.mode(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}
struct Input {
    pin: InputPin,
    raw: Vec<u8>,
}
impl Input {
    fn read(path: &Path) -> Result<Self> {
        let path = super::resolve_source(path).map_err(|_| invalid())?;
        paths::same(&path, &path).map_err(|_| invalid())?;
        let before = fs::symlink_metadata(&path).map_err(|_| invalid())?;
        if !before.is_file()
            || before.len() == 0
            || before.len() > LIMIT as u64
            || before.mode() & 0o022 != 0
        {
            return Err(invalid());
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(FLAGS)
            .open(&path)
            .map_err(|_| invalid())?;
        let stamp = Stamp::of(&before);
        if Stamp::of(&file.metadata().map_err(|_| invalid())?) != stamp {
            return Err(changed());
        }
        let mut raw = Vec::new();
        #[cfg(test)]
        tests::record_content_read(&path);
        (&mut file)
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut raw)
            .map_err(|_| invalid())?;
        if raw.len() as u64 != stamp.size {
            return Err(changed());
        }
        check_stamp(&path, &file, &stamp)?;
        Ok(Self {
            pin: InputPin {
                path,
                sha256: hex::encode(Sha256::digest(&raw)),
                stamp,
            },
            raw,
        })
    }
}
fn check_stamp(path: &Path, file: &File, stamp: &Stamp) -> Result<()> {
    paths::same(path, path).map_err(|_| changed())?;
    if Stamp::of(&file.metadata().map_err(|_| changed())?) != *stamp
        || Stamp::of(&fs::symlink_metadata(path).map_err(|_| changed())?) != *stamp
    {
        return Err(changed());
    }
    Ok(())
}
/// Verify exact document bytes AND file identity. A same-byte replacement is a changed input.
pub fn verify_pins(pins: &[InputPin]) -> Result<()> {
    if pins.len() != 3 {
        return Err(changed());
    }
    let mut seen = BTreeSet::new();
    // Check every stored identity before reading any document again. A public
    // path replaced with a credential/data file must not be opened for hashing.
    for pin in pins {
        paths::same(&pin.path, &pin.path).map_err(|_| changed())?;
        let metadata = fs::symlink_metadata(&pin.path).map_err(|_| changed())?;
        if !seen.insert(&pin.path) || Stamp::of(&metadata) != pin.stamp {
            return Err(changed());
        }
    }
    for pin in pins {
        if Input::read(&pin.path).map_err(|_| changed())?.pin != *pin {
            return Err(changed());
        }
    }
    Ok(())
}
pub struct Prepared {
    /// Private native output bytes and paths, never part of the safe summary.
    pub native_bytes: Vec<u8>,
    pub summary: Summary,
    pub input_pins: Vec<InputPin>,
    pub referenced_paths: Vec<PathBuf>,
    secret_stamps: Vec<(PathBuf, Stamp)>,
}
impl Prepared {
    pub fn recheck(&self) -> Result<()> {
        verify_pins(&self.input_pins)?;
        for (path, stamp) in &self.secret_stamps {
            paths::same(path, path).map_err(|_| changed())?;
            if Stamp::of(&fs::symlink_metadata(path).map_err(|_| changed())?) != *stamp {
                return Err(changed());
            }
        }
        for path in &self.referenced_paths {
            paths::same(path, path).map_err(|_| changed())?;
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Network {
    schema_version: u32,
    kind: String,
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Node {
    node_id: String,
    validator_id: String,
    host: String,
    port: u16,
    transaction_permission: String,
    credential_fingerprints: Vec<String>,
    #[serde(default)]
    revoked_credential_fingerprints: Vec<String>,
}
fn decode<T: serde::de::DeserializeOwned>(raw: &[u8]) -> Result<T> {
    crate::artifact::decode_strict_json(raw).map_err(|_| invalid())
}
fn canonical_hex(text: &str, count: usize) -> Result<String> {
    if text.len() != count + 2
        || !text.starts_with("0x")
        || !text[2..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    Ok(text.to_ascii_lowercase())
}
fn host(text: &str) -> Result<String> {
    if let Ok(ip) = text.parse::<IpAddr>() {
        if !super::valid_p2p_address(text) {
            return Err(invalid());
        }
        return Ok(match ip {
            IpAddr::V6(v) => v
                .to_ipv4_mapped()
                .map_or_else(|| v.to_string(), |a| a.to_string()),
            v => v.to_string(),
        });
    }
    let name = text.strip_suffix('.').unwrap_or(text);
    if name.is_empty()
        || name.len() > 253
        || !name.split('.').all(|s| {
            !s.is_empty()
                && s.len() <= 63
                && s.as_bytes()[0].is_ascii_alphanumeric()
                && s.as_bytes()[s.len() - 1].is_ascii_alphanumeric()
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(invalid());
    }
    Ok(name.to_ascii_lowercase())
}
fn chain_members(raw: &[u8]) -> Result<BTreeSet<String>> {
    let values: BTreeMap<String, Value> = decode(raw)?;
    if values
        .get("nigo.protocol.consensus.protocol")
        .and_then(Value::as_str)
        != Some("QBFT")
    {
        return Err(invalid());
    }
    let mut members = BTreeMap::new();
    for (key, value) in &values {
        if !(value.is_string() || value.is_boolean() || value.is_number())
            || value.as_str().is_some_and(|v| v.contains("${"))
        {
            return Err(invalid());
        }
        if key.starts_with(MEMBERS) {
            let suffix = key.strip_prefix(MEMBERS).ok_or_else(invalid)?;
            let index = suffix
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .ok_or_else(invalid)?;
            let number = index.parse::<usize>().map_err(|_| invalid())?;
            if number.to_string() != index || number >= 128 {
                return Err(invalid());
            }
            members.insert(
                number,
                canonical_hex(value.as_str().ok_or_else(invalid)?, 40)?,
            );
        }
    }
    if members.is_empty() || members.keys().enumerate().any(|(n, key)| n != *key) {
        return Err(invalid());
    }
    let unique = members.values().cloned().collect::<BTreeSet<_>>();
    if unique.len() != members.len() {
        return Err(invalid());
    }
    Ok(unique)
}

/// Only public selection fields. Credential fingerprints stay inside validation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeChoice {
    pub node_id: String,
    pub validator_id: String,
    pub host: String,
    pub port: u16,
    pub transaction_permission: String,
}

/// Validate approved public chain/network documents without requiring a local
/// identity yet. The caller checks these paths against partial product inputs
/// before calling; no credentials are opened by this function.
pub fn node_choices(chain_path: &Path, network_path: &Path) -> Result<Vec<NodeChoice>> {
    if paths::overlaps(chain_path, network_path)? {
        return Err(input_conflict());
    }
    let chain = Input::read(chain_path)?;
    let network = Input::read(network_path)?;
    let value = validated_network(&chain.raw, &network.raw)?;
    for input in [&chain, &network] {
        if Input::read(&input.pin.path)?.pin != input.pin {
            return Err(changed());
        }
    }
    Ok(value
        .nodes
        .into_iter()
        .map(|node| NodeChoice {
            node_id: node.node_id,
            validator_id: node.validator_id,
            host: node.host,
            port: node.port,
            transaction_permission: node.transaction_permission,
        })
        .collect())
}

fn validated_network(chain_raw: &[u8], network_raw: &[u8]) -> Result<Network> {
    let membership = chain_members(chain_raw)?;
    let mut value: Network = decode(network_raw)?;
    if value.schema_version != 1
        || value.kind != "BXDL_QBFT_NETWORK"
        || value.nodes.is_empty()
        || value.nodes.len() > 128
    {
        return Err(input_reason(
            "네트워크 자료는 schemaVersion 1의 BXDL_QBFT_NETWORK이며 validator 1~128개를 포함해야 합니다.",
        ));
    }
    let mut node_ids = BTreeSet::new();
    let mut validator_ids = BTreeSet::new();
    let mut endpoints = BTreeSet::new();
    let mut pins = BTreeSet::new();
    for node in &mut value.nodes {
        node.node_id = canonical_hex(&node.node_id, 64)
            .map_err(|_| input_reason("노드 ID는 0x로 시작하는 32바이트 16진수여야 합니다."))?;
        node.validator_id = canonical_hex(&node.validator_id, 40).map_err(|_| {
            input_reason("Validator ID는 0x로 시작하는 20바이트 16진수여야 합니다.")
        })?;
        node.host = host(&node.host).map_err(|_| {
            input_reason("Peer 주소는 유효한 unicast IP 또는 DNS 이름이어야 합니다.")
        })?;
        if node.port == 0
            || !matches!(
                node.transaction_permission.as_str(),
                "NONE" | "RELAY" | "INGRESS"
            )
        {
            return Err(input_reason(
                "Peer 포트 범위 또는 거래 권한 NONE·RELAY·INGRESS를 확인하세요.",
            ));
        }
        if !node_ids.insert(node.node_id.clone())
            || !validator_ids.insert(node.validator_id.clone())
        {
            return Err(input_reason(
                "네트워크 자료의 노드 ID 또는 validator ID가 중복되었습니다.",
            ));
        }
        if !endpoints.insert((node.host.clone(), node.port)) {
            return Err(input_reason(
                "서로 다른 노드가 같은 peer 주소와 포트를 사용하고 있습니다.",
            ));
        }
        if node.credential_fingerprints.is_empty()
            || node.credential_fingerprints.len() > 16
            || node.revoked_credential_fingerprints.len() > 16
        {
            return Err(input_reason(
                "각 노드에는 active 인증서 pin 1~16개와 revoked pin 최대 16개가 필요합니다.",
            ));
        }
        for pin in node
            .credential_fingerprints
            .iter_mut()
            .chain(&mut node.revoked_credential_fingerprints)
        {
            *pin = canonical_hex(pin, 64).map_err(|_| {
                input_reason("인증서 pin은 0x로 시작하는 NIGO 방식의 32바이트 16진수여야 합니다.")
            })?;
            if !pins.insert(pin.clone()) {
                return Err(input_reason(
                    "인증서 pin이 중복되거나 active·revoked 목록이 겹칩니다.",
                ));
            }
        }
        node.credential_fingerprints.sort();
        node.revoked_credential_fingerprints.sort();
    }
    if validator_ids != membership {
        return Err(input_reason(
            "네트워크의 validator 목록이 승인된 체인의 validator 목록과 정확히 일치해야 합니다.",
        ));
    }
    value.nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    Ok(value)
}

/// Prepare deterministic native bytes from explicit public input; credentials are metadata only.
pub fn load(product_path: &Path, network_path: &Path, options: RuntimeOptions) -> Result<Prepared> {
    options.validate()?;
    let product = Input::read(product_path)?;
    let config = parse_bytes(&product.raw, &product.pin.path).map_err(|_| invalid())?;
    // Metadata-only product checks also reject unsafe secret permissions and data references.
    let metadata =
        super::preflight_bytes(&product.raw, &product.pin.path).map_err(|_| invalid())?;
    if metadata.outcome == "FAIL" {
        return Err(invalid());
    }
    // The explicit product document must be read to discover its references.
    // Before reading network or chain bytes, reject aliases of credentials,
    // data, or the other public documents using filesystem identities.
    let mut referenced_paths = vec![
        product.pin.path.clone(),
        super::resolve_source(network_path).map_err(|_| input_conflict())?,
        PathBuf::from(&config.chain_description),
        PathBuf::from(&config.storage.data_directory),
    ];
    referenced_paths.extend(secret_paths(&config).into_iter().map(PathBuf::from));
    // Input documents must not alias each other or credentials, and no input may reside inside data.
    let data = &referenced_paths[3];
    for (i, path) in referenced_paths.iter().enumerate() {
        if i != 3 && paths::overlaps(path, data).map_err(|_| input_conflict())? {
            return Err(input_conflict());
        }
        if i < 3 {
            for other in referenced_paths.iter().skip(i + 1) {
                if paths::overlaps(path, other).map_err(|_| input_conflict())? {
                    return Err(input_conflict());
                }
            }
        }
    }
    let network = Input::read(&referenced_paths[1])?;
    let chain = Input::read(&referenced_paths[2])?;
    let value = validated_network(&chain.raw, &network.raw)?;
    let local_id = canonical_hex(&config.node_id, 64)?;
    let local = value
        .nodes
        .iter()
        .find(|n| n.node_id == local_id)
        .ok_or_else(|| input_reason("제품 설정의 로컬 노드 ID가 네트워크 자료에 없습니다."))?;
    if local.host != host(&config.p2p.address)? || local.port as i64 != config.p2p.port {
        return Err(input_reason(
            "로컬 노드의 peer 주소와 포트가 제품 설정과 일치해야 합니다.",
        ));
    }
    let secret_stamps = secret_paths(&config)
        .into_iter()
        .map(|p| {
            let path = PathBuf::from(p);
            let m = fs::symlink_metadata(&path).map_err(|_| invalid())?;
            Ok((path, Stamp::of(&m)))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut node = BTreeMap::<String, Value>::new();
    let mut put = |key: &str, v: Value| {
        node.insert(key.into(), v);
    };
    put("server.address", config.http.address.clone().into());
    put("server.port", config.http.port.into());
    put("nigo.monitor.console.enabled", true.into());
    put("nigo.monitor.console.gc-control-enabled", false.into());
    put("nigo.storage.gc.automatic.enabled", false.into());
    put(
        "nigo.protocol.consensus.auto-start",
        options.consensus_auto_start.into(),
    );
    put(
        "nigo.transaction-ingress.permission",
        local.transaction_permission.clone().into(),
    );
    put(
        "nigo.protocol.consensus.qbft.sync.enabled",
        (options.mode == NetworkMode::JoinNetwork).into(),
    );
    put(
        "nigo.protocol.consensus.qbft.sync.trust-mode",
        "GENESIS_VERIFIED".into(),
    );
    put(
        "nigo.protocol.consensus.qbft.sync.archive.enabled",
        options.archive_enabled.into(),
    );
    for (key, val) in [
        ("role", "VALIDATOR"),
        ("node-id", local.node_id.as_str()),
        ("validator-id", local.validator_id.as_str()),
        ("listen-host", config.p2p.address.as_str()),
        ("transport-security-scheme", "MTLS"),
        ("keystore-path", config.secrets.validator_keystore.as_str()),
        (
            "keystore-password-file",
            config.secrets.validator_password_file.as_str(),
        ),
        ("mtls-key-store-path", config.secrets.tls_key_store.as_str()),
        (
            "mtls-key-store-password-file",
            config.secrets.tls_key_password_file.as_str(),
        ),
        (
            "mtls-trust-store-path",
            config.secrets.tls_trust_store.as_str(),
        ),
        (
            "mtls-trust-store-password-file",
            config.secrets.tls_trust_password_file.as_str(),
        ),
        ("mtls-key-store-type", options.key_store_type.text()),
        ("mtls-trust-store-type", options.trust_store_type.text()),
    ] {
        put(&format!("{PREFIX}{key}"), val.into());
    }
    put(&format!("{PREFIX}listen-port"), config.p2p.port.into());
    for (index, peer) in value
        .nodes
        .iter()
        .filter(|n| n.node_id != local_id)
        .enumerate()
    {
        let prefix = format!("{PREFIX}peers[{index}].");
        for (key, val) in [
            ("node-id", peer.node_id.as_str()),
            ("validator-id", peer.validator_id.as_str()),
            ("role", "VALIDATOR"),
            ("host", peer.host.as_str()),
            (
                "transaction-permission",
                peer.transaction_permission.as_str(),
            ),
        ] {
            put(&format!("{prefix}{key}"), val.into());
        }
        put(&format!("{prefix}port"), peer.port.into());
        for (name, values) in [
            ("credential-fingerprints", &peer.credential_fingerprints),
            (
                "revoked-credential-fingerprints",
                &peer.revoked_credential_fingerprints,
            ),
        ] {
            for (i, pin) in values.iter().enumerate() {
                put(&format!("{prefix}{name}[{i}]"), pin.clone().into());
            }
        }
    }
    let native = serde_json::json!({"chainFile":config.chain_description,"dataDirectory":config.storage.data_directory,"backend":"rocksdb","node":node});
    let mut native_bytes = serde_json::to_vec_pretty(&native).map_err(|_| invalid())?;
    native_bytes.push(b'\n');
    if native_bytes.len() > LIMIT {
        return Err(invalid());
    }
    let summary = Summary {
        schema_version: 1,
        outcome: "PREPARED",
        instance_id: config.instance_id,
        node_id: local.node_id.clone(),
        validator_id: local.validator_id.clone(),
        validator_count: value.nodes.len(),
        peer_count: value.nodes.len() - 1,
        http_address: config.http.address,
        http_port: config.http.port as u16,
        p2p_address: config.p2p.address,
        p2p_port: local.port,
        transaction_permission: local.transaction_permission.clone(),
        options,
        key_and_certificate_identity: "NOT_CHECKED",
        network_and_quorum: "NOT_CHECKED",
        engine_configuration: "NOT_CHECKED",
    };
    let prepared = Prepared {
        native_bytes,
        summary,
        input_pins: vec![product.pin, network.pin, chain.pin],
        referenced_paths,
        secret_stamps,
    };
    prepared.recheck()?;
    Ok(prepared)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintReport {
    pub outcome: &'static str,
    pub credential_fingerprint: String,
    pub encoding: &'static str,
    pub certificate_validation: &'static str,
}
/// Compute the NIGO transport credential pin from one explicit public certificate.
/// Checks DER envelope shape only, not X.509 semantics, dates, signatures, identity or trust.
pub fn fingerprint(path: &Path) -> Result<FingerprintReport> {
    let input = Input::read(path)?;
    let (der, encoding) = if input.raw.starts_with(b"-----BEGIN CERTIFICATE-----") {
        let text = std::str::from_utf8(&input.raw).map_err(|_| certificate_invalid())?;
        let body = text
            .strip_prefix("-----BEGIN CERTIFICATE-----")
            .and_then(|t| t.trim().strip_suffix("-----END CERTIFICATE-----"))
            .ok_or_else(certificate_invalid)?;
        if body.contains('-')
            || !body.bytes().all(|b| {
                b.is_ascii_whitespace() || b.is_ascii_alphanumeric() || b"+/=".contains(&b)
            })
        {
            return Err(certificate_invalid());
        }
        let compact = body
            .bytes()
            .filter(|b| !b.is_ascii_whitespace())
            .collect::<Vec<_>>();
        (
            STANDARD
                .decode(compact)
                .map_err(|_| certificate_invalid())?,
            "PEM",
        )
    } else {
        (input.raw.clone(), "DER")
    };
    certificate_envelope(&der)?;
    let mut digest = Sha256::new();
    digest.update(b"NIGO_TRANSPORT_CREDENTIAL/v1\0MTLS\0");
    digest.update(&der);
    if Input::read(&input.pin.path)?.pin != input.pin {
        return Err(changed());
    }
    Ok(FingerprintReport {
        outcome: "FINGERPRINT_COMPUTED",
        credential_fingerprint: format!("0x{}", hex::encode(digest.finalize())),
        encoding,
        certificate_validation: "NOT_CHECKED",
    })
}
fn tlv<'a>(input: &mut &'a [u8], tag: u8) -> Result<&'a [u8]> {
    if input.len() < 2 || input[0] != tag {
        return Err(certificate_invalid());
    }
    let first = input[1];
    let (offset, length) = if first < 128 {
        (2, first as usize)
    } else {
        let count = (first & 127) as usize;
        if count == 0 || count > 4 || input.len() < 2 + count || input[2] == 0 {
            return Err(certificate_invalid());
        }
        let mut size = 0usize;
        for b in &input[2..2 + count] {
            size = size
                .checked_mul(256)
                .and_then(|v| v.checked_add(*b as usize))
                .ok_or_else(certificate_invalid)?;
        }
        if size < 128 {
            return Err(certificate_invalid());
        }
        (2 + count, size)
    };
    let end = offset + length;
    if end > input.len() {
        return Err(certificate_invalid());
    }
    let body = &input[offset..end];
    *input = &input[end..];
    Ok(body)
}
fn certificate_envelope(raw: &[u8]) -> Result<()> {
    let mut input = raw;
    let mut outer = tlv(&mut input, 0x30)?;
    if !input.is_empty() {
        return Err(certificate_invalid());
    }
    let mut tbs = tlv(&mut outer, 0x30)?;
    let mut alg = tlv(&mut outer, 0x30)?;
    if tlv(&mut alg, 0x06)?.is_empty() {
        return Err(certificate_invalid());
    }
    let signature = tlv(&mut outer, 0x03)?;
    if !outer.is_empty() || signature.len() < 2 || signature[0] > 7 {
        return Err(certificate_invalid());
    }
    if tbs.first() == Some(&0xa0) {
        let mut version = tlv(&mut tbs, 0xa0)?;
        if tlv(&mut version, 0x02)?.len() != 1 || !version.is_empty() {
            return Err(certificate_invalid());
        }
    }
    if tlv(&mut tbs, 0x02)?.is_empty() {
        return Err(certificate_invalid());
    }
    tlv(&mut tbs, 0x30)?;
    tlv(&mut tbs, 0x30)?;
    let mut validity = tlv(&mut tbs, 0x30)?;
    for _ in 0..2 {
        let tag = *validity.first().ok_or_else(certificate_invalid)?;
        if !matches!(tag, 0x17 | 0x18) || tlv(&mut validity, tag)?.is_empty() {
            return Err(certificate_invalid());
        }
    }
    if !validity.is_empty() {
        return Err(certificate_invalid());
    }
    tlv(&mut tbs, 0x30)?;
    let mut spki = tlv(&mut tbs, 0x30)?;
    let mut key_alg = tlv(&mut spki, 0x30)?;
    if tlv(&mut key_alg, 0x06)?.is_empty() {
        return Err(certificate_invalid());
    }
    let key = tlv(&mut spki, 0x03)?;
    if key.len() < 2 || key[0] > 7 || !spki.is_empty() {
        return Err(certificate_invalid());
    }
    let mut previous = 0;
    while !tbs.is_empty() {
        let tag = tbs[0];
        if !matches!(tag, 0x81 | 0x82 | 0xa3) || tag <= previous {
            return Err(certificate_invalid());
        }
        previous = tag;
        tlv(&mut tbs, tag)?;
    }
    Ok(())
}
fn input_reason(message: &'static str) -> BxdlError {
    BxdlError::new("NETWORK_INPUT_INVALID", message)
}
fn input_conflict() -> BxdlError {
    input_reason("공개 입력 경로는 다른 입력, 키·비밀번호 파일 및 데이터 경로와 겹칠 수 없습니다.")
}
fn invalid() -> BxdlError {
    BxdlError::new(
        "NETWORK_INPUT_INVALID",
        "네트워크 자료, 체인 구성 및 기존 키 파일의 경로·권한을 확인하세요.",
    )
}
fn changed() -> BxdlError {
    BxdlError::new(
        "NETWORK_INPUT_CHANGED",
        "준비에 사용한 입력 파일이 변경되었습니다. 새 준비 작업을 시작하세요.",
    )
}
fn certificate_invalid() -> BxdlError {
    BxdlError::new(
        "NETWORK_CERTIFICATE_INVALID",
        "단일 공개 X.509 인증서의 PEM 또는 DER 형식을 확인하세요.",
    )
}
#[cfg(test)]
mod tests;
