//! Read-only projections of explicitly selected targets. A catalog snapshot is
//! a remembered record, never proof of current service health or action consent.
use crate::{
    engine::instance,
    error::{BxdlError, Result},
    setup,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Kind {
    Instance,
    InstallSession,
    DraftSession,
    NetworkSession,
    PreparationSession,
}
impl Kind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Instance => "등록된 노드",
            Self::InstallSession => "설치 세션",
            Self::DraftSession => "제품 설정 초안",
            Self::NetworkSession => "네트워크 설정 초안",
            Self::PreparationSession => "설치 준비 세션",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum RecordState {
    Registered,
    Initialized,
    Unknown,
    DraftIncomplete,
    DraftComplete,
    InstallPending,
    InstallPartial,
    InstallRecorded,
}
impl RecordState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Registered => "등록 기록 있음 · 초기화 전",
            Self::Initialized => "초기화 완료 기록 있음 · 운용 상태 미조회",
            Self::Unknown => "작업 결과 불명 · 기존 자료 보존 필요",
            Self::DraftIncomplete => "입력 중인 초안",
            Self::DraftComplete => "입력 완료 초안 · 설치 전",
            Self::InstallPending => "설치 전 세션",
            Self::InstallPartial => "설치 진행 기록 있음 · 등록 미완료",
            Self::InstallRecorded => "등록 기록 있음 · 운용 상태 미조회",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum CliMatch {
    Match,
    Mismatch,
    Unavailable,
    NotApplicable,
}
impl CliMatch {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Match => "현재 CLI와 패키지 CLI의 파일 내용 일치 · 실행 승인 아님",
            Self::Mismatch => "CLI 파일 내용 불일치 · 실행기 호환 검사 필요",
            Self::Unavailable => "CLI 파일 내용 비교 불가",
            Self::NotApplicable => "이 세션에는 CLI 비교를 적용하지 않음",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub kind: Kind,
    pub path: PathBuf,
    pub identity: Identity,
    pub schema_version: u32,
    pub name: String,
    pub state: RecordState,
    pub cli: CliMatch,
}
impl Snapshot {
    /// Validate an untrusted catalog record without touching its target path.
    pub(crate) fn validate(&self) -> Result<()> {
        let text = self.path.to_str().ok_or_else(invalid)?;
        if !self.path.is_absolute()
            || text.is_empty()
            || text.len() > 4096
            || text.chars().any(char::is_control)
            || setup::absolute(&self.path)
                .map_err(|_| invalid())?
                .as_os_str()
                != self.path.as_os_str()
            || self.identity.inode == 0
            || self.name.is_empty()
            || self.name.len() > 256
            || self.name.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let valid = match self.kind {
            Kind::Instance => {
                matches!(self.schema_version, 1 | 2)
                    && matches!(
                        self.state,
                        RecordState::Registered | RecordState::Initialized | RecordState::Unknown
                    )
                    && self.cli != CliMatch::NotApplicable
            }
            Kind::InstallSession => {
                self.schema_version == 1
                    && matches!(
                        self.state,
                        RecordState::InstallPending
                            | RecordState::InstallPartial
                            | RecordState::InstallRecorded
                            | RecordState::Unknown
                    )
                    && self.cli == CliMatch::NotApplicable
            }
            Kind::DraftSession | Kind::NetworkSession | Kind::PreparationSession => {
                self.schema_version == 1
                    && matches!(
                        self.state,
                        RecordState::DraftIncomplete | RecordState::DraftComplete
                    )
                    && self.cli == CliMatch::NotApplicable
            }
        };
        if !valid {
            return Err(invalid());
        }
        Ok(())
    }
}

pub(crate) fn inspect(kind: Kind, path: &Path) -> Result<Snapshot> {
    let path = setup::absolute(path).map_err(|_| invalid())?;
    let identity = root_identity(&path)?;
    let (schema_version, name, state, cli) = match kind {
        Kind::Instance => {
            let record = instance::catalog_record(&path).map_err(safe_error)?;
            let state = match record.initialization {
                "NOT_STARTED" => RecordState::Registered,
                "INITIALIZED" => RecordState::Initialized,
                _ => RecordState::Unknown,
            };
            let cli = match record.cli_match {
                Some(true) => CliMatch::Match,
                Some(false) => CliMatch::Mismatch,
                None => CliMatch::Unavailable,
            };
            (record.schema_version, record.instance_id, state, cli)
        }
        Kind::InstallSession => {
            // Opening an existing workflow takes its bounded lifetime lock but
            // does not reconcile, save, repair or run a node. Drop immediately.
            let summary = setup::workflow::Workflow::resume(&path)
                .map_err(safe_error)?
                .summary();
            let state = if [
                summary.installation.as_str(),
                summary.registration.as_str(),
                summary.initialization.as_str(),
            ]
            .contains(&"UNKNOWN")
            {
                RecordState::Unknown
            } else if summary.registration == "CONFIRMED" {
                RecordState::InstallRecorded
            } else if summary.installation == "CONFIRMED" {
                RecordState::InstallPartial
            } else {
                RecordState::InstallPending
            };
            (1, kind.label().into(), state, CliMatch::NotApplicable)
        }
        Kind::DraftSession => {
            let session = setup::Session::resume(&path).map_err(safe_error)?;
            let summary = session.summary();
            let name = if session.has_answer("instanceId") {
                session
                    .review_rows()
                    .into_iter()
                    .zip(setup::FIELDS)
                    .find_map(|((_, value), field)| (field.name == "instanceId").then_some(value))
                    .unwrap_or_else(|| kind.label().into())
            } else {
                kind.label().into()
            };
            (
                1,
                name,
                draft_state(summary.draft_complete),
                CliMatch::NotApplicable,
            )
        }
        Kind::PreparationSession => {
            // Reading a preparation record holds only the existing bounded
            // workflow lock. It does not re-verify a package, prepare outputs,
            // run preflight, install or observe the runtime.
            let summary = setup::preparation::Preparation::resume(&path)
                .map_err(safe_error)?
                .summary();
            (
                1,
                kind.label().into(),
                draft_state(summary.draft_complete),
                CliMatch::NotApplicable,
            )
        }
        Kind::NetworkSession => {
            let summary = setup::network::Session::resume(&path)
                .map_err(safe_error)?
                .summary();
            (
                1,
                kind.label().into(),
                draft_state(summary.draft_complete),
                CliMatch::NotApplicable,
            )
        }
    };
    if root_identity(&path)? != identity {
        return Err(replaced());
    }
    let snapshot = Snapshot {
        kind,
        path,
        identity,
        schema_version,
        name,
        state,
        cli,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

/// Re-observe public record state while refusing replacement or migration of
/// the target remembered in the advisory catalog. This does not grant consent.
pub(crate) fn recheck(expected: &Snapshot) -> Result<Snapshot> {
    expected.validate()?;
    if root_identity(&expected.path)? != expected.identity {
        return Err(replaced());
    }
    let current = inspect(expected.kind, &expected.path)?;
    if current.identity != expected.identity
        || current.schema_version != expected.schema_version
        || current.path != expected.path
        || (current.kind == Kind::Instance && current.name != expected.name)
    {
        return Err(replaced());
    }
    Ok(current)
}

/// A selected node with pinned root/binding identity and, for names, a pinned
/// catalog revision. It never authorizes an engine action by itself.
pub(crate) struct ResolvedInstance {
    catalog: Option<super::catalog::Catalog>,
    snapshot: Snapshot,
    binding_sha256: String,
}
impl ResolvedInstance {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    /// Call again immediately before dispatch, in addition to the operation's
    /// existing input pins and locks. No prompt should span this final check.
    pub(crate) fn recheck(&self) -> Result<Snapshot> {
        if let Some(catalog) = &self.catalog {
            catalog.check_current()?;
        }
        let current = recheck(&self.snapshot)?;
        if current.name != self.snapshot.name || binding_sha256(&current)? != self.binding_sha256 {
            return Err(replaced());
        }
        if let Some(catalog) = &self.catalog {
            catalog.check_current()?;
        }
        Ok(current)
    }
}

/// Both direct paths and names use the same explicit target selection. Path
/// mode intentionally ignores catalog/home state, preserving the fallback.
pub(crate) fn resolve_instance(
    home: Option<&Path>,
    name: Option<&str>,
    instance: Option<&Path>,
) -> Result<ResolvedInstance> {
    let resolved = match (name, instance) {
        (None, Some(path)) => {
            let snapshot = inspect(Kind::Instance, path)?;
            let binding_sha256 = binding_sha256(&snapshot)?;
            ResolvedInstance {
                catalog: None,
                snapshot,
                binding_sha256,
            }
        }
        (Some(name), None) => {
            if !super::catalog::valid_name(name) {
                return Err(super::catalog::invalid_name());
            }
            let missing = || {
                BxdlError::new(
                    "NODE_NAME_MISSING",
                    "등록한 노드 이름이 없습니다. bxdl nodes로 저장 목록을 확인하세요.",
                )
            };
            let home = home.ok_or_else(|| {
                BxdlError::new(
                    "NODE_HOME_REQUIRED",
                    "이름을 해석할 관리 위치가 필요합니다.",
                )
            })?;
            let catalog = super::catalog::Catalog::open(home)?.ok_or_else(missing)?;
            let entry = catalog
                .entries()
                .iter()
                .find(|entry| entry.alias.as_deref() == Some(name))
                .ok_or_else(missing)?;
            let snapshot = entry.target.clone();
            let binding_sha256 = entry.binding_sha256.clone().ok_or_else(missing)?;
            ResolvedInstance {
                catalog: Some(catalog),
                snapshot,
                binding_sha256,
            }
        }
        _ => {
            return Err(BxdlError::new(
                "NODE_TARGET_REQUIRED",
                "노드 이름 또는 --instance 경로 중 하나만 지정하세요.",
            ));
        }
    };
    resolved.recheck()?;
    Ok(resolved)
}

pub(super) fn binding_sha256(expected: &Snapshot) -> Result<String> {
    if expected.kind != Kind::Instance {
        return Err(invalid());
    }
    if root_identity(&expected.path)? != expected.identity {
        return Err(replaced());
    }
    let binding =
        instance::catalog_binding_sha256(&expected.path, expected.schema_version, &expected.name)
            .map_err(safe_error)?;
    if root_identity(&expected.path)? != expected.identity {
        return Err(replaced());
    }
    Ok(binding)
}

/// Guard the concrete files/directories written by catalog creation and saves.
/// Private data/package/credential paths remain inside their existing adapters.
/// A managed home may contain sibling data and control namespaces safely.
pub(crate) fn check_catalog_writes(expected: &Snapshot, home: &Path) -> Result<()> {
    let current = recheck(expected)?;
    let home = setup::absolute(home).map_err(|_| invalid())?;
    let candidates = ["catalog", ".bxdl-home.json", ".catalog.lock"].map(|name| home.join(name));
    for candidate in &candidates {
        if setup::paths::overlaps(candidate, &current.path).map_err(|_| write_conflict())? {
            return Err(write_conflict());
        }
    }
    let checked = (|| -> Result<()> {
        match current.kind {
            Kind::Instance => {
                for candidate in &candidates {
                    instance::check_catalog_location(&current.path, candidate)?;
                }
            }
            Kind::InstallSession => {
                let workflow = setup::workflow::Workflow::resume(&current.path)?;
                for candidate in &candidates {
                    workflow.check_catalog_location(candidate)?;
                }
            }
            Kind::DraftSession => {
                let session = setup::Session::resume(&current.path)?;
                for candidate in &candidates {
                    session.check_catalog_location(candidate)?;
                }
            }
            Kind::PreparationSession => {
                let preparation = setup::preparation::Preparation::resume(&current.path)?;
                for candidate in &candidates {
                    preparation.check_catalog_location(candidate)?;
                }
            }
            Kind::NetworkSession => {
                let session = setup::network::Session::resume(&current.path)?;
                for candidate in &candidates {
                    session.check_catalog_location(candidate)?;
                }
            }
        }
        Ok(())
    })();
    checked.map_err(|_| write_conflict())?;
    if root_identity(&current.path)? != current.identity {
        return Err(replaced());
    }
    Ok(())
}

fn write_conflict() -> BxdlError {
    BxdlError::new(
        "CONSOLE_TARGET_WRITE_CONFLICT",
        "목록 기록 위치가 대상의 입력·데이터·설치 위치와 겹치거나 안전성을 확인할 수 없습니다. 별도 관리 위치를 선택하세요.",
    )
}

fn draft_state(complete: bool) -> RecordState {
    if complete {
        RecordState::DraftComplete
    } else {
        RecordState::DraftIncomplete
    }
}
fn root_identity(path: &Path) -> Result<Identity> {
    // The filesystem-aware walker rejects every symlink component, including
    // ancestors, before canonicalization. The canonical name is stored only.
    setup::paths::same(path, path).map_err(|_| invalid())?;
    if fs::canonicalize(path)
        .map_err(|_| unavailable())?
        .as_os_str()
        != path.as_os_str()
    {
        return Err(invalid());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if !metadata.is_dir()
        || metadata.mode() & 0o7777 != 0o700
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(invalid());
    }
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}
fn safe_error(error: BxdlError) -> BxdlError {
    if error.code == "SETUP_WORKFLOW_BUSY" {
        BxdlError::new(
            "CONSOLE_TARGET_BUSY",
            "다른 설치 도우미가 이 세션을 사용하고 있습니다. 기존 기록은 변경하지 않았습니다.",
        )
    } else {
        unavailable()
    }
}
fn invalid() -> BxdlError {
    BxdlError::new(
        "CONSOLE_TARGET_INVALID",
        "지원되는 비공개 등록·세션 폴더인지 확인하세요. 기존 자료는 변경하지 않았습니다.",
    )
}
fn unavailable() -> BxdlError {
    BxdlError::new(
        "CONSOLE_TARGET_UNAVAILABLE",
        "등록·세션 기록을 확인할 수 없습니다. 원래 CLI의 도움말로 경로와 자료를 확인하세요.",
    )
}
fn replaced() -> BxdlError {
    BxdlError::new(
        "CONSOLE_TARGET_CHANGED",
        "목록에 저장한 대상과 현재 폴더·형식이 다릅니다. 기존 자료를 보존하고 명시적으로 다시 연결하세요.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn root() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        (temp, path)
    }
    #[test]
    fn explicit_draft_inspection_is_read_only_and_rechecks_replacements() {
        let (_temp, root) = root();
        let path = root.join("공백 있는 초안");
        setup::Session::create(&path, None).unwrap();
        let before = fs::read_dir(&path).unwrap().count();
        let snapshot = inspect(Kind::DraftSession, &path).unwrap();
        assert_eq!(snapshot.state, RecordState::DraftIncomplete);
        assert_eq!(snapshot.cli, CliMatch::NotApplicable);
        assert_eq!(before, fs::read_dir(&path).unwrap().count());
        assert_eq!(recheck(&snapshot).unwrap(), snapshot);
        fs::rename(&path, root.join("이전 초안")).unwrap();
        setup::Session::create(&path, None).unwrap();
        assert_eq!(
            recheck(&snapshot).unwrap_err().code,
            "CONSOLE_TARGET_CHANGED"
        );
    }
    fn recorded_files(path: &Path) -> Vec<(PathBuf, Vec<u8>, u64, u32)> {
        fn visit(base: &Path, path: &Path, out: &mut Vec<(PathBuf, Vec<u8>, u64, u32)>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                let metadata = fs::symlink_metadata(&path).unwrap();
                if metadata.is_dir() {
                    visit(base, &path, out);
                } else {
                    out.push((
                        path.strip_prefix(base).unwrap().to_owned(),
                        fs::read(&path).unwrap(),
                        metadata.ino(),
                        metadata.mode(),
                    ));
                }
            }
        }
        let mut files = Vec::new();
        visit(path, path, &mut files);
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }

    #[test]
    fn preparation_inspection_is_read_only_and_does_not_keep_its_lock() {
        let (_temp, root) = root();
        let path = root.join("preparation");
        let mut preparation = setup::preparation::Preparation::create(&path, None).unwrap();
        let data = root.join("PRIVATE_CANARY_data");
        preparation
            .set_field("dataDirectory", data.to_str().unwrap())
            .unwrap();
        assert_eq!(
            inspect(Kind::PreparationSession, &path).unwrap_err().code,
            "CONSOLE_TARGET_BUSY"
        );
        drop(preparation);
        let before = recorded_files(&path);
        let snapshot = inspect(Kind::PreparationSession, &path).unwrap();
        assert_eq!(snapshot.state, RecordState::DraftIncomplete);
        assert_eq!(snapshot.cli, CliMatch::NotApplicable);
        assert_eq!(snapshot.name, "설치 준비 세션");
        assert_eq!(snapshot.schema_version, 1);
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("PRIVATE_CANARY")
        );
        assert_eq!(recheck(&snapshot).unwrap(), snapshot);
        assert_eq!(recorded_files(&path), before);
        assert!(!data.exists());
        let preparation = setup::preparation::Preparation::resume(&path).unwrap();
        assert_eq!(preparation.summary().preparation, "DRAFT");
        drop(preparation);
        let mut invalid = snapshot.clone();
        invalid.state = RecordState::Initialized;
        assert!(invalid.validate().is_err());
        invalid.state = RecordState::DraftComplete;
        assert!(invalid.validate().is_ok());
        invalid.schema_version = 2;
        assert!(invalid.validate().is_err());
        fs::rename(&path, root.join("previous-preparation")).unwrap();
        drop(setup::preparation::Preparation::create(&path, None).unwrap());
        assert_eq!(
            recheck(&snapshot).unwrap_err().code,
            "CONSOLE_TARGET_CHANGED"
        );
    }

    #[test]
    fn preparation_catalog_guard_protects_planned_paths_without_preparing_outputs() {
        let (_temp, root) = root();
        let path = root.join("preparation");
        let mut preparation = setup::preparation::Preparation::create(&path, None).unwrap();
        let data = root.join("data");
        let credential = root.join("credentials/validator.key");
        let archive = root.join("supply/package.tar.gz");
        let destination = root.join("installed-package");
        let instance = root.join("control");
        preparation
            .set_field("dataDirectory", data.to_str().unwrap())
            .unwrap();
        preparation
            .set_field("validatorKeystore", credential.to_str().unwrap())
            .unwrap();
        preparation
            .set_inputs(setup::preparation::Inputs {
                archive: Some(archive.clone()),
                destination: Some(destination.clone()),
                instance: Some(instance.clone()),
                ..Default::default()
            })
            .unwrap();
        drop(preparation);
        let snapshot = inspect(Kind::PreparationSession, &path).unwrap();
        let before = recorded_files(&path);
        check_catalog_writes(&snapshot, &root.join("separate-home")).unwrap();
        for reference in [data, credential, archive, destination, instance] {
            let unsafe_home = reference.join("home");
            assert_eq!(
                check_catalog_writes(&snapshot, &unsafe_home)
                    .unwrap_err()
                    .code,
                "CONSOLE_TARGET_WRITE_CONFLICT"
            );
            assert!(!reference.exists());
        }
        assert_eq!(recorded_files(&path), before);
    }

    #[test]
    fn catalog_write_guard_protects_partial_draft_data_and_credential_paths() {
        let (_temp, root) = root();
        let home = root.join("managed");
        fs::create_dir_all(home.join("sessions")).unwrap();
        let path = home.join("sessions/draft");
        let mut session = setup::Session::create(&path, None).unwrap();
        let data = home.join("data/node");
        let credential = root.join("private-keys/validator.key");
        session
            .set("dataDirectory", data.to_str().unwrap())
            .unwrap();
        session
            .set("validatorKeystore", credential.to_str().unwrap())
            .unwrap();
        let snapshot = inspect(Kind::DraftSession, &path).unwrap();
        let before = fs::read_dir(&path).unwrap().count();
        check_catalog_writes(&snapshot, &home).unwrap();
        let unsafe_home = data.join("new-console");
        assert_eq!(
            check_catalog_writes(&snapshot, &unsafe_home)
                .unwrap_err()
                .code,
            "CONSOLE_TARGET_WRITE_CONFLICT"
        );
        assert!(check_catalog_writes(&snapshot, &credential.join("console")).is_err());
        assert!(!unsafe_home.exists());
        assert!(!data.exists());
        assert!(!credential.exists());
        assert_eq!(before, fs::read_dir(&path).unwrap().count());
    }

    #[test]
    fn symlink_public_root_and_foreign_identity_are_refused() {
        let (_temp, root) = root();
        let path = root.join("draft");
        setup::Session::create(&path, None).unwrap();
        let mut snapshot = inspect(Kind::DraftSession, &path).unwrap();
        snapshot.identity.inode += 1;
        assert!(recheck(&snapshot).is_err());
        symlink(&path, root.join("alias")).unwrap();
        assert!(inspect(Kind::DraftSession, &root.join("alias")).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(inspect(Kind::DraftSession, &path).is_err());
    }
    #[test]
    fn catalog_metadata_rejects_cross_kind_states_and_private_error_text() {
        let (_temp, root) = root();
        let path = root.join("draft");
        setup::Session::create(&path, None).unwrap();
        let mut snapshot = inspect(Kind::DraftSession, &path).unwrap();
        snapshot.state = RecordState::Initialized;
        assert!(snapshot.validate().is_err());
        snapshot.state = RecordState::DraftIncomplete;
        snapshot.name = "credential\ncanary".into();
        assert!(snapshot.validate().is_err());
        let error = safe_error(BxdlError::new("TEST", "PRIVATE_CANARY /secret/file"));
        assert!(!error.to_string().contains("PRIVATE_CANARY"));
        assert!(!error.to_string().contains("/secret/file"));
    }
    #[test]
    fn network_inspection_keeps_credentials_out_of_metadata_and_creates_no_output() {
        use serde_json::{Value, json};
        let (_temp, root) = root();
        let mut product: Value = serde_json::from_slice(include_bytes!(
            "../../config/examples/instance.development.json"
        ))
        .unwrap();
        let node = format!("0x{}", "11".repeat(32));
        let validator = format!("0x{}", "22".repeat(20));
        product["nodeId"] = node.clone().into();
        product["chainDescription"] = "chain.json".into();
        product["storage"]["dataDirectory"] = "must-not-create-data".into();
        product["p2p"]["address"] = "127.0.0.1".into();
        product["p2p"]["port"] = 19090.into();
        for name in [
            "validatorKeystore",
            "validatorPasswordFile",
            "tlsKeyStore",
            "tlsKeyPasswordFile",
            "tlsTrustStore",
            "tlsTrustPasswordFile",
        ] {
            let secret = root.join(format!("PRIVATE_CANARY_{name}"));
            fs::write(&secret, b"PRIVATE_CREDENTIAL_CONTENT").unwrap();
            fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
            product["secrets"][name] = secret.to_str().unwrap().into();
        }
        for (name, document) in [
            ("product.json", product),
            (
                "chain.json",
                json!({
                    "nigo.protocol.consensus.protocol": "QBFT",
                    "nigo.protocol.consensus.qbft.validators[0]": validator,
                }),
            ),
            (
                "network.json",
                json!({
                    "schemaVersion": 1, "kind": "BXDL_QBFT_NETWORK",
                    "nodes": [{"nodeId": node, "validatorId": validator,
                        "host": "127.0.0.1", "port": 19090,
                        "transactionPermission": "NONE",
                        "credentialFingerprints": [format!("0x{}", "33".repeat(32))]}],
                }),
            ),
        ] {
            let path = root.join(name);
            fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let path = root.join("network draft");
        setup::network::Session::create(
            &path,
            &root.join("product.json"),
            &root.join("network.json"),
        )
        .unwrap();
        let before = fs::read_dir(&path).unwrap().count();
        let snapshot = inspect(Kind::NetworkSession, &path).unwrap();
        assert_eq!(snapshot.state, RecordState::DraftIncomplete);
        assert_eq!(recheck(&snapshot).unwrap(), snapshot);
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("PRIVATE_")
        );
        assert_eq!(before, fs::read_dir(&path).unwrap().count());
        assert!(!root.join("must-not-create-data").exists());
        assert!(!path.join("native.json").exists());
        check_catalog_writes(&snapshot, &root.join("separate-console")).unwrap();
        assert!(
            check_catalog_writes(&snapshot, &root.join("must-not-create-data/console")).is_err()
        );
        assert!(
            check_catalog_writes(
                &snapshot,
                &root.join("PRIVATE_CANARY_validatorKeystore/console")
            )
            .is_err()
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn install_session_inspection_never_reconciles_or_runs_active_workflow() {
        let (_temp, root) = root();
        let path = root.join("install");
        let workflow = setup::workflow::Workflow::create(&path, None).unwrap();
        assert_eq!(
            inspect(Kind::InstallSession, &path).unwrap_err().code,
            "CONSOLE_TARGET_BUSY"
        );
        drop(workflow);
        let before = fs::read_dir(path.join("workflow")).unwrap().count();
        let snapshot = inspect(Kind::InstallSession, &path).unwrap();
        assert_eq!(snapshot.state, RecordState::InstallPending);
        assert_eq!(before, fs::read_dir(path.join("workflow")).unwrap().count());
        assert_eq!(fs::read_dir(path.join("generated")).unwrap().count(), 0);
    }
}
