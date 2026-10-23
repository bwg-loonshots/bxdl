//! Immutable preparation-to-installation contract. Only this module constructs
//! a source from an approved preparation. Public summaries never serialize it.
use super::*;
use crate::setup::{workflow, workflow_store::ContainerIdentity};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ApplicationSource {
    pub(crate) preparation: PathBuf,
    identity: ContainerIdentity,
    pub(crate) revision: u32,
    review_sha256: String,
    pub(crate) plan_binding_sha256: String,
    pub(crate) product: PathBuf,
    pub(crate) native: PathBuf,
    pub(crate) product_sha256: String,
    pub(crate) native_sha256: String,
    pub(crate) archive_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) data_directory: PathBuf,
    pub(crate) inputs: workflow::Inputs,
}

impl ApplicationSource {
    pub(crate) fn destination(&self) -> &Path {
        self.inputs
            .destination
            .as_deref()
            .expect("validated application destination")
    }
    pub(crate) fn instance(&self) -> &Path {
        self.inputs
            .instance
            .as_deref()
            .expect("validated application control")
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.revision == 0
            || self.revision > 1000
            || !self.preparation.is_absolute()
            || absolute(&self.preparation)? != self.preparation
            || self.product
                != self
                    .preparation
                    .join("generated")
                    .join(format!("instance-{:04}.json", self.revision))
            || self.native
                != self
                    .preparation
                    .join("generated")
                    .join(format!("node-{:04}.json", self.revision))
            || self.inputs.native.as_ref() != Some(&self.native)
            || self.inputs.destination.is_none()
            || self.inputs.instance.is_none()
            || self.inputs.archive.is_none()
            || self.inputs.lock.is_none()
            || !self.data_directory.is_absolute()
            || absolute(&self.data_directory)? != self.data_directory
            || ![
                &self.review_sha256,
                &self.plan_binding_sha256,
                &self.product_sha256,
                &self.native_sha256,
                &self.archive_sha256,
                &self.manifest_sha256,
            ]
            .iter()
            .all(|hash| valid_hash(hash))
        {
            return Err(application_changed());
        }
        for path in [
            &self.inputs.archive,
            &self.inputs.public_key,
            &self.inputs.lock,
            &self.inputs.native,
            &self.inputs.destination,
            &self.inputs.instance,
        ]
        .into_iter()
        .flatten()
        {
            if !path.is_absolute() || absolute(path)? != *path {
                return Err(application_changed());
            }
        }
        Ok(())
    }

    /// Reopening does not take a second writer lock. The caller retains the
    /// original preparation lock; publication rechecks detect same-UID changes.
    pub(crate) fn recheck(&self, supplied_files: bool) -> Result<()> {
        self.validate()?;
        let preparation = Preparation::load(WorkflowStore::inspect(&self.preparation)?)?;
        let current = if supplied_files {
            preparation.application_source()?
        } else {
            preparation.application_binding()?
        };
        if &current != self {
            return Err(application_changed());
        }
        Ok(())
    }

    pub(crate) fn check_location(&self, candidate: &Path) -> Result<()> {
        let preparation = Preparation::load(WorkflowStore::inspect(&self.preparation)?)?;
        if preparation.application_binding()? != *self {
            return Err(application_changed());
        }
        preparation.check_catalog_location(candidate)
    }
}

impl Preparation {
    /// The initial apply must still verify all original supplied files.
    pub(crate) fn application_source(&self) -> Result<ApplicationSource> {
        self.check()?;
        self.application_binding()
    }

    /// Match an existing operation to the current approved local revision.
    /// Original archive/key/lock bytes are not reopened here: an owned completed
    /// registration authenticates its own retained supply material instead.
    pub(crate) fn application_binding(&self) -> Result<ApplicationSource> {
        self.recheck()?;
        self.validate()?;
        let review = self.journal.review.as_ref().ok_or_else(incomplete)?;
        let product = self.product_path(review.revision);
        let native = self.native_path(review.revision);
        if digest(&self.draft.config_bytes()?) != review.product_sha256 {
            return Err(application_changed());
        }
        for (path, expected) in [
            (&product, &review.product_sha256),
            (&native, &review.native_sha256),
        ] {
            let observation = review
                .observations
                .iter()
                .find(|value| value.path == *path)
                .ok_or_else(application_changed)?;
            observation.recheck().map_err(|_| application_changed())?;
            if digest(&read_document(path)?) != *expected {
                return Err(application_changed());
            }
        }
        let inputs = self.inputs();
        let result = ApplicationSource {
            preparation: self.path().to_owned(),
            identity: self.storage.identity()?,
            revision: review.revision,
            review_sha256: self
                .journal
                .review_sha256
                .clone()
                .ok_or_else(application_changed)?,
            plan_binding_sha256: digest(
                &serde_json::to_vec(&(&review.prepared, &review.trust_pin))
                    .map_err(|_| application_changed())?,
            ),
            product,
            native: native.clone(),
            product_sha256: review.product_sha256.clone(),
            native_sha256: review.native_sha256.clone(),
            archive_sha256: review.archive_sha256.clone(),
            manifest_sha256: review.manifest_sha256.clone(),
            data_directory: review.prepared.data_directory.clone(),
            inputs: workflow::Inputs {
                archive: inputs.archive.clone(),
                public_key: inputs.public_key.clone(),
                unsigned: inputs.unsigned,
                lock: inputs.lock.clone(),
                native: Some(native),
                destination: inputs.destination.clone(),
                instance: inputs.instance.clone(),
            },
        };
        result.validate()?;
        self.recheck()?;
        Ok(result)
    }

    pub(crate) fn inspect_application_binding(workspace: &Path) -> Result<ApplicationSource> {
        Self::load(WorkflowStore::inspect(&absolute(workspace)?)?)?.application_binding()
    }
}

fn application_changed() -> BxdlError {
    error(
        "SETUP_APPLICATION_SOURCE_CHANGED",
        "설치 실행에 연결한 준비 기록이 변경되었습니다. 기존 실행 기록과 출력은 보존합니다.",
    )
}
