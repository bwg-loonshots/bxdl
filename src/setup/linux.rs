//! User-owned Linux preparation. Root installation never opens or writes this
//! journal/catalog. Requests are immutable, individually approved revisions.
use super::{Session, absolute, store, workflow_store::WorkflowStore};
use crate::{
    artifact::decode_strict_json,
    engine::linux_service::{install, management},
    error::{BxdlError, Result},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_jar: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_config: Option<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reviewed {
    revision: u32,
    request: PathBuf,
    sha256: String,
    product_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    kind: String,
    name: String,
    operator_uid: u32,
    sources: Sources,
    revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    review: Option<Reviewed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    apply_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    installed: Option<management::Descriptor>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub name: String,
    pub platform: &'static str,
    pub preparation: &'static str,
    pub installation: &'static str,
    pub missing_inputs: Vec<String>,
    pub missing_fields: Vec<String>,
    pub revision: u32,
    pub control: PathBuf,
    pub data_directory: PathBuf,
    pub service_account: String,
    pub engine_validation: &'static str,
    pub runtime_readiness: &'static str,
    pub development_only: bool,
}
pub struct Preparation {
    storage: WorkflowStore,
    draft: Session,
    record: Record,
}
fn invalid() -> BxdlError {
    BxdlError::new(
        "LINUX_PREPARATION_INVALID",
        "Linux 설치 준비 기록을 확인할 수 없습니다. 기존 자료를 보존했습니다.",
    )
}
fn frozen() -> BxdlError {
    BxdlError::new(
        "LINUX_PREPARATION_APPLY_UNRESOLVED",
        "설치 요청 후에는 준비 입력을 바꾸지 않습니다. 같은 계획의 설치 기록을 확인하거나 다른 이름으로 준비하세요.",
    )
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| invalid())
}
fn digest(raw: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(raw))
}
#[cfg(test)]
thread_local! {
    // Synthetic user-journal tests run on macOS too. Production always uses
    // /var/lib; this override never participates in the root installer.
    static TEST_CONTROL_BASE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}
pub fn control(name: &str) -> Result<PathBuf> {
    if !crate::operator::catalog::valid_name(name) {
        return Err(invalid());
    }
    let base = PathBuf::from("/var/lib/bxdl/instances");
    #[cfg(test)]
    let base = TEST_CONTROL_BASE
        .with(|value| value.borrow().clone())
        .unwrap_or(base);
    Ok(base.join(name))
}
fn initial_draft(path: &Path, name: &str, from: Option<&Path>) -> Result<Vec<u8>> {
    let base = absolute(&std::env::current_dir().map_err(|_| invalid())?)?;
    let raw = super::initial_draft(path, from, &base)?;
    let mut draft: super::Draft = decode_strict_json(&raw).map_err(|_| invalid())?;
    draft.answers.insert("instanceId".into(), name.into());
    draft.answers.insert(
        "dataDirectory".into(),
        control(name)?
            .join("data")
            .to_str()
            .ok_or_else(invalid)?
            .into(),
    );
    super::check_draft(&draft, path)?;
    encode(&draft)
}
fn user() -> Result<u32> {
    let uid = rustix::process::getuid().as_raw();
    if uid == 0 || uid != rustix::process::geteuid().as_raw() {
        return Err(BxdlError::new(
            "LINUX_SETUP_USER_REQUIRED",
            "setup은 일반 사용자로 실행하세요. 승인한 시스템 설치 단계에서만 관리자 인증을 요청합니다.",
        ));
    }
    Ok(uid)
}
impl Preparation {
    pub fn validate_new(path: &Path, name: &str, from: Option<&Path>) -> Result<()> {
        user()?;
        control(name)?;
        let path = absolute(path)?;
        initial_draft(&path, name, from)?;
        if let Some(from) = from {
            if super::paths::overlaps(&path, &absolute(from)?)? {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub fn create(path: &Path, name: &str, from: Option<&Path>) -> Result<Self> {
        Self::validate_new(path, name, from)?;
        let path = absolute(path)?;
        let draft = initial_draft(&path, name, from)?;
        let record = Record {
            schema_version: 1,
            kind: "BXDL_LINUX_INSTALL_PREPARATION".into(),
            name: name.into(),
            operator_uid: user()?,
            sources: Sources::default(),
            revision: 0,
            review: None,
            apply_intent: None,
            installed: None,
        };
        let storage = WorkflowStore::create(&path, &encode(&record)?, &draft)?;
        Self::load(storage)
    }
    pub fn open(path: &Path) -> Result<Self> {
        Self::load(WorkflowStore::open(&absolute(path)?)?)
    }
    pub fn inspect(path: &Path) -> Result<Summary> {
        Self::load(WorkflowStore::inspect(&absolute(path)?)?)?.summary()
    }
    fn load(storage: WorkflowStore) -> Result<Self> {
        let record: Record = decode_strict_json(&storage.read()?).map_err(|_| invalid())?;
        let draft = Session::resume(storage.draft_path())?;
        let s = Self {
            storage,
            draft,
            record,
        };
        s.recheck()?;
        Ok(s)
    }
    pub fn recheck(&self) -> Result<()> {
        self.storage.recheck()?;
        let r = &self.record;
        if r.schema_version != 1
            || r.kind != "BXDL_LINUX_INSTALL_PREPARATION"
            || r.operator_uid != user()?
            || r.revision > 1000
        {
            return Err(invalid());
        }
        let data = control(&r.name)?.join("data");
        if !self.draft.answer_matches("instanceId", &r.name)
            || !self
                .draft
                .answer_matches("dataDirectory", data.to_str().ok_or_else(invalid)?)
        {
            return Err(invalid());
        }
        if self.storage.read()? != encode(r)? {
            return Err(invalid());
        }
        for p in [
            &r.sources.runtime_root,
            &r.sources.engine_jar,
            &r.sources.native_config,
        ]
        .into_iter()
        .flatten()
        {
            if !p.is_absolute()
                || super::paths::overlaps(self.path(), p)?
                || super::paths::overlaps(&control(&r.name)?, p)?
            {
                return Err(invalid());
            }
        }
        if let Some(review) = &r.review {
            if review.revision != r.revision
                || review.request != self.request_path(review.revision)
                || review.sha256.len() != 64
            {
                return Err(invalid());
            }
        } else if r.apply_intent.is_some() || r.installed.is_some() {
            return Err(invalid());
        }
        if r.apply_intent
            .as_ref()
            .is_some_and(|sha| r.review.as_ref().is_none_or(|review| sha != &review.sha256))
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        self.storage.save(&encode(&self.record)?)?;
        self.recheck()
    }
    pub fn path(&self) -> &Path {
        self.storage.path()
    }
    pub fn name(&self) -> &str {
        &self.record.name
    }
    pub fn draft(&self) -> &Session {
        &self.draft
    }
    pub fn sources(&self) -> &Sources {
        &self.record.sources
    }
    pub fn field_value(&self, name: &str) -> Option<String> {
        self.draft.draft.answers.get(name).cloned()
    }
    pub fn installed(&self) -> Option<&management::Descriptor> {
        self.record.installed.as_ref()
    }
    pub fn set_source(&mut self, field: &str, value: &Path) -> Result<()> {
        self.recheck()?;
        if self.record.apply_intent.is_some() {
            return Err(frozen());
        }
        let path = absolute(value)?;
        if super::paths::overlaps(self.path(), &path)?
            || super::paths::overlaps(&control(self.name())?, &path)?
        {
            return Err(invalid());
        }
        let source = match field {
            "runtimeRoot" => &mut self.record.sources.runtime_root,
            "engineJar" => &mut self.record.sources.engine_jar,
            "nativeConfig" => &mut self.record.sources.native_config,
            _ => return Err(invalid()),
        };
        if source.as_ref() == Some(&path) {
            return Ok(());
        }
        *source = Some(path);
        self.record.review = None;
        self.save()
    }
    pub fn set_field(&mut self, field: &str, value: &str) -> Result<()> {
        self.recheck()?;
        if self.record.apply_intent.is_some() {
            return Err(frozen());
        }
        if matches!(field, "instanceId" | "dataDirectory") {
            return Err(BxdlError::new(
                "LINUX_SETUP_FIXED_LOCATION",
                "Linux 인스턴스 이름과 데이터 위치는 준비 이름에 따라 정해집니다. 새 이름은 setup new로 준비하세요.",
            ));
        }
        self.draft.validate_answer(field, value)?;
        if self.draft.answer_matches(field, value) {
            return Ok(());
        }
        // Invalidate approval first; a crash cannot retain an old approved draft.
        self.record.review = None;
        self.save()?;
        self.draft.set(field, value)
    }
    fn request_path(&self, revision: u32) -> PathBuf {
        self.storage
            .generated_path()
            .join(format!("linux-request-{revision}.json"))
    }
    pub fn prepare(&mut self) -> Result<Summary> {
        self.recheck()?;
        if self.record.apply_intent.is_some() {
            return Err(frozen());
        }
        if self.record.review.is_some() {
            let (_, _, approved) = self.reviewed_request()?;
            if install::recheck(&approved).is_ok() {
                return self.summary();
            }
            // An explicit review confirmation may pin new source bytes at the
            // same paths. Preserve the old immutable files but invalidate their
            // approval before attempting to publish a new revision.
        }
        let product = self.draft.config_bytes()?;
        let required = |path: &Option<PathBuf>| {
            path.clone().ok_or_else(|| {
                BxdlError::new(
                    "LINUX_PREPARATION_INCOMPLETE",
                    "runtime/JAR/native 설정과 제품 항목을 먼저 준비하세요.",
                )
            })
        };
        let runtime_root = required(&self.record.sources.runtime_root)?;
        let engine_jar = required(&self.record.sources.engine_jar)?;
        let native_config = required(&self.record.sources.native_config)?;
        self.storage.require_capacity(2)?;
        self.record.review = None;
        self.record.revision = self
            .record
            .revision
            .checked_add(1)
            .filter(|v| *v <= 1000)
            .ok_or_else(invalid)?;
        self.save()?;
        let product_config = self
            .storage
            .generated_path()
            .join(format!("linux-product-{}.json", self.record.revision));
        store::write_new(&product_config, &product)?;
        let request = install::prepare(
            self.name(),
            &install::Inputs {
                runtime_root,
                engine_jar,
                native_config,
                product_config,
            },
            self.record.operator_uid,
        )?;
        let path = self.request_path(self.record.revision);
        let raw = encode(&request)?;
        let sha = install::digest(&request)?;
        if digest(&raw) != sha {
            return Err(invalid());
        }
        store::write_new(&path, &raw)?;
        self.record.review = Some(Reviewed {
            revision: self.record.revision,
            request: path,
            sha256: sha,
            product_sha256: digest(&product),
        });
        self.save()?;
        self.summary()
    }
    pub fn reviewed_request(&self) -> Result<(PathBuf, String, install::Request)> {
        self.recheck()?;
        let r = self.record.review.as_ref().ok_or_else(|| {
            BxdlError::new(
                "LINUX_PREPARATION_NOT_REVIEWED",
                "setup resume으로 자료를 검증하고 설치 준비를 저장하세요.",
            )
        })?;
        if digest(&self.draft.config_bytes()?) != r.product_sha256 {
            return Err(invalid());
        }
        let raw = crate::engine::linux_service::install::read_request_user(
            &r.request,
            self.record.operator_uid,
        )?;
        if digest(&raw) != r.sha256 {
            return Err(invalid());
        }
        let request: install::Request = decode_strict_json(&raw).map_err(|_| invalid())?;
        install::validate(&request)?;
        if request.name != self.record.name
            || request.operator_uid != self.record.operator_uid
            || request.control != control(self.name())?
            || install::digest(&request)? != r.sha256
        {
            return Err(invalid());
        }
        Ok((r.request.clone(), r.sha256.clone(), request))
    }
    pub fn begin_apply(&mut self) -> Result<(PathBuf, String, install::Request)> {
        let request = self.reviewed_request()?;
        if self.record.apply_intent.is_none() {
            self.record.apply_intent = Some(request.1.clone());
            self.save()?;
        }
        Ok(request)
    }
    pub fn mark_installed(&mut self, descriptor: management::Descriptor) -> Result<()> {
        let (_, _, request) = self.reviewed_request()?;
        if descriptor.control_directory != request.control
            || descriptor.instance_id != self.record.name
            || self.record.apply_intent.is_none()
        {
            return Err(invalid());
        }
        let current = management::inspect(&request.control)?;
        if management::descriptor_sha256(&current)? != management::descriptor_sha256(&descriptor)? {
            return Err(invalid());
        }
        self.record.installed = Some(descriptor);
        self.save()
    }
    pub fn summary(&self) -> Result<Summary> {
        let mut missing = Vec::new();
        for (name, p) in [
            ("runtimeRoot", &self.record.sources.runtime_root),
            ("engineJar", &self.record.sources.engine_jar),
            ("nativeConfig", &self.record.sources.native_config),
        ] {
            if p.is_none() {
                missing.push(name.into())
            }
        }
        let control = control(self.name())?;
        Ok(Summary {
            name: self.record.name.clone(),
            platform: "linux-amd64",
            preparation: if self.record.review.is_some() {
                "REVIEWED"
            } else {
                "DRAFT"
            },
            installation: if self.record.installed.is_some() {
                "REGISTERED"
            } else if self.record.apply_intent.is_some() {
                "UNRESOLVED"
            } else {
                "NOT_STARTED"
            },
            missing_inputs: missing,
            missing_fields: super::FIELDS
                .iter()
                .filter(|f| !self.draft.has_answer(f.name))
                .map(|f| f.name.into())
                .collect(),
            revision: self.record.revision,
            data_directory: control.join("data"),
            service_account: crate::engine::linux_service::service_account(&control)?,
            control,
            engine_validation: if self.record.installed.is_some() {
                "COLD_CHECKED_AT_REGISTRATION"
            } else if self.record.apply_intent.is_some() {
                "NOT_OBSERVED"
            } else {
                "NOT_RUN"
            },
            runtime_readiness: "NOT_CHECKED",
            development_only: true,
        })
    }
}

#[cfg(test)]
mod tests;
