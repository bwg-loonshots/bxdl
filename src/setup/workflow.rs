//! Explicit new-install orchestration. The workflow never repairs or adopts an
//! unrelated installation and never turns an uncertain engine result into success.
use super::{Session, absolute, error, paths, store, workflow_store::WorkflowStore};
use crate::{
    artifact,
    engine::{instance, setup_plan},
    error::Result,
    install,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Phase {
    #[default]
    Pending,
    InProgress,
    Confirmed,
    Unknown,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Step {
    phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    root: Option<install::RootIdentity>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Inputs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsigned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    revision: u32,
    inputs: Inputs,
    product_path: PathBuf,
    product_sha256: String,
    archive_sha256: String,
    manifest_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_pin: Option<setup_plan::Pin>,
    prepared: setup_plan::Prepared,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    kind: String,
    operation_id: String,
    input_base: PathBuf,
    inputs: Inputs,
    revision: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_sha256: Option<String>,
    generated: Phase,
    installation: Step,
    registration: Step,
    initialization: Phase,
    start: Phase,
}

/// A small public projection, never the private plan or its credential pins.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub mode: &'static str,
    pub workspace: PathBuf,
    pub plan_revision: u32,
    pub installation: String,
    pub registration: String,
    pub initialization: String,
    pub service_state: String,
    pub runtime_readiness: String,
    pub global_consensus: &'static str,
    pub development_only: bool,
}

pub struct Workflow {
    storage: WorkflowStore,
    pub(super) draft: Session,
    journal: Journal,
    observed_initialization: String,
    observed_service: String,
    observed_readiness: String,
}

impl Workflow {
    pub fn create(workspace: &Path, from: Option<&Path>) -> Result<Self> {
        supported()?;
        let workspace = absolute(workspace)?;
        let input_base = absolute(&std::env::current_dir().map_err(|_| invalid())?)?;
        // Validate against the whole container before reserving it, not only draft/.
        let draft = super::initial_draft(&workspace, from, &input_base)?;
        let journal = Journal {
            schema_version: 1,
            kind: "BXDL_INSTALL_WORKFLOW".into(),
            operation_id: operation_id()?,
            input_base,
            inputs: Inputs::default(),
            revision: 0,
            plan: None,
            plan_sha256: None,
            generated: Phase::Pending,
            installation: Step::default(),
            registration: Step::default(),
            initialization: Phase::Pending,
            start: Phase::Pending,
        };
        let raw = serde_json::to_vec(&journal).map_err(|_| invalid())?;
        let storage = WorkflowStore::create(&workspace, &raw, &draft)?;
        Self::load(storage)
    }

    pub fn resume(workspace: &Path) -> Result<Self> {
        supported()?;
        Self::load(WorkflowStore::open(&absolute(workspace)?)?)
    }

    fn load(storage: WorkflowStore) -> Result<Self> {
        let raw = storage.read()?;
        let value = artifact::decode_strict_json(&raw).map_err(|_| invalid())?;
        let journal: Journal = serde_json::from_value(value).map_err(|_| invalid())?;
        let draft = Session::resume(storage.draft_path())?;
        let value = Self {
            storage,
            draft,
            journal,
            observed_initialization: "NOT_OBSERVED".into(),
            observed_service: "NOT_OBSERVED".into(),
            observed_readiness: "NOT_CHECKED".into(),
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<()> {
        let j = &self.journal;
        if j.schema_version != 1
            || j.kind != "BXDL_INSTALL_WORKFLOW"
            || j.operation_id.len() != 32
            || !j.operation_id.bytes().all(|c| c.is_ascii_hexdigit())
            || !j.input_base.is_absolute()
            || absolute(&j.input_base)? != j.input_base
            || j.revision > 1000
            || j.inputs.unsigned == Some(true) && j.inputs.public_key.is_some()
        {
            return Err(invalid());
        }
        for path in input_paths(&j.inputs) {
            if !path.is_absolute() || absolute(path)? != path {
                return Err(invalid());
            }
        }
        if let Some(plan) = &j.plan {
            if plan.revision != j.revision
                || plan.inputs != j.inputs
                || plan.revision == 0
                || plan.product_path != self.product_path(plan.revision)
                || Some(digest(&serde_json::to_vec(plan).map_err(|_| invalid())?)) != j.plan_sha256
            {
                return Err(invalid());
            }
            complete_inputs(&plan.inputs)?;
            self.disjoint(plan)?;
        } else if j.plan_sha256.is_some() || !self.pristine() {
            return Err(invalid());
        }
        if j.installation.phase == Phase::Confirmed
            && (j.installation.root.is_none() || j.generated != Phase::Confirmed)
            || j.registration.phase != Phase::Pending && j.installation.phase != Phase::Confirmed
            || j.registration.phase == Phase::Confirmed && j.registration.root.is_none()
            || j.initialization != Phase::Pending && j.registration.phase != Phase::Confirmed
            || j.start != Phase::Pending && j.initialization != Phase::Confirmed
            || j.installation.phase == Phase::Pending && j.installation.root.is_some()
            || j.registration.phase == Phase::Pending && j.registration.root.is_some()
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        self.validate()?;
        self.storage
            .save(&serde_json::to_vec(&self.journal).map_err(|_| invalid())?)
    }

    pub(super) fn inputs(&self) -> &Inputs {
        &self.journal.inputs
    }
    pub(super) fn workspace(&self) -> &Path {
        self.storage.path()
    }
    pub(super) fn resolve(&self, text: &str) -> Result<PathBuf> {
        if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control) {
            return Err(invalid());
        }
        let path = Path::new(text);
        absolute(&if path.is_absolute() {
            path.to_owned()
        } else {
            self.journal.input_base.join(path)
        })
    }

    pub(super) fn set_inputs(&mut self, inputs: Inputs) -> Result<()> {
        if !self.pristine() {
            return Err(locked_plan());
        }
        self.journal.plan = None;
        self.journal.plan_sha256 = None;
        self.journal.inputs = inputs;
        self.save()
    }

    pub(super) fn edit(&mut self) -> Result<()> {
        if !self.pristine() {
            return Err(locked_plan());
        }
        self.journal.plan = None;
        self.journal.plan_sha256 = None;
        self.observed_initialization = "NOT_OBSERVED".into();
        self.save()
    }

    pub(super) fn set_field(&mut self, name: &str, answer: &str) -> Result<()> {
        if !self.pristine() || self.journal.plan.is_some() {
            return Err(locked_plan());
        }
        if matches!(
            name,
            "dataDirectory"
                | "chainDescription"
                | "validatorKeystore"
                | "validatorPasswordFile"
                | "tlsKeyStore"
                | "tlsKeyPasswordFile"
                | "tlsTrustStore"
                | "tlsTrustPasswordFile"
        ) && paths::overlaps(self.workspace(), &self.resolve(answer)?)?
        {
            return Err(error(
                "SETUP_PATH_CONFLICT",
                "데이터·체인·키 자료는 설치 작업 폴더 밖에 두세요.",
            ));
        }
        self.draft.set(name, answer)
    }

    fn pristine(&self) -> bool {
        self.journal.generated == Phase::Pending
            && self.journal.installation.phase == Phase::Pending
            && self.journal.registration.phase == Phase::Pending
            && self.journal.initialization == Phase::Pending
            && self.journal.start == Phase::Pending
    }

    fn product_path(&self, revision: u32) -> PathBuf {
        self.storage
            .generated_path()
            .join(format!("instance-{revision:04}.json"))
    }

    pub(super) fn has_plan(&self) -> bool {
        self.journal.plan.is_some()
    }

    pub(super) fn check_supply(&self) -> Result<()> {
        let inputs = &self.journal.inputs;
        complete_inputs(inputs)?;
        let package = artifact::verify(required(&inputs.archive)?, &verify_options(inputs))?;
        if package.manifest.platform.os != "darwin" || package.manifest.platform.arch != "arm64" {
            return Err(error(
                "SETUP_PACKAGE_PLATFORM",
                "macOS arm64 패키지를 선택하세요.",
            ));
        }
        setup_plan::check_cli(&package)
    }

    pub(super) fn prepare(&mut self) -> Result<()> {
        if !self.pristine() {
            return Err(locked_plan());
        }
        complete_inputs(&self.journal.inputs)?;
        self.storage.recheck()?;
        let inputs = self.journal.inputs.clone();
        let product = self.draft.config_bytes()?;
        if self.draft.preflight()?.outcome == "FAIL" {
            return Err(error(
                "SETUP_PRODUCT_CHECK_FAILED",
                "제품 설정의 경로·파일·권한 검사를 통과하지 못했습니다. 초안을 수정하세요.",
            ));
        }
        let verified = artifact::verify(required(&inputs.archive)?, &verify_options(&inputs))?;
        if verified.manifest.platform.os != "darwin" || verified.manifest.platform.arch != "arm64" {
            return Err(error(
                "SETUP_PACKAGE_PLATFORM",
                "macOS arm64 패키지를 선택하세요.",
            ));
        }
        setup_plan::check_cli(&verified)?;
        let revision = self
            .journal
            .revision
            .checked_add(1)
            .filter(|r| *r <= 1000)
            .ok_or_else(invalid)?;
        let product_path = self.product_path(revision);
        let prepared = setup_plan::prepare(
            &product,
            &product_path,
            required(&inputs.native)?,
            required(&inputs.lock)?,
            &verified,
        )?;
        let plan = Plan {
            revision,
            trust_pin: inputs
                .public_key
                .as_deref()
                .map(setup_plan::pin)
                .transpose()?,
            inputs,
            product_path,
            product_sha256: digest(&product),
            archive_sha256: verified.archive_sha256,
            manifest_sha256: verified.manifest_sha256,
            prepared,
        };
        self.disjoint(&plan)?;
        require_new_directory(required(&plan.inputs.destination)?)?;
        require_new_directory(required(&plan.inputs.instance)?)?;
        // This install UX intentionally accepts fresh data only. A previous
        // development DB is not an upgrade or a reusable installation target.
        require_new_directory(&plan.prepared.data_directory)?;
        let plan_sha256 = digest(&serde_json::to_vec(&plan).map_err(|_| invalid())?);
        self.journal.revision = revision;
        self.journal.plan = Some(plan);
        self.journal.plan_sha256 = Some(plan_sha256);
        self.save()
    }

    fn disjoint(&self, plan: &Plan) -> Result<()> {
        let outputs = [
            self.workspace(),
            required(&plan.inputs.destination)?,
            required(&plan.inputs.instance)?,
            &plan.prepared.data_directory,
        ];
        for (i, a) in outputs.iter().enumerate() {
            for b in outputs.iter().skip(i + 1) {
                if paths::overlaps(a, b)? {
                    return Err(overlap());
                }
            }
            for input in std::iter::once(required(&plan.inputs.archive)?)
                .chain(plan.inputs.public_key.as_deref())
                .chain(plan.prepared.pins.iter().map(|pin| pin.path.as_path()))
            {
                if paths::overlaps(a, input)? {
                    return Err(overlap());
                }
            }
        }
        Ok(())
    }

    fn plan(&self) -> Result<Plan> {
        self.journal.plan.clone().ok_or_else(invalid)
    }

    fn verify_plan(&self) -> Result<artifact::Report> {
        self.storage.recheck()?;
        self.validate()?;
        let plan = self.plan()?;
        if digest(&self.draft.config_bytes()?) != plan.product_sha256 {
            return Err(changed());
        }
        if self.draft.preflight()?.outcome == "FAIL" {
            return Err(changed());
        }
        setup_plan::recheck(&plan.prepared.pins)?;
        if let Some(pin) = &plan.trust_pin {
            setup_plan::recheck(std::slice::from_ref(pin))?;
        }
        let report = artifact::verify(
            required(&plan.inputs.archive)?,
            &verify_options(&plan.inputs),
        )?;
        if report.archive_sha256 != plan.archive_sha256
            || report.manifest_sha256 != plan.manifest_sha256
        {
            return Err(changed());
        }
        let prepared = setup_plan::prepare(
            &self.draft.config_bytes()?,
            &plan.product_path,
            required(&plan.inputs.native)?,
            required(&plan.inputs.lock)?,
            &report,
        )?;
        if prepared != plan.prepared {
            return Err(changed());
        }
        setup_plan::check_cli(&report)?;
        self.storage.recheck()?;
        Ok(report)
    }

    /// Reconcile only complete, owned results. Incomplete extraction/registration
    /// is kept UNKNOWN. Initialization remains governed by the engine adapter.
    pub(super) fn reconcile(&mut self) -> Result<()> {
        if !self.has_plan() {
            return Ok(());
        }
        let report = self.verify_plan()?;
        let plan = self.plan()?;
        if self.journal.generated != Phase::Pending {
            if setup_plan::pin(&plan.product_path)?.sha256 != plan.product_sha256 {
                return Err(changed());
            }
            self.journal.generated = Phase::Confirmed;
        }
        if self.journal.installation.phase != Phase::Pending {
            let owner = self.journal.installation.root.ok_or_else(unknown)?;
            install::verify_installed_owned(required(&plan.inputs.destination)?, &report, &owner)
                .map_err(|_| unknown())?;
            self.journal.installation.phase = Phase::Confirmed;
        }
        if self.journal.registration.phase != Phase::Pending {
            let owner = self.journal.registration.root.ok_or_else(unknown)?;
            let options = register_options(&plan)?;
            let observed = if self.journal.registration.phase == Phase::Confirmed {
                instance::verify_owned_binding(&options, &owner)?
            } else {
                instance::inspect_owned_registration(&options, &owner).map_err(|_| unknown())?
            };
            self.journal.registration.phase = Phase::Confirmed;
            self.observe_initialization(&observed);
        }
        if self.journal.start != Phase::Pending {
            self.observe_service()?;
        }
        self.save()
    }

    fn observe_initialization(&mut self, value: &instance::Summary) {
        self.observed_initialization = value.initialization.clone();
        match value.initialization.as_str() {
            "INITIALIZED" => self.journal.initialization = Phase::Confirmed,
            "UNKNOWN" => self.journal.initialization = Phase::Unknown,
            "NOT_STARTED" if value.operation_busy == Some(false) => {
                self.journal.initialization = Phase::Pending
            }
            _ => {}
        }
    }

    pub(super) fn apply(&mut self) -> Result<()> {
        let report = self.verify_plan()?;
        let plan = self.plan()?;
        self.storage.require_capacity(12)?;
        if self.journal.generated == Phase::Pending {
            require_new_directory(&plan.prepared.data_directory)?;
            self.journal.generated = Phase::InProgress;
            self.save()?;
            store::write_new(&plan.product_path, &self.draft.config_bytes()?)?;
            self.journal.generated = Phase::Confirmed;
            self.save()?;
        }
        if self.journal.generated != Phase::Confirmed {
            return Err(unknown());
        }
        if self.journal.installation.phase == Phase::Pending {
            self.verify_plan()?;
            require_new_directory(required(&plan.inputs.destination)?)?;
            self.journal.installation.phase = Phase::InProgress;
            self.save()?;
            let result = install::install_owned(
                required(&plan.inputs.archive)?,
                required(&plan.inputs.destination)?,
                &verify_options(&plan.inputs),
                |identity| {
                    self.journal.installation.root = Some(identity);
                    self.save()
                },
            );
            let validated = result.and_then(|installed| {
                if installed.archive_sha256 != plan.archive_sha256
                    || installed.manifest_sha256 != plan.manifest_sha256
                {
                    return Err(changed());
                }
                let current = self.verify_plan()?;
                install::verify_installed_owned(
                    required(&plan.inputs.destination)?,
                    &current,
                    &self.journal.installation.root.ok_or_else(unknown)?,
                )
            });
            match validated {
                Ok(()) => self.journal.installation.phase = Phase::Confirmed,
                Err(failure) => {
                    self.journal.installation.phase = if self.journal.installation.root.is_none()
                        && absent(required(&plan.inputs.destination)?)?
                    {
                        Phase::Pending
                    } else {
                        Phase::Unknown
                    };
                    self.save()?;
                    return Err(failure);
                }
            }
            self.save()?;
        }
        if self.journal.installation.phase != Phase::Confirmed {
            return Err(unknown());
        }
        let owner = self.journal.installation.root.ok_or_else(unknown)?;
        install::verify_installed_owned(required(&plan.inputs.destination)?, &report, &owner)?;
        if self.journal.registration.phase == Phase::Pending {
            self.verify_plan()?;
            require_new_directory(required(&plan.inputs.instance)?)?;
            require_new_directory(&plan.prepared.data_directory)?;
            self.journal.registration.phase = Phase::InProgress;
            self.save()?;
            let result = instance::register_owned(&register_options(&plan)?, |identity| {
                self.journal.registration.root = Some(identity);
                self.save()
            });
            let validated = result.and_then(|_| {
                self.verify_plan()?;
                instance::inspect_owned_registration(
                    &register_options(&plan)?,
                    &self.journal.registration.root.ok_or_else(unknown)?,
                )
            });
            match validated {
                Ok(value) => {
                    self.journal.registration.phase = Phase::Confirmed;
                    self.observe_initialization(&value);
                }
                Err(failure) => {
                    // A cold failure before reservation has no registration side
                    // effect. It can be attempted again only after explicit apply.
                    self.journal.registration.phase = if self.journal.registration.root.is_none()
                        && absent(required(&plan.inputs.instance)?)?
                    {
                        Phase::Pending
                    } else {
                        Phase::Unknown
                    };
                    self.save()?;
                    return Err(failure);
                }
            }
            self.save()?;
        }
        Ok(())
    }

    pub(super) fn initialize(&mut self, resume: bool) -> Result<()> {
        self.reconcile()?;
        if self.journal.registration.phase != Phase::Confirmed {
            return Err(unknown());
        }
        self.storage.require_capacity(3)?;
        let plan = self.plan()?;
        if !resume {
            require_new_directory(&plan.prepared.data_directory)?;
        }
        self.journal.initialization = Phase::InProgress;
        self.save()?;
        match instance::initialize(required(&plan.inputs.instance)?, resume, TIMEOUT) {
            Ok(observed) => {
                self.observe_initialization(&observed);
                self.save()?;
            }
            Err(failure) => {
                self.journal.initialization = Phase::Unknown;
                self.save()?;
                return Err(failure);
            }
        }
        Ok(())
    }

    pub(super) fn start(&mut self) -> Result<()> {
        self.reconcile()?;
        if self.journal.initialization != Phase::Confirmed || self.journal.start != Phase::Pending {
            return Err(unknown());
        }
        self.storage.require_capacity(3)?;
        let plan = self.plan()?;
        self.journal.start = Phase::InProgress;
        self.save()?;
        match instance::service::start(required(&plan.inputs.instance)?, TIMEOUT) {
            Ok(summary) => self.set_service(summary),
            Err(failure) => {
                self.journal.start = Phase::Unknown;
                self.save()?;
                return Err(failure);
            }
        }
        self.save()
    }

    fn set_service(&mut self, value: instance::service::ServiceSummary) {
        self.journal.start = if value.reason == "SERVICE_NOT_STARTED" && !value.operation_busy {
            Phase::Pending
        } else if value.service_state == "RUNNING" && value.engine_state == "RUNNING" {
            Phase::Confirmed
        } else {
            Phase::Unknown
        };
        self.observed_service = value.service_state;
        self.observed_readiness = value.runtime_readiness;
    }

    pub(super) fn observe_service(&mut self) -> Result<()> {
        let plan = self.plan()?;
        let summary =
            instance::service::status(required(&plan.inputs.instance)?, Duration::from_secs(15))?;
        self.set_service(summary);
        Ok(())
    }

    pub(super) fn can_apply(&self) -> bool {
        self.has_plan() && self.journal.registration.phase == Phase::Pending
    }
    pub(super) fn can_init(&self) -> bool {
        self.journal.registration.phase == Phase::Confirmed
            && self.journal.initialization == Phase::Pending
    }
    pub(super) fn can_resume_init(&self) -> bool {
        self.journal.registration.phase == Phase::Confirmed
            && matches!(
                self.journal.initialization,
                Phase::Unknown | Phase::InProgress
            )
    }
    pub(super) fn can_start(&self) -> bool {
        self.journal.initialization == Phase::Confirmed && self.journal.start == Phase::Pending
    }
    pub(super) fn can_edit(&self) -> bool {
        self.pristine()
    }

    pub fn summary(&self) -> Summary {
        let label = |phase: Phase| {
            match phase {
                Phase::Pending => "NOT_STARTED",
                Phase::Confirmed => "CONFIRMED",
                _ => "UNKNOWN",
            }
            .to_owned()
        };
        Summary {
            mode: "INSTALL",
            workspace: self.workspace().to_owned(),
            plan_revision: self.journal.revision,
            installation: label(self.journal.installation.phase),
            registration: label(self.journal.registration.phase),
            initialization: if self.journal.initialization == Phase::Confirmed {
                "INITIALIZED".into()
            } else if self.journal.initialization == Phase::Pending {
                "NOT_STARTED".into()
            } else {
                "UNKNOWN".into()
            },
            service_state: self.observed_service.clone(),
            runtime_readiness: self.observed_readiness.clone(),
            global_consensus: "NOT_CHECKED",
            development_only: true,
        }
    }

    pub(super) fn show_plan(&self, out: &mut dyn Write) -> Result<()> {
        let plan = self.plan()?;
        let text = format!(
            "\n설치 계획 #{} — 새 개발용 노드\n엔진 source: {} (dirty={})\n패키지 설치: {}\n제품 설정: {}\n인스턴스 기록: {}\n새 데이터: {}\n자동 GC: {}\n자산 발행자 허용목록: {}\n기존 DB 업그레이드가 아닙니다. cold 검사는 설치 후 수행하며 네트워크 합의는 별도 확인합니다.\n원본 archive·공개키·lock·native·chain·키 참조와 이 작업 폴더는 등록 후에도 보존하세요.\n",
            plan.revision,
            plan.prepared.identity.source.commit,
            plan.prepared.identity.source.dirty,
            required(&plan.inputs.destination)?.display(),
            plan.product_path.display(),
            required(&plan.inputs.instance)?.display(),
            plan.prepared.data_directory.display(),
            if plan.prepared.automatic_gc {
                "사용자 입력에서 ON — 운영자 개입이 필요할 수 있음"
            } else {
                "OFF"
            },
            plan.prepared
                .issuer_count
                .map_or("미제공 — native/chain 설정 확인".into(), |n| format!(
                    "{n}개 (체인 초기화 전에 확인)"
                ))
        );
        out.write_all(text.as_bytes()).map_err(|_| output_failed())
    }
}

pub fn interact(
    workflow: &mut Workflow,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<bool> {
    super::install_wizard::interact(workflow, input, out)
}

fn input_paths(inputs: &Inputs) -> impl Iterator<Item = &Path> {
    [
        &inputs.archive,
        &inputs.public_key,
        &inputs.lock,
        &inputs.native,
        &inputs.destination,
        &inputs.instance,
    ]
    .into_iter()
    .filter_map(|v| v.as_deref())
}
fn complete_inputs(i: &Inputs) -> Result<()> {
    for p in [&i.archive, &i.lock, &i.native, &i.destination, &i.instance] {
        required(p)?;
    }
    if i.unsigned.is_none()
        || i.unsigned == Some(false) && i.public_key.is_none()
        || i.unsigned == Some(true) && i.public_key.is_some()
    {
        return Err(invalid());
    }
    Ok(())
}
fn required(path: &Option<PathBuf>) -> Result<&Path> {
    path.as_deref().ok_or_else(invalid)
}
fn verify_options(inputs: &Inputs) -> artifact::VerifyOptions {
    artifact::VerifyOptions {
        public_key_path: inputs.public_key.clone(),
        allow_unsigned_development: inputs.unsigned == Some(true),
    }
}
fn register_options(plan: &Plan) -> Result<instance::RegisterOptions> {
    Ok(instance::RegisterOptions {
        instance: required(&plan.inputs.instance)?.to_owned(),
        package: required(&plan.inputs.destination)?.to_owned(),
        archive: required(&plan.inputs.archive)?.to_owned(),
        public_key: plan.inputs.public_key.clone(),
        allow_unsigned_development: plan.inputs.unsigned == Some(true),
        product: plan.product_path.clone(),
        native: required(&plan.inputs.native)?.to_owned(),
        lock: required(&plan.inputs.lock)?.to_owned(),
        timeout: TIMEOUT,
    })
}
fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Ok(_) => Ok(false),
        Err(_) => Err(invalid()),
    }
}
fn require_new_directory(path: &Path) -> Result<()> {
    paths::same(path, path)?;
    let parent = path.parent().ok_or_else(invalid)?;
    if !absent(path)? {
        return Err(error(
            "SETUP_TARGET_EXISTS",
            "새 설치·등록·데이터 경로가 필요합니다. 기존 자료는 보존했습니다.",
        ));
    }
    if !fs::metadata(parent).is_ok_and(|m| m.is_dir()) {
        return Err(error(
            "SETUP_PARENT_REQUIRED",
            "목적지의 기존 부모 폴더를 준비하거나 다른 경로를 선택하세요.",
        ));
    }
    Ok(())
}
fn digest(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn operation_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|_| invalid())?;
    Ok(hex::encode(bytes))
}
fn supported() -> Result<()> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(error(
            "SETUP_INSTALL_PLATFORM_UNSUPPORTED",
            "설치 도우미는 현재 macOS arm64에서 지원합니다.",
        ));
    }
    Ok(())
}
fn invalid() -> crate::error::BxdlError {
    error(
        "SETUP_WORKFLOW_INVALID",
        "설치 작업 기록 또는 입력이 올바르지 않습니다. 기존 자료를 보존했습니다.",
    )
}
fn changed() -> crate::error::BxdlError {
    error(
        "SETUP_PLAN_CHANGED",
        "설치 계획 이후 입력이 변경됐습니다. 실행 전 계획을 다시 만들거나 기존 등록을 별도로 확인하세요.",
    )
}
fn locked_plan() -> crate::error::BxdlError {
    error(
        "SETUP_PLAN_LOCKED",
        "출력 생성 이후에는 이 설치 작업의 설정을 변경할 수 없습니다. 기존 결과를 보존하세요.",
    )
}
fn unknown() -> crate::error::BxdlError {
    error(
        "SETUP_OPERATION_UNKNOWN",
        "작업 완료를 확정할 수 없습니다. 기존 결과를 보존하고 상태를 확인하세요. 자동 재실행하지 않았습니다.",
    )
}
fn overlap() -> crate::error::BxdlError {
    error(
        "SETUP_PATH_CONFLICT",
        "작업·설치·인스턴스·데이터와 원본 입력 경로는 서로 분리하세요.",
    )
}
fn output_failed() -> crate::error::BxdlError {
    error(
        "SETUP_OUTPUT_FAILED",
        "안내를 출력하지 못했습니다. 작업 기록을 보존했습니다.",
    )
}
