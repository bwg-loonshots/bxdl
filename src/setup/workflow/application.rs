//! Approved preparation execution without wizard edits or runtime actions.
use super::*;

/// Recorded journal stages remain distinct from this read's verification.
/// This projection deliberately excludes the source binding and credential pins.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApplicationSummary {
    pub operation_id: String,
    pub preparation_revision: u32,
    pub instance: PathBuf,
    pub installation: String,
    pub registration: String,
    pub initialization: String,
    pub registration_confirmed: bool,
    pub source_match: &'static str,
    pub verification: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_code: Option<String>,
}

impl Workflow {
    /// The first durable journal already names the approved source. A failed
    /// later preflight leaves a bound operation, never an adoptable empty one.
    pub(crate) fn create_prepared(workspace: &Path, source: &ApplicationSource) -> Result<Self> {
        supported()?;
        source.recheck(true)?;
        let workspace = absolute(workspace)?;
        source.check_location(&workspace)?;
        let input_base = absolute(&std::env::current_dir().map_err(|_| invalid())?)?;
        let draft = super::super::initial_draft(&workspace, Some(&source.product), &input_base)?;
        let journal = Journal {
            schema_version: 1,
            kind: "BXDL_INSTALL_WORKFLOW".into(),
            operation_id: operation_id()?,
            origin: Some(source.clone()),
            input_base,
            inputs: source.inputs.clone(),
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
        let flow = Self::load(storage)?;
        flow.check_origin(source)?;
        Ok(flow)
    }

    pub(crate) fn resume_prepared(workspace: &Path, source: &ApplicationSource) -> Result<Self> {
        supported()?;
        let flow = Self::load(WorkflowStore::open(&absolute(workspace)?)?)?;
        flow.check_origin(source)?;
        Ok(flow)
    }

    fn check_origin(&self, source: &ApplicationSource) -> Result<()> {
        self.storage.recheck()?;
        self.validate()?;
        source.recheck(false)?;
        if self.journal.origin.as_ref() != Some(source) {
            return Err(source_changed());
        }
        Ok(())
    }

    /// Caller retains its preparation/home/catalog snapshots and rechecks them
    /// after confirmation and before each new mutation. Owner callbacks always
    /// persist ownership once reserved, even if a later verification fails.
    pub(crate) fn apply_prepared(
        &mut self,
        source: &ApplicationSource,
        mut guard: impl FnMut() -> Result<()>,
    ) -> Result<ApplicationSummary> {
        self.check_origin(source)?;
        guard()?;
        if !self.has_plan() {
            source.recheck(true)?;
            guard()?;
            self.prepare()?;
        }
        // Preparation.check requires absent destinations. Once a bound plan
        // exists, Workflow verifies all still-required original inputs and its
        // owned outputs; rerunning the new-install preflight would reject a
        // legitimate partial installation merely because it owns its root.
        source.recheck(false)?;
        // This adapter never initializes or observes/starts a service. Complete
        // externally initialized registrations are verified from retained data.
        guard()?;
        self.reconcile_application()?;
        let mut guarded = || {
            source.recheck(false)?;
            guard()
        };
        if self.journal.registration.phase != Phase::Confirmed {
            self.apply_with_guard(&mut guarded)?;
        }
        self.check_origin(source)?;
        self.verify_application()?;
        let mut summary = self.application_summary();
        summary.source_match = "MATCH";
        summary.verification = "PASSED";
        Ok(summary)
    }

    /// Reconcile only partial owned install/registration checkpoints. This
    /// excludes runtime observation and preserves recorded initialization.
    fn reconcile_application(&mut self) -> Result<()> {
        if [
            self.journal.generated,
            self.journal.installation.phase,
            self.journal.registration.phase,
        ]
        .into_iter()
        .all(|phase| matches!(phase, Phase::Pending | Phase::Confirmed))
        {
            return Ok(());
        }
        let report = self.verify_plan()?;
        let plan = self.plan()?;
        if matches!(self.journal.generated, Phase::InProgress | Phase::Unknown) {
            if setup_plan::pin(&plan.product_path)?.sha256 != plan.product_sha256 {
                return Err(unknown());
            }
            self.journal.generated = Phase::Confirmed;
        }
        if matches!(
            self.journal.installation.phase,
            Phase::InProgress | Phase::Unknown
        ) {
            let owner = self.journal.installation.root.ok_or_else(unknown)?;
            install::verify_installed_owned(required(&plan.inputs.destination)?, &report, &owner)
                .map_err(|_| unknown())?;
            self.journal.installation.phase = Phase::Confirmed;
        }
        if matches!(
            self.journal.registration.phase,
            Phase::InProgress | Phase::Unknown
        ) {
            let owner = self.journal.registration.root.ok_or_else(unknown)?;
            instance::inspect_owned_registration(&register_options(&plan)?, &owner)
                .map_err(|_| unknown())?;
            self.journal.registration.phase = Phase::Confirmed;
        }
        self.save()
    }

    fn verify_application(&self) -> Result<()> {
        let report = self.verify_plan()?;
        let plan = self.plan()?;
        if self.journal.generated != Phase::Pending
            && (self.journal.generated != Phase::Confirmed
                || setup_plan::pin(&plan.product_path)?.sha256 != plan.product_sha256)
        {
            return Err(unknown());
        }
        if self.journal.installation.phase != Phase::Pending {
            if self.journal.installation.phase != Phase::Confirmed {
                return Err(unknown());
            }
            install::verify_installed_owned(
                required(&plan.inputs.destination)?,
                &report,
                &self.journal.installation.root.ok_or_else(unknown)?,
            )?;
        }
        if self.journal.registration.phase != Phase::Pending {
            if self.journal.registration.phase != Phase::Confirmed {
                return Err(unknown());
            }
            instance::verify_owned_binding(
                &register_options(&plan)?,
                &self.journal.registration.root.ok_or_else(unknown)?,
            )?;
        }
        self.storage.recheck()
    }

    fn application_summary(&self) -> ApplicationSummary {
        ApplicationSummary {
            operation_id: self.journal.operation_id.clone(),
            preparation_revision: self
                .journal
                .origin
                .as_ref()
                .map_or(0, |source| source.revision),
            instance: self.journal.inputs.instance.clone().unwrap_or_default(),
            installation: phase(self.journal.installation.phase).into(),
            registration: phase(self.journal.registration.phase).into(),
            initialization: phase(self.journal.initialization).into(),
            registration_confirmed: self.journal.registration.phase == Phase::Confirmed,
            source_match: "NOT_CHECKED",
            verification: "NOT_CHECKED",
            verification_code: None,
        }
    }

    pub(crate) fn inspect_prepared(
        workspace: &Path,
        source: &ApplicationSource,
        revalidate: bool,
    ) -> Result<ApplicationSummary> {
        Self::inspect_application(workspace, Some(source), revalidate)
    }

    /// No writer lock, reconciliation, engine call or service observation. A
    /// changed/missing preparation must not hide existing recorded side effects.
    pub(crate) fn inspect_application(
        workspace: &Path,
        source: Option<&ApplicationSource>,
        revalidate: bool,
    ) -> Result<ApplicationSummary> {
        let flow = Self::load(WorkflowStore::inspect(&absolute(workspace)?)?)?;
        let origin = flow.journal.origin.as_ref().ok_or_else(source_changed)?;
        let mut summary = flow.application_summary();
        summary.source_match = match source {
            Some(source) if source == origin => "MATCH",
            Some(_) => "MISMATCH",
            None => "UNAVAILABLE",
        };
        if revalidate {
            let check = if summary.source_match != "MATCH" {
                Err(source_changed())
            } else {
                origin.recheck(false).and_then(|_| {
                    if flow.has_plan() {
                        flow.verify_application()
                    } else {
                        origin.recheck(true)
                    }
                })
            };
            match check {
                Ok(()) => summary.verification = "PASSED",
                Err(failure) => {
                    summary.verification = "FAILED";
                    summary.verification_code = Some(failure.code);
                }
            }
        }
        flow.storage.recheck()?;
        Ok(summary)
    }
}

pub(super) fn check_plan_origin(plan: &Plan, source: &ApplicationSource) -> Result<()> {
    if plan.inputs != source.inputs
        || digest(&serde_json::to_vec(&(&plan.prepared, &plan.trust_pin)).map_err(|_| invalid())?)
            != source.plan_binding_sha256
        || plan.product_sha256 != source.product_sha256
        || plan.archive_sha256 != source.archive_sha256
        || plan.manifest_sha256 != source.manifest_sha256
        || plan.prepared.data_directory != source.data_directory
        || !plan
            .prepared
            .pins
            .iter()
            .any(|pin| pin.path == source.native && pin.sha256 == source.native_sha256)
    {
        return Err(source_changed());
    }
    Ok(())
}

pub(super) fn source_changed() -> crate::error::BxdlError {
    error(
        "SETUP_APPLICATION_SOURCE_CHANGED",
        "설치 실행과 준비 기록의 연결이 일치하지 않습니다. 기존 실행 기록과 출력은 보존합니다.",
    )
}

fn phase(value: Phase) -> &'static str {
    match value {
        Phase::Pending => "PENDING",
        Phase::InProgress => "IN_PROGRESS",
        Phase::Confirmed => "CONFIRMED",
        Phase::Unknown => "UNKNOWN",
    }
}
