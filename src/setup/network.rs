//! Resumable native configuration preparation, separate from v1 product drafts
//! and installation journals. Only public input documents are pinned here.
use super::{Field, absolute, error, paths, store::Store};
use crate::{
    artifact,
    config::network::{self, InputPin, NetworkMode, Prepared, RuntimeOptions, StoreType},
    error::Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub use super::network_wizard::{Finish, interact};

pub(super) const FIELDS: &[Field] = &[
    Field {
        name: "keyStoreType",
        label: "TLS keystore 형식",
        hint: "기존 TLS 파일의 실제 형식을 선택하세요: PKCS12 / JKS",
        default: Some("PKCS12"),
    },
    Field {
        name: "trustStoreType",
        label: "TLS truststore 형식",
        hint: "기존 신뢰 저장소의 실제 형식을 선택하세요: PKCS12 / JKS",
        default: Some("PKCS12"),
    },
    Field {
        name: "mode",
        label: "네트워크 시작 방식",
        hint: "new-network: 공통 체인 자료의 새 네트워크 / join-network: 기존 네트워크 참여. 체인 자료나 멤버를 변경하지 않습니다",
        default: Some("new-network"),
    },
    Field {
        name: "consensusAutoStart",
        label: "엔진 실행 후 합의 참여 시작",
        hint: "yes / no. yes는 나중에 노드를 명시적으로 시작했을 때 적용됩니다. 이 도우미는 노드를 시작하지 않습니다",
        default: Some("yes"),
    },
    Field {
        name: "archiveEnabled",
        label: "블록 아카이브 사용",
        hint: "yes / no. yes는 보존 데이터와 저장 공간을 늘립니다. 네트워크 전체에서 끄면 archive 기반 따라잡기에 필요한 공급 노드가 없습니다",
        default: Some("no"),
    },
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Draft {
    schema_version: u32,
    kind: String,
    input_base: PathBuf,
    product: PathBuf,
    network: PathBuf,
    input_pins: Vec<InputPin>,
    answers: BTreeMap<String, String>,
}

pub struct Session {
    store: Store,
    draft: Draft,
    prepared: Prepared,
}

/// Fixed public projection. Private paths, source hashes and native bytes stay
/// in the private checkpoint or explicit output, never in this report.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub mode: &'static str,
    pub workspace: PathBuf,
    pub completed_fields: Vec<String>,
    pub next_field: Option<String>,
    pub total_fields: usize,
    pub draft_complete: bool,
    pub installation: &'static str,
    pub engine_validation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<network::Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<PathBuf>,
}

impl Session {
    pub fn create(workspace: &Path, product: &Path, network_path: &Path) -> Result<Self> {
        let workspace = absolute(workspace)?;
        let input_base = absolute(&std::env::current_dir().map_err(|_| invalid())?)?;
        let product = absolute(product)?;
        let network_path = absolute(network_path)?;
        // The temporary options only inspect inputs. Every option must still be
        // accepted by the operator before an output can be published.
        let prepared = network::load(&product, &network_path, RuntimeOptions::default())?;
        check_workspace(&workspace, &prepared)?;
        let draft = Draft {
            schema_version: 1,
            kind: "BXDL_NETWORK_PREPARATION".into(),
            input_base,
            product,
            network: network_path,
            input_pins: prepared.input_pins.clone(),
            answers: BTreeMap::new(),
        };
        check_draft(&draft)?;
        prepared.recheck()?;
        let mut store = Store::create(&workspace)?;
        store.save(&draft_bytes(&draft)?)?;
        Ok(Self {
            store,
            draft,
            prepared,
        })
    }

    pub fn resume(workspace: &Path) -> Result<Self> {
        let store = Store::open(&absolute(workspace)?)?;
        let raw = store.read()?.ok_or_else(invalid)?;
        let draft: Draft = artifact::decode_strict_json(&raw).map_err(|_| invalid())?;
        check_draft(&draft)?;
        let prepared = prepare(&draft)?;
        check_workspace(store.path(), &prepared)?;
        Ok(Self {
            store,
            draft,
            prepared,
        })
    }

    pub fn summary(&self) -> Summary {
        Summary {
            mode: "NETWORK",
            workspace: self.store.path().to_owned(),
            completed_fields: FIELDS
                .iter()
                .filter(|field| self.has_answer(field.name))
                .map(|field| field.name.to_owned())
                .collect(),
            next_field: FIELDS
                .get(self.next_index())
                .map(|field| field.name.to_owned()),
            total_fields: FIELDS.len(),
            draft_complete: self.complete(),
            installation: "NOT_PERFORMED",
            engine_validation: "NOT_CHECKED",
            configuration: self.complete().then(|| self.prepared.summary.clone()),
            output_path: None,
        }
    }

    pub(super) fn next_index(&self) -> usize {
        FIELDS
            .iter()
            .position(|field| !self.has_answer(field.name))
            .unwrap_or(FIELDS.len())
    }
    pub(super) fn complete(&self) -> bool {
        self.next_index() == FIELDS.len()
    }
    pub(super) fn has_answer(&self, name: &str) -> bool {
        self.draft.answers.contains_key(name)
    }
    pub(super) fn set(&mut self, name: &str, value: &str) -> Result<()> {
        let mut next = self.draft.clone();
        next.answers.insert(name.to_owned(), value.to_owned());
        check_draft(&next)?;
        let prepared = prepare(&next)?;
        check_workspace(self.store.path(), &prepared)?;
        self.store.save(&draft_bytes(&next)?)?;
        self.draft = next;
        self.prepared = prepared;
        Ok(())
    }

    pub(super) fn check(&mut self) -> Result<()> {
        self.store.read()?;
        let prepared = prepare(&self.draft)?;
        check_workspace(self.store.path(), &prepared)?;
        self.prepared = prepared;
        Ok(())
    }

    pub(super) fn resolve(&self, path: &Path) -> Result<PathBuf> {
        absolute(&if path.is_absolute() {
            path.to_owned()
        } else {
            self.draft.input_base.join(path)
        })
    }

    pub(super) fn default_output(&self) -> Result<PathBuf> {
        let parent = self.store.path().parent().ok_or_else(invalid)?;
        let name = self
            .store
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(invalid)?;
        Ok(parent.join(format!("{name}-node.json")))
    }

    pub(super) fn product_path(&self) -> &Path {
        &self.draft.product
    }

    pub fn export(&self, output: &Path) -> Result<PathBuf> {
        if !self.complete() {
            return Err(error(
                "SETUP_NETWORK_INCOMPLETE",
                "TLS 형식과 실행 정책의 선택이 남아 있습니다. --resume으로 이어가세요.",
            ));
        }
        self.store.read()?;
        let output = self.resolve(output)?;
        let prepared = prepare(&self.draft)?;
        check_workspace(self.store.path(), &prepared)?;
        if paths::overlaps(&output, self.store.path())? {
            return Err(output_conflict());
        }
        for reference in &prepared.referenced_paths {
            if paths::overlaps(&output, reference)? {
                return Err(output_conflict());
            }
        }
        prepared.recheck()?;
        // OUTPUT_EXISTS proves this call did not publish. Other storage errors
        // may occur after the exclusive link, including directory fsync errors.
        super::store::write_new(&output, &prepared.native_bytes).map_err(|failure| {
            if failure.code == "OUTPUT_EXISTS" {
                failure
            } else {
                publication_uncertain()
            }
        })?;
        // Once publication was attempted, an input/checkpoint recheck failure
        // must not be converted into an ordinary saved-draft outcome.
        prepared.recheck().map_err(|_| publication_uncertain())?;
        self.store.read().map_err(|_| publication_uncertain())?;
        Ok(output)
    }
}

fn prepare(draft: &Draft) -> Result<Prepared> {
    network::verify_pins(&draft.input_pins)?;
    let prepared = network::load(&draft.product, &draft.network, options(&draft.answers)?)?;
    if serde_json::to_vec(&draft.input_pins).map_err(|_| invalid())?
        != serde_json::to_vec(&prepared.input_pins).map_err(|_| invalid())?
    {
        return Err(error(
            "SETUP_NETWORK_INPUT_CHANGED",
            "처음 준비한 공개 설정 자료가 변경되었습니다. 기존 초안을 보존하고 새 작업 폴더에서 다시 준비하세요.",
        ));
    }
    Ok(prepared)
}

fn check_draft(draft: &Draft) -> Result<()> {
    if draft.schema_version != 1
        || draft.kind != "BXDL_NETWORK_PREPARATION"
        || draft.answers.len() > FIELDS.len()
    {
        return Err(invalid());
    }
    for path in [&draft.input_base, &draft.product, &draft.network] {
        if !path.is_absolute() || absolute(path)? != *path {
            return Err(invalid());
        }
    }
    options(&draft.answers)?;
    Ok(())
}

fn options(answers: &BTreeMap<String, String>) -> Result<RuntimeOptions> {
    let mut options = RuntimeOptions::default();
    for (name, value) in answers {
        match (name.as_str(), value.as_str()) {
            ("keyStoreType", "PKCS12") => options.key_store_type = StoreType::Pkcs12,
            ("keyStoreType", "JKS") => options.key_store_type = StoreType::Jks,
            ("trustStoreType", "PKCS12") => options.trust_store_type = StoreType::Pkcs12,
            ("trustStoreType", "JKS") => options.trust_store_type = StoreType::Jks,
            ("mode", "new-network") => options.mode = NetworkMode::NewNetwork,
            ("mode", "join-network") => options.mode = NetworkMode::JoinNetwork,
            ("consensusAutoStart", "yes") => options.consensus_auto_start = true,
            ("consensusAutoStart", "no") => options.consensus_auto_start = false,
            ("archiveEnabled", "yes") => options.archive_enabled = true,
            ("archiveEnabled", "no") => options.archive_enabled = false,
            _ => {
                return Err(error(
                    "SETUP_NETWORK_OPTION_INVALID",
                    "표시된 선택지 중 하나를 정확히 입력하세요.",
                ));
            }
        }
    }
    options.validate()?;
    Ok(options)
}

fn check_workspace(workspace: &Path, prepared: &Prepared) -> Result<()> {
    for reference in &prepared.referenced_paths {
        if paths::overlaps(workspace, reference)? {
            return Err(error(
                "SETUP_PATH_CONFLICT",
                "준비 작업 폴더는 제품·네트워크·체인·키 자료 및 데이터 위치와 분리하세요.",
            ));
        }
    }
    Ok(())
}
fn draft_bytes(draft: &Draft) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(draft).map_err(|_| invalid())?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn invalid() -> crate::error::BxdlError {
    error(
        "SETUP_NETWORK_DRAFT_INVALID",
        "네트워크 준비 기록의 형식이 올바르지 않습니다. 기존 자료를 보존했습니다.",
    )
}
fn output_conflict() -> crate::error::BxdlError {
    error(
        "SETUP_OUTPUT_CONFLICT",
        "native 출력은 작업·제품·네트워크·체인·키 자료 및 데이터 경로와 분리하세요.",
    )
}

fn publication_uncertain() -> crate::error::BxdlError {
    error(
        "SETUP_NETWORK_PUBLICATION_UNCERTAIN",
        "native 파일이 이미 생성되었을 수 있으나 저장 완료를 확정할 수 없습니다. 출력 파일과 초안을 보존하고 확인하세요. 자동 재시도하거나 덮어쓰지 않았습니다.",
    )
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
