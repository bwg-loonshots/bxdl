//! Direct, noninteractive navigation. This module never starts an engine or
//! observes live service status. Catalog names remain advisory.
use super::{
    catalog::{self, Catalog},
    target::{self, Kind, RecordState},
};
use crate::error::{BxdlError, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Node {
    id: String,
    name: Option<String>,
    label: String,
    instance_id: String,
    recorded_state: RecordState,
    recorded_at: String,
    current_status: &'static str,
}
impl From<&catalog::Entry> for Node {
    fn from(entry: &catalog::Entry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.alias.clone(),
            label: entry.label.clone(),
            instance_id: entry.target.name.clone(),
            recorded_state: entry.target.state,
            recorded_at: entry.recorded_at.clone(),
            current_status: "NOT_OBSERVED",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listing {
    entries: Vec<Node>,
    current_status: &'static str,
}

pub(crate) fn list(home: &Path) -> Result<Listing> {
    let mut entries = Catalog::open(home)?
        .map(|catalog| {
            catalog
                .entries()
                .iter()
                .filter(|entry| entry.target.kind == Kind::Instance)
                .map(Node::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| (&a.name, &a.label, &a.id).cmp(&(&b.name, &b.label, &b.id)));
    Ok(Listing {
        entries,
        current_status: "NOT_OBSERVED",
    })
}

pub(crate) fn add(home: &Path, name: &str, instance: &Path) -> Result<Node> {
    // Validate even when called outside the argument parser, before any writes.
    if !catalog::valid_name(name) {
        return Err(BxdlError::new(
            "NODE_NAME_INVALID",
            "이름은 소문자로 시작하는 1~32자의 소문자·숫자·하이픈으로 지정하세요.",
        ));
    }
    let resolved = target::resolve_instance(None, None, Some(instance))?;
    let snapshot = resolved.snapshot().clone();
    catalog::check_target_location(home, &snapshot)?;
    resolved.recheck()?;
    // The explicit `nodes add` command authorizes only the management record.
    // Validation is complete before preparing even a missing management parent.
    let mut catalog = match Catalog::open(home)? {
        Some(catalog) => catalog,
        None => Catalog::create(home)?,
    };
    let id = catalog.add_named(name, snapshot)?;
    // add_named pins and checks binding identity; do not report a subsequent
    // filesystem failure as though the catalog save had never taken place.
    let entry = catalog
        .entries()
        .iter()
        .find(|entry| entry.id == id)
        .expect("successful catalog add retains its entry");
    Ok(Node::from(entry))
}
