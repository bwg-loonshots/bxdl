//! Persisted preparation of one installation. This module verifies supplied
//! materials and writes private, immutable configuration snapshots only. It
//! never extracts a package, registers a node, opens a DB or starts an engine.
mod application;
use super::{Session, absolute, error, paths, store, workflow_store::WorkflowStore};
use crate::{
    artifact,
    config::network::{self, NodeChoice, RuntimeOptions},
    engine::setup_plan,
    error::{BxdlError, Result},
};
pub(crate) use application::ApplicationSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inputs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsigned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_options: Option<RuntimeOptions>,
    #[serde(default)]
    pub runtime_answers: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    kind: String,
    input_base: PathBuf,
    inputs: Inputs,
    revision: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<Review>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_sha256: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Review {
    revision: u32,
    inputs: Inputs,
    product_sha256: String,
    native_sha256: String,
    archive_sha256: String,
    manifest_sha256: String,
    prepared: setup_plan::Prepared,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_pin: Option<setup_plan::Pin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_native_pin: Option<setup_plan::Pin>,
    network_pins: Vec<network::InputPin>,
    observations: Vec<Observation>,
    cli_match: CliMatch,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum CliMatch {
    Match,
    Mismatch,
}
impl CliMatch {
    fn text(self) -> &'static str {
        match self {
            Self::Match => "MATCH",
            Self::Mismatch => "MISMATCH",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    path: PathBuf,
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    owner: u32,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Observation {
    fn read(path: &Path) -> Result<Self> {
        paths::same(path, path).map_err(|_| changed())?;
        let metadata = fs::symlink_metadata(path).map_err(|_| changed())?;
        if !metadata.is_file() || metadata.mode() & 0o022 != 0 {
            return Err(changed());
        }
        Ok(Self::of(path, &metadata))
    }
    fn of(path: &Path, m: &Metadata) -> Self {
        Self {
            path: path.into(),
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            mode: m.mode(),
            owner: m.uid(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
    fn recheck(&self) -> Result<()> {
        if Self::read(&self.path)? == *self {
            Ok(())
        } else {
            Err(changed())
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub mode: &'static str,
    pub workspace: PathBuf,
    pub draft_complete: bool,
    pub materials_complete: bool,
    pub plan_revision: u32,
    pub preparation: &'static str,
    pub installation: &'static str,
    pub engine_validation: &'static str,
    pub runtime_readiness: &'static str,
    pub cli_match: &'static str,
    pub development_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_options: Option<RuntimeOptions>,
}

/// Public projection of saved preparation records. Credential values, reference
/// paths and hashes are deliberately absent. Reading does not reserve a session.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub summary: Summary,
    pub fields: Vec<PublicField>,
    pub missing_fields: Vec<String>,
    pub missing_inputs: Vec<String>,
    pub next_input: Option<String>,
    pub node_selection: &'static str,
    pub verification: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicField {
    pub name: &'static str,
    pub label: &'static str,
    pub value: String,
}

pub struct Preparation {
    storage: WorkflowStore,
    draft: Session,
    journal: Journal,
}
impl Preparation {
    /// The console calls this before creating its own management namespaces.
    /// This only validates references; it does not reserve or create anything.
    pub fn validate_new(workspace: &Path, from: Option<&Path>) -> Result<()> {
        let workspace = absolute(workspace)?;
        let base = absolute(&std::env::current_dir().map_err(|_| invalid())?)?;
        super::initial_draft(&workspace, from, &base)?;
        if let Some(from) = from {
            if paths::overlaps(&workspace, &absolute(from)?)? {
                return Err(overlap());
            }
        }
        Ok(())
    }

    pub fn create(workspace: &Path, from: Option<&Path>) -> Result<Self> {
        Self::validate_new(workspace, from)?;
        let workspace = absolute(workspace)?;
        let input_base = absolute(&std::env::current_dir().map_err(|_| invalid())?)?;
        let draft = super::initial_draft(&workspace, from, &input_base)?;
        let journal = Journal {
            schema_version: 1,
            kind: "BXDL_INSTALL_PREPARATION".into(),
            input_base,
            inputs: Inputs::default(),
            revision: 0,
            review: None,
            review_sha256: None,
        };
        let storage = WorkflowStore::create(&workspace, &encode(&journal)?, &draft)?;
        Self::load(storage)
    }
    pub fn resume(workspace: &Path) -> Result<Self> {
        Self::load(WorkflowStore::open(&absolute(workspace)?)?)
    }
    /// Read-only snapshot. `revalidate` checks the saved supply/configuration
    /// fingerprints only; it never runs an engine or observes a service.
    pub fn inspect(workspace: &Path, revalidate: bool) -> Result<Inspection> {
        let session = Self::load(WorkflowStore::inspect(&absolute(workspace)?)?)?;
        let missing_fields: Vec<String> = super::FIELDS
            .iter()
            .filter(|field| !session.draft.has_answer(field.name))
            .map(|field| field.name.to_owned())
            .collect();
        let mut missing_inputs = session.missing_inputs();
        let node_selection = if session.inputs().network.is_none() {
            "NOT_APPLICABLE"
        } else if ["nodeId", "p2pAddress", "p2pPort"]
            .iter()
            .any(|name| !session.draft.has_answer(name))
        {
            "MISSING"
        } else if !revalidate {
            "NOT_CHECKED"
        } else {
            match session.selected_node_matches() {
                Ok(true) => "MATCH",
                Ok(false) => "MISMATCH",
                Err(_) => "UNVERIFIED",
            }
        };
        if matches!(node_selection, "MISSING" | "MISMATCH" | "UNVERIFIED") {
            missing_inputs.push("node".into());
        }
        // Match the sequential wizard's dependency order. List uses saved
        // presence only; show additionally checks the selected public node.
        let mut order = vec!["archive", "trustPolicy", "lock", "networkSource"];
        if session.inputs().network.is_some() {
            order.extend(["chainDescription", "node"]);
        }
        order.extend(
            super::FIELDS
                .iter()
                .filter(|field| {
                    session.inputs().network.is_none()
                        || !["chainDescription", "nodeId", "p2pAddress", "p2pPort"]
                            .contains(&field.name)
                })
                .map(|field| field.name),
        );
        if session.inputs().network.is_some() {
            order.extend(RUNTIME_FIELDS);
        }
        order.extend(["destination", "instance"]);
        let next_input = order
            .into_iter()
            .find(|name| {
                missing_inputs
                    .iter()
                    .chain(&missing_fields)
                    .any(|missing| missing == name)
            })
            .map(str::to_owned);
        let (verification, verification_code) = if session.journal.review.is_none() {
            ("NOT_PREPARED", None)
        } else if !revalidate {
            ("NOT_CHECKED", None)
        } else {
            match session.check() {
                Ok(()) => ("VERIFIED", None),
                Err(error) => ("CHANGED", Some(error.code)),
            }
        };
        let fields = super::FIELDS
            .iter()
            .zip(session.review_rows())
            .map(|(field, (label, value))| PublicField {
                name: field.name,
                label,
                value,
            })
            .collect();
        session.storage.recheck()?;
        session.draft.store.read()?;
        Ok(Inspection {
            summary: session.summary(),
            fields,
            missing_fields,
            missing_inputs,
            next_input,
            node_selection,
            verification,
            verification_code,
        })
    }

    /// Structural missing inputs, in preparation order. This is not a validity
    /// claim for referenced files and does not read their contents.
    pub fn missing_inputs(&self) -> Vec<String> {
        let inputs = self.inputs();
        let mut missing = Vec::new();
        if inputs.archive.is_none() {
            missing.push("archive".into());
        }
        if inputs.unsigned.is_none()
            || (inputs.unsigned == Some(false) && inputs.public_key.is_none())
        {
            missing.push("trustPolicy".into());
        }
        if inputs.lock.is_none() {
            missing.push("lock".into());
        }
        if inputs.network.is_none() && inputs.native.is_none() {
            missing.push("networkSource".into());
        }
        if inputs.network.is_some() && inputs.runtime_options.is_none() {
            missing.extend(
                RUNTIME_FIELDS
                    .iter()
                    .filter(|name| !inputs.runtime_answers.contains_key(**name))
                    .map(|name| (*name).to_owned()),
            );
        }
        if inputs.destination.is_none() {
            missing.push("destination".into());
        }
        if inputs.instance.is_none() {
            missing.push("instance".into());
        }
        missing
    }

    /// Detect publication or root replacement after an interactive prompt.
    pub(crate) fn recheck(&self) -> Result<()> {
        self.storage.recheck()?;
        self.draft.store.read()?;
        Ok(())
    }

    fn load(storage: WorkflowStore) -> Result<Self> {
        let journal: Journal =
            artifact::decode_strict_json(&storage.read()?).map_err(|_| invalid())?;
        let draft = Session::resume(storage.draft_path())?;
        let result = Self {
            storage,
            draft,
            journal,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn path(&self) -> &Path {
        self.storage.path()
    }
    pub fn draft(&self) -> &Session {
        &self.draft
    }
    pub fn review_rows(&self) -> Vec<(&'static str, String)> {
        self.draft.review_rows()
    }
    /// Explicit-detail projection: operational/public locations only, never
    /// credential paths or private hashes.
    pub(crate) fn public_locations(&self) -> Result<Vec<(&'static str, PathBuf)>> {
        self.storage.recheck()?;
        [
            ("데이터 예정", "dataDirectory"),
            ("공통 chain", "chainDescription"),
        ]
        .into_iter()
        .filter_map(|(label, field)| {
            self.draft
                .draft
                .answers
                .get(field)
                .map(|value| (label, value))
        })
        .map(|(label, value)| Ok((label, self.resolve(value)?)))
        .collect()
    }
    pub fn inputs(&self) -> &Inputs {
        &self.journal.inputs
    }
    pub fn resolve(&self, value: &str) -> Result<PathBuf> {
        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(invalid());
        }
        let path = Path::new(value);
        absolute(&if path.is_absolute() {
            path.to_owned()
        } else {
            self.journal.input_base.join(path)
        })
    }
    pub fn summary(&self) -> Summary {
        Summary {
            mode: "PREPARATION",
            workspace: self.path().into(),
            draft_complete: self.draft.complete(),
            materials_complete: complete_inputs(self.inputs()).is_ok(),
            plan_revision: self.journal.revision,
            preparation: if self.journal.review.is_some() {
                "PREPARED_RECORDED"
            } else {
                "DRAFT"
            },
            installation: "NOT_PERFORMED",
            engine_validation: "NOT_CHECKED",
            runtime_readiness: "NOT_CHECKED",
            cli_match: self
                .journal
                .review
                .as_ref()
                .map_or("NOT_CHECKED", |r| r.cli_match.text()),
            development_only: true,
            runtime_options: self.inputs().runtime_options.clone(),
        }
    }
    pub fn set_inputs(&mut self, inputs: Inputs) -> Result<()> {
        self.check_layout(&self.draft.draft, &inputs)?;
        if self.inputs() == &inputs {
            return Ok(());
        }
        let mut journal = self.journal.clone();
        journal.inputs = inputs;
        journal.review = None;
        self.save(journal)
    }
    /// Each accepted policy answer is durable. Defaults remain proposals until
    /// answered; a new draft gains runtime options only after all five answers.
    pub fn set_runtime_answer(&mut self, index: usize, value: &str) -> Result<()> {
        let field = RUNTIME_FIELDS.get(index).ok_or_else(invalid)?;
        let value = canonical_runtime_answer(index, value)?;
        let mut inputs = self.inputs().clone();
        if inputs.native.is_some() {
            return Err(invalid());
        }
        if inputs.runtime_answers.is_empty() {
            if let Some(options) = &inputs.runtime_options {
                inputs.runtime_answers = runtime_answers(options);
            }
        }
        inputs.runtime_answers.insert((*field).into(), value);
        inputs.runtime_options = options_from_answers(&inputs.runtime_answers)?;
        self.set_inputs(inputs)
    }

    pub(crate) fn field_matches(&self, name: &str, value: &str) -> bool {
        self.draft.answer_matches(name, value)
    }
    pub(crate) fn validate_field(&self, name: &str, value: &str) -> Result<()> {
        self.draft.validate_answer(name, value)?;
        let mut next = self.draft.draft.clone();
        next.answers.insert(name.into(), value.into());
        self.check_layout(&next, self.inputs())
    }
    pub fn set_field(&mut self, name: &str, value: &str) -> Result<()> {
        self.validate_field(name, value)?;
        if self.draft.answer_matches(name, value) {
            return Ok(());
        }
        self.invalidate()?;
        self.storage.recheck()?;
        self.draft.set(name, value)?;
        self.storage.recheck()
    }
    fn invalidate(&mut self) -> Result<()> {
        if self.journal.review.is_some() {
            let mut journal = self.journal.clone();
            journal.review = None;
            self.save(journal)?;
        }
        Ok(())
    }
    fn save(&mut self, mut journal: Journal) -> Result<()> {
        journal.review_sha256 = journal
            .review
            .as_ref()
            .map(|review| encode(review).map(|raw| digest(&raw)))
            .transpose()?;
        // Keep memory on the last published journal when CAS/fsync fails.
        self.storage.save(&encode(&journal)?)?;
        self.journal = journal;
        Ok(())
    }
    pub fn node_choices(&self) -> Result<Vec<NodeChoice>> {
        self.storage.recheck()?;
        self.check_layout(&self.draft.draft, self.inputs())?;
        let chain = self
            .draft
            .draft
            .answers
            .get("chainDescription")
            .ok_or_else(incomplete)?;
        let chain_path = self.resolve(chain)?;
        for field in [
            "dataDirectory",
            "validatorKeystore",
            "validatorPasswordFile",
            "tlsKeyStore",
            "tlsKeyPasswordFile",
            "tlsTrustStore",
            "tlsTrustPasswordFile",
        ] {
            if let Some(reference) = self.draft.draft.answers.get(field) {
                if paths::overlaps(&chain_path, &self.resolve(reference)?)? {
                    return Err(overlap());
                }
            }
        }
        let choices = network::node_choices(&chain_path, required(&self.inputs().network)?)
            .map_err(|_| materials_invalid())?;
        self.storage.recheck()?;
        Ok(choices)
    }
    /// Match the saved public node triple to the current selected network.
    /// Reads public materials only; it does not attest engine/key identity.
    pub fn selected_node_matches(&self) -> Result<bool> {
        let answers = &self.draft.draft.answers;
        Ok(self.node_choices()?.iter().any(|node| {
            answers.get("nodeId") == Some(&node.node_id)
                && answers.get("p2pAddress") == Some(&node.host)
                && answers.get("p2pPort") == Some(&node.port.to_string())
        }))
    }

    /// The three related fields are validated and persisted together, so a
    /// change never leaves an accidental node-ID/endpoint mixture on resume.
    pub fn select_node(&mut self, node_id: &str) -> Result<()> {
        let choice = self
            .node_choices()?
            .into_iter()
            .find(|c| c.node_id == node_id)
            .ok_or_else(materials_invalid)?;
        let mut next = self.draft.draft.clone();
        for (field, value) in [
            ("nodeId", choice.node_id),
            ("p2pAddress", choice.host),
            ("p2pPort", choice.port.to_string()),
        ] {
            next.answers.insert(field.into(), value);
        }
        self.check_layout(&next, self.inputs())?;
        if next.answers == self.draft.draft.answers {
            return Ok(());
        }
        self.invalidate()?;
        self.storage.recheck()?;
        let previous = self.draft.draft.clone();
        self.draft.draft = next;
        if let Err(failure) = self.draft.save() {
            self.draft.draft = previous;
            return Err(failure);
        }
        self.storage.recheck()
    }
    fn product_path(&self, revision: u32) -> PathBuf {
        self.storage
            .generated_path()
            .join(format!("instance-{revision:04}.json"))
    }
    fn native_path(&self, revision: u32) -> PathBuf {
        self.storage
            .generated_path()
            .join(format!("node-{revision:04}.json"))
    }

    /// Call only after the operator approves preparing a snapshot. Publication
    /// is append-only; a failed attempt consumes its revision and is never
    /// silently resumed as an accepted plan or overwritten on another attempt.
    pub fn prepare(&mut self) -> Result<Summary> {
        self.storage.recheck()?;
        self.validate()?;
        complete_inputs(self.inputs())?;
        self.check_layout(&self.draft.draft, self.inputs())?;
        let product = self.draft.config_bytes()?;
        let document: Value = artifact::decode_strict_json(&product).map_err(|_| invalid())?;
        let data = Path::new(
            document["storage"]["dataDirectory"]
                .as_str()
                .ok_or_else(invalid)?,
        );
        for output in [
            required(&self.inputs().destination)?,
            required(&self.inputs().instance)?,
            data,
        ] {
            require_absent(output)?;
        }
        if self.draft.preflight()?.outcome == "FAIL" {
            return Err(materials_invalid());
        }
        let initial_observations = input_paths(self.inputs())
            .map(Observation::read)
            .collect::<Result<Vec<_>>>()?;
        let package = artifact::verify(
            required(&self.inputs().archive)?,
            &verify_options(self.inputs()),
        )
        .map_err(|_| materials_invalid())?;
        for observation in &initial_observations {
            observation.recheck()?;
        }
        let cli_match = if setup_plan::check_cli(&package).is_ok() {
            CliMatch::Match
        } else {
            CliMatch::Mismatch
        };
        let revision = self
            .journal
            .revision
            .checked_add(1)
            .filter(|r| *r <= 1000)
            .ok_or_else(invalid)?;
        let product_path = self.product_path(revision);
        let native_path = self.native_path(revision);
        // Validate supplied native references before the first generated write.
        let original_native = if let Some(native) = &self.inputs().native {
            let bound = setup_plan::prepare(
                &product,
                &product_path,
                native,
                required(&self.inputs().lock)?,
                &package,
            )
            .map_err(|_| materials_invalid())?;
            self.check_bound(&bound, self.inputs(), None)?;
            Some((
                normalize_native(native)?,
                setup_plan::pin(native).map_err(|_| materials_invalid())?,
            ))
        } else {
            None
        };
        let mut next = self.journal.clone();
        next.revision = revision;
        next.review = None;
        self.storage.require_capacity(2)?;
        self.save(next)?;
        self.storage.recheck()?;
        store::write_new(&product_path, &product)?;
        self.storage.recheck()?;
        let mut network_pins = Vec::new();
        let generated = if original_native.is_none() {
            let prepared = network::load(
                &product_path,
                required(&self.inputs().network)?,
                self.inputs()
                    .runtime_options
                    .clone()
                    .ok_or_else(incomplete)?,
            )
            .map_err(|_| materials_invalid())?;
            // All referenced external paths were checked against the container;
            // this check additionally covers the complete network projection.
            for reference in &prepared.referenced_paths {
                if reference != &product_path && paths::overlaps(self.path(), reference)? {
                    return Err(overlap());
                }
            }
            prepared.recheck().map_err(|_| changed())?;
            network_pins = prepared.input_pins.clone();
            Some(prepared)
        } else {
            None
        };
        let native = original_native
            .as_ref()
            .map(|(raw, _)| raw.as_slice())
            .or_else(|| generated.as_ref().map(|p| p.native_bytes.as_slice()))
            .ok_or_else(invalid)?;
        store::write_new(&native_path, native)?;
        self.storage.recheck()?;
        let prepared = setup_plan::prepare(
            &product,
            &product_path,
            &native_path,
            required(&self.inputs().lock)?,
            &package,
        )
        .map_err(|_| materials_invalid())?;
        self.check_bound(&prepared, self.inputs(), Some(&native_path))?;
        let trust_pin = self
            .inputs()
            .public_key
            .as_deref()
            .map(setup_plan::pin)
            .transpose()
            .map_err(|_| materials_invalid())?;
        if let Some(generated) = &generated {
            generated.recheck().map_err(|_| changed())?;
        }
        for observation in &initial_observations {
            observation.recheck()?;
        }
        let mut pin_paths = prepared
            .pins
            .iter()
            .map(|pin| pin.path.clone())
            .chain(input_paths(self.inputs()).map(Path::to_owned))
            .chain([product_path, native_path])
            .collect::<BTreeSet<_>>();
        for pin in &network_pins {
            pin_paths.insert(pin.path.clone());
        }
        let observations = pin_paths
            .iter()
            .map(|path| Observation::read(path))
            .collect::<Result<Vec<_>>>()?;
        setup_plan::recheck(&prepared.pins).map_err(|_| changed())?;
        let review = Review {
            revision,
            inputs: self.inputs().clone(),
            product_sha256: digest(&product),
            native_sha256: digest(native),
            archive_sha256: package.archive_sha256,
            manifest_sha256: package.manifest_sha256,
            prepared,
            trust_pin,
            original_native_pin: original_native.map(|(_, pin)| pin),
            network_pins,
            observations,
            cli_match,
        };
        let mut next = self.journal.clone();
        next.review = Some(review);
        self.save(next)?;
        self.check()?;
        Ok(self.summary())
    }

    /// Recheck a recorded preparation without writing or observing a runtime.
    pub fn check(&self) -> Result<()> {
        self.storage.recheck()?;
        self.validate()?;
        let review = self.journal.review.as_ref().ok_or_else(incomplete)?;
        for observation in &review.observations {
            observation.recheck()?;
        }
        if digest(&self.draft.config_bytes()?) != review.product_sha256 {
            return Err(changed());
        }
        self.check_layout(&self.draft.draft, self.inputs())?;
        self.check_bound(
            &review.prepared,
            self.inputs(),
            Some(&self.native_path(review.revision)),
        )?;
        if self.draft.preflight()?.outcome == "FAIL" {
            return Err(changed());
        }
        if !review.network_pins.is_empty() {
            network::verify_pins(&review.network_pins).map_err(|_| changed())?;
        }
        setup_plan::recheck(&review.prepared.pins).map_err(|_| changed())?;
        for pin in review.trust_pin.iter().chain(&review.original_native_pin) {
            setup_plan::recheck(std::slice::from_ref(pin)).map_err(|_| changed())?;
        }
        for (path, hash) in [
            (self.product_path(review.revision), &review.product_sha256),
            (self.native_path(review.revision), &review.native_sha256),
        ] {
            if digest(&read_document(&path)?) != *hash {
                return Err(changed());
            }
        }
        let package = artifact::verify(
            required(&self.inputs().archive)?,
            &verify_options(self.inputs()),
        )
        .map_err(|_| changed())?;
        if package.archive_sha256 != review.archive_sha256
            || package.manifest_sha256 != review.manifest_sha256
        {
            return Err(changed());
        }
        for observation in &review.observations {
            observation.recheck()?;
        }
        self.storage.recheck()
    }

    /// Keep private path checks behind this adapter for the console catalog.
    pub(crate) fn check_catalog_location(&self, candidate: &Path) -> Result<()> {
        self.storage.recheck()?;
        super::check_draft(&self.draft.draft, candidate)?;
        for reference in input_paths(self.inputs())
            .chain(self.inputs().destination.as_deref())
            .chain(self.inputs().instance.as_deref())
        {
            if paths::overlaps(candidate, reference)? {
                return Err(overlap());
            }
        }
        if let Some(review) = &self.journal.review {
            for pin in &review.prepared.pins {
                if paths::overlaps(candidate, &pin.path)? {
                    return Err(overlap());
                }
            }
        }
        self.storage.recheck()
    }
    fn validate(&self) -> Result<()> {
        let j = &self.journal;
        if j.schema_version != 1
            || j.kind != "BXDL_INSTALL_PREPARATION"
            || j.revision > 1000
            || !j.input_base.is_absolute()
            || absolute(&j.input_base)? != j.input_base
        {
            return Err(invalid());
        }
        if j.review
            .as_ref()
            .map(|review| encode(review).map(|raw| digest(&raw)))
            .transpose()?
            != j.review_sha256
        {
            return Err(invalid());
        }
        validate_inputs(&j.inputs)?;
        super::check_draft(&self.draft.draft, self.path())?;
        if let Some(review) = &j.review {
            if review.revision == 0
                || review.revision != j.revision
                || review.inputs != j.inputs
                || ![
                    &review.product_sha256,
                    &review.native_sha256,
                    &review.archive_sha256,
                    &review.manifest_sha256,
                ]
                .into_iter()
                .all(|h| valid_hash(h))
                || review.observations.is_empty()
                || review.observations.len() > 140
                || review.network_pins.len() != if j.inputs.network.is_some() { 3 } else { 0 }
                || review.original_native_pin.is_some() != j.inputs.native.is_some()
                || review.trust_pin.is_some() != j.inputs.public_key.is_some()
            {
                return Err(invalid());
            }
            complete_inputs(&review.inputs)?;
            let mut seen = BTreeSet::new();
            for observation in &review.observations {
                if !observation.path.is_absolute()
                    || absolute(&observation.path)? != observation.path
                    || !seen.insert(&observation.path)
                {
                    return Err(invalid());
                }
            }
            let mut required_paths = input_paths(&j.inputs)
                .map(Path::to_owned)
                .chain(review.prepared.pins.iter().map(|p| p.path.clone()))
                .chain([self.product_path(j.revision), self.native_path(j.revision)])
                .collect::<BTreeSet<_>>();
            for pin in &review.network_pins {
                required_paths.insert(pin.path.clone());
            }
            if required_paths != seen.into_iter().cloned().collect() {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn check_layout(&self, draft: &super::Draft, inputs: &Inputs) -> Result<()> {
        validate_inputs(inputs)?;
        super::check_draft(draft, self.path())?;
        let sources = input_paths(inputs).collect::<Vec<_>>();
        let outputs = std::iter::once(self.path())
            .chain(inputs.destination.as_deref())
            .chain(inputs.instance.as_deref())
            .collect::<Vec<_>>();
        for (index, output) in outputs.iter().enumerate() {
            super::check_draft(draft, output)?;
            for other in outputs.iter().skip(index + 1) {
                if paths::overlaps(output, other)? {
                    return Err(overlap());
                }
            }
            for source in &sources {
                if paths::overlaps(output, source)? {
                    return Err(overlap());
                }
            }
        }
        // A file offered as public material must not alias data or credentials
        // already named in the partial draft, even before its contents are read.
        for (index, source) in sources.iter().enumerate() {
            super::check_draft(draft, source)?;
            for other in sources.iter().skip(index + 1) {
                if paths::overlaps(source, other)? {
                    return Err(overlap());
                }
            }
        }
        Ok(())
    }
    fn check_bound(
        &self,
        prepared: &setup_plan::Prepared,
        inputs: &Inputs,
        generated_native: Option<&Path>,
    ) -> Result<()> {
        let outputs = [
            self.path(),
            required(&inputs.destination)?,
            required(&inputs.instance)?,
            prepared.data_directory.as_path(),
        ];
        for (index, output) in outputs.iter().enumerate() {
            for other in outputs.iter().skip(index + 1) {
                if paths::overlaps(output, other)? {
                    return Err(overlap());
                }
            }
            for source in input_paths(inputs)
                .chain(prepared.pins.iter().map(|p| p.path.as_path()))
                .filter(|source| Some(*source) != generated_native)
            {
                if paths::overlaps(output, source)? {
                    return Err(overlap());
                }
            }
        }
        for output in outputs.iter().skip(1) {
            require_absent(output)?;
        }
        Ok(())
    }
}

fn require_absent(output: &Path) -> Result<()> {
    match fs::symlink_metadata(output) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(error(
            "SETUP_PREPARATION_DESTINATION_EXISTS",
            "새 설치에는 새 설치·인스턴스·데이터 경로를 지정하세요. 기존 경로는 변경하지 않았습니다.",
        )),
    }
}

pub const RUNTIME_FIELDS: [&str; 5] = [
    "keyStoreType",
    "trustStoreType",
    "mode",
    "consensusAutoStart",
    "archiveEnabled",
];
fn canonical_runtime_answer(index: usize, value: &str) -> Result<String> {
    let answer = match index {
        0 | 1 => match value {
            "PKCS12" | "pkcs12" => "PKCS12",
            "JKS" | "jks" => "JKS",
            _ => return Err(invalid()),
        },
        2 => match value {
            "new-network" => "new-network",
            "join-network" => "join-network",
            _ => return Err(invalid()),
        },
        3 | 4 => match value {
            "yes" | "y" => "yes",
            "no" | "n" => "no",
            _ => return Err(invalid()),
        },
        _ => return Err(invalid()),
    };
    Ok(answer.into())
}
fn runtime_answers(options: &RuntimeOptions) -> BTreeMap<String, String> {
    use network::{NetworkMode, StoreType};
    let store = |kind| match kind {
        StoreType::Pkcs12 => "PKCS12",
        StoreType::Jks => "JKS",
    };
    let values = [
        store(options.key_store_type),
        store(options.trust_store_type),
        match options.mode {
            NetworkMode::NewNetwork => "new-network",
            NetworkMode::JoinNetwork => "join-network",
        },
        if options.consensus_auto_start {
            "yes"
        } else {
            "no"
        },
        if options.archive_enabled { "yes" } else { "no" },
    ];
    RUNTIME_FIELDS
        .into_iter()
        .zip(values)
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
}
fn options_from_answers(answers: &BTreeMap<String, String>) -> Result<Option<RuntimeOptions>> {
    if RUNTIME_FIELDS.iter().any(|key| !answers.contains_key(*key)) {
        return Ok(None);
    }
    use network::{NetworkMode, StoreType};
    let get = |index: usize| canonical_runtime_answer(index, &answers[RUNTIME_FIELDS[index]]);
    let store = |value: String| {
        if value == "PKCS12" {
            StoreType::Pkcs12
        } else {
            StoreType::Jks
        }
    };
    Ok(Some(RuntimeOptions {
        key_store_type: store(get(0)?),
        trust_store_type: store(get(1)?),
        mode: if get(2)? == "new-network" {
            NetworkMode::NewNetwork
        } else {
            NetworkMode::JoinNetwork
        },
        consensus_auto_start: get(3)? == "yes",
        archive_enabled: get(4)? == "yes",
    }))
}

fn validate_inputs(inputs: &Inputs) -> Result<()> {
    if inputs.unsigned == Some(true) && inputs.public_key.is_some()
        || inputs.network.is_some() && inputs.native.is_some()
        || inputs.native.is_some() && inputs.runtime_options.is_some()
    {
        return Err(invalid());
    }
    for path in input_paths(inputs)
        .chain(inputs.destination.as_deref())
        .chain(inputs.instance.as_deref())
    {
        if !path.is_absolute() || absolute(path)? != path {
            return Err(invalid());
        }
        paths::same(path, path)?;
    }
    for (field, value) in &inputs.runtime_answers {
        let index = RUNTIME_FIELDS
            .iter()
            .position(|key| key == field)
            .ok_or_else(invalid)?;
        if canonical_runtime_answer(index, value)? != *value {
            return Err(invalid());
        }
    }
    if let Some(options) = &inputs.runtime_options {
        options.validate()?;
        if !inputs.runtime_answers.is_empty()
            && options_from_answers(&inputs.runtime_answers)?.as_ref() != Some(options)
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn input_paths(inputs: &Inputs) -> impl Iterator<Item = &Path> {
    [
        &inputs.archive,
        &inputs.public_key,
        &inputs.lock,
        &inputs.network,
        &inputs.native,
    ]
    .into_iter()
    .filter_map(|p| p.as_deref())
}
fn complete_inputs(inputs: &Inputs) -> Result<()> {
    validate_inputs(inputs)?;
    if inputs.archive.is_none()
        || inputs.lock.is_none()
        || inputs.destination.is_none()
        || inputs.instance.is_none()
        || !matches!(
            (inputs.unsigned, &inputs.public_key),
            (Some(true), None) | (Some(false), Some(_))
        )
        || !matches!(
            (&inputs.network, &inputs.native, &inputs.runtime_options),
            (Some(_), None, Some(_)) | (None, Some(_), None)
        )
    {
        return Err(incomplete());
    }
    Ok(())
}
fn verify_options(inputs: &Inputs) -> artifact::VerifyOptions {
    artifact::VerifyOptions {
        public_key_path: inputs.public_key.clone(),
        allow_unsigned_development: inputs.unsigned == Some(true),
    }
}
fn required(path: &Option<PathBuf>) -> Result<&Path> {
    path.as_deref().ok_or_else(incomplete)
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| invalid())
}
fn digest(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn read_document(path: &Path) -> Result<Vec<u8>> {
    let observed = Observation::read(path)?;
    if observed.size == 0 || observed.size > 262_144 {
        return Err(materials_invalid());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC)
                .bits() as i32,
        )
        .open(path)
        .map_err(|_| materials_invalid())?;
    if Observation::of(path, &file.metadata().map_err(|_| changed())?) != observed {
        return Err(changed());
    }
    let mut raw = Vec::new();
    (&mut file)
        .take(262_145)
        .read_to_end(&mut raw)
        .map_err(|_| materials_invalid())?;
    if raw.len() as u64 != observed.size
        || Observation::of(path, &file.metadata().map_err(|_| changed())?) != observed
    {
        return Err(changed());
    }
    observed.recheck()?;
    Ok(raw)
}
fn normalize_native(path: &Path) -> Result<Vec<u8>> {
    let mut value: Value =
        artifact::decode_strict_json(&read_document(path)?).map_err(|_| materials_invalid())?;
    let base = path.parent().ok_or_else(materials_invalid)?;
    let normalize = |value: &mut Value| -> Result<()> {
        let source = Path::new(value.as_str().ok_or_else(materials_invalid)?);
        let resolved = absolute(&if source.is_absolute() {
            source.to_owned()
        } else {
            base.join(source)
        })?;
        *value = Value::String(resolved.to_str().ok_or_else(materials_invalid)?.into());
        Ok(())
    };
    normalize(&mut value["chainFile"])?;
    normalize(&mut value["dataDirectory"])?;
    for (key, value) in value["node"]
        .as_object_mut()
        .ok_or_else(materials_invalid)?
    {
        if key.ends_with("-path") || key.ends_with("-file") {
            normalize(value)?;
        }
    }
    let mut raw = serde_json::to_vec_pretty(&value).map_err(|_| materials_invalid())?;
    raw.push(b'\n');
    if raw.len() > 262_144 {
        return Err(materials_invalid());
    }
    Ok(raw)
}
fn invalid() -> BxdlError {
    error(
        "SETUP_PREPARATION_INVALID",
        "준비 기록이나 입력 형식이 올바르지 않습니다. 기존 기록을 보존하고 입력을 확인하세요.",
    )
}
fn incomplete() -> BxdlError {
    error(
        "SETUP_PREPARATION_INCOMPLETE",
        "패키지·신뢰 정책·lock·제품 설정과 네트워크 또는 native 자료의 입력을 완료하세요.",
    )
}
fn materials_invalid() -> BxdlError {
    error(
        "SETUP_PREPARATION_MATERIAL_INVALID",
        "준비 자료의 파일·권한·서명·설정 일치를 확인하지 못했습니다. 선택한 자료와 참조 파일을 확인하세요.",
    )
}
fn changed() -> BxdlError {
    error(
        "SETUP_PREPARATION_INPUT_CHANGED",
        "준비한 자료 또는 파일 identity가 변경되었습니다. 저장한 답변을 확인하고 준비 검토를 다시 승인하세요.",
    )
}
fn overlap() -> BxdlError {
    error(
        "SETUP_PREPARATION_PATH_CONFLICT",
        "준비 세션·설치·인스턴스·데이터와 입력 자료의 경로는 서로 분리해야 합니다.",
    )
}

#[cfg(test)]
mod tests;
