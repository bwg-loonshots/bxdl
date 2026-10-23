//! Advisory navigation records. Catalog contents never authorize a node action.
use super::{
    home::{self, Home},
    target::{self, Snapshot},
};
use crate::{
    artifact::decode_strict_json,
    error::{BxdlError, Result},
    setup::{paths, store::Store},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, path::Path};
use time::{Date, Month, OffsetDateTime, Time, format_description::well_known::Rfc3339};

const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 262_144;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub id: String,
    pub label: String,
    pub target: Snapshot,
    pub recorded_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    kind: String,
    entries: Vec<Entry>,
}
impl Record {
    fn empty() -> Self {
        Self {
            schema_version: 1,
            kind: "BXDL_CONSOLE_CATALOG".into(),
            entries: Vec::new(),
        }
    }
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.kind != "BXDL_CONSOLE_CATALOG" {
            return Err(corrupt());
        }
        if self.entries.len() > MAX_ENTRIES {
            return Err(home::limit());
        }
        let mut ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for entry in &self.entries {
            if !valid_id(&entry.id)
                || !valid_label(&entry.label)
                || !valid_time(&entry.recorded_at)
                || !ids.insert(&entry.id)
                || !targets.insert((entry.target.identity.device, entry.target.identity.inode))
                || !paths.insert(&entry.target.path)
            {
                return Err(corrupt());
            }
            entry.target.validate().map_err(|_| corrupt())?;
        }
        Ok(())
    }
}

pub(crate) struct Catalog {
    home: Home,
    store: Store,
    record: Record,
}
impl Catalog {
    /// Absence is an empty view, never a request to create or repair storage.
    pub(crate) fn open(path: &Path) -> Result<Option<Self>> {
        let Some(home) = Home::open(path)? else {
            return Ok(None);
        };
        let store = Store::open(&home.catalog_path()).map_err(home::store_error)?;
        let raw = store
            .read()
            .map_err(home::store_error)?
            .ok_or_else(corrupt)?;
        let record = decode(&raw)?;
        home.recheck()?;
        Ok(Some(Self {
            home,
            store,
            record,
        }))
    }
    /// Invoke only after the user explicitly approves management-root creation.
    pub(crate) fn create(path: &Path) -> Result<Self> {
        let record = Record::empty();
        let raw = encode(&record)?;
        let home = Home::create(path, &raw)?;
        let store = Store::open(&home.catalog_path()).map_err(home::store_error)?;
        // A second console may save once the complete home marker is visible.
        // Match the returned in-memory projection to this Store's snapshot.
        let current = store
            .read()
            .map_err(home::store_error)?
            .ok_or_else(corrupt)?;
        let record = decode(&current)?;
        home.recheck()?;
        Ok(Self {
            home,
            store,
            record,
        })
    }
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.record.entries
    }

    pub(crate) fn add(&mut self, label: &str, target: Snapshot) -> Result<String> {
        if !valid_label(label) {
            return Err(invalid_label());
        }
        check_target_location(self.home.path(), &target)?;
        let target = target::recheck(&target)?;
        let _guard = self.home.lock()?;
        self.check_current()?;
        if self.record.entries.len() >= MAX_ENTRIES {
            return Err(home::limit());
        }
        if self
            .record
            .entries
            .iter()
            .any(|entry| same_target(&entry.target, &target))
        {
            return Err(error(
                "CONSOLE_TARGET_EXISTS",
                "이 대상은 이미 목록에 연결되어 있습니다. 기존 항목을 선택하세요.",
            ));
        }
        let id = unique_id(&self.record.entries)?;
        let mut record = self.record.clone();
        record.entries.push(Entry {
            id: id.clone(),
            label: label.to_owned(),
            target,
            recorded_at: now()?,
        });
        self.save(record)?;
        Ok(id)
    }

    /// Hiding removes this navigation entry only. Its target is never opened.
    pub(crate) fn remove(&mut self, id: &str) -> Result<()> {
        let _guard = self.home.lock()?;
        self.check_current()?;
        let mut record = self.record.clone();
        let position = record
            .entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(missing)?;
        record.entries.remove(position);
        self.save(record)
    }

    pub(crate) fn refresh(&mut self, id: &str, target: Snapshot) -> Result<()> {
        let entry = self
            .record
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(missing)?;
        if entry.target.path != target.path
            || entry.target.identity != target.identity
            || entry.target.kind != target.kind
            || entry.target.schema_version != target.schema_version
        {
            return Err(error(
                "CONSOLE_TARGET_CHANGED",
                "연결한 대상의 식별 정보가 바뀌었습니다. 목록을 자동으로 새 대상으로 바꾸지 않았습니다.",
            ));
        }
        check_target_location(self.home.path(), &target)?;
        let target = target::recheck(&target)?;
        let _guard = self.home.lock()?;
        self.check_current()?;
        let mut record = self.record.clone();
        let entry = record
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(missing)?;
        entry.target = target;
        entry.recorded_at = now()?;
        self.save(record)
    }

    fn check_current(&self) -> Result<()> {
        self.home.recheck()?;
        let raw = self
            .store
            .read()
            .map_err(home::store_error)?
            .ok_or_else(corrupt)?;
        if raw != encode(&self.record)? {
            return Err(corrupt());
        }
        Ok(())
    }
    fn save(&mut self, record: Record) -> Result<()> {
        let raw = encode(&record)?;
        self.home.recheck()?;
        self.store.save(&raw).map_err(home::store_error)?;
        self.home.recheck()?;
        self.record = record;
        Ok(())
    }
}

/// Call before creating a management root as well as before catalog writes.
/// This prevents an explicit connection from placing console data in a node.
pub(crate) fn check_target_location(home: &Path, target: &Snapshot) -> Result<()> {
    target.validate()?;
    if paths::overlaps(home, &target.path)? {
        // The console may later create targets directly in these namespaces.
        // A marker-valid existing home is required; arbitrary descendants and
        // the inverse relationship (home inside a target) remain forbidden.
        let namespace = match target.kind {
            target::Kind::Instance => "instances",
            _ => "sessions",
        };
        let expected_parent = home.join(namespace);
        if target.path.parent() != Some(expected_parent.as_path()) || Home::open(home)?.is_none() {
            return Err(error(
                "CONSOLE_TARGET_OVERLAP",
                "관리 위치 안에서는 지정된 노드·세션 폴더만 연결할 수 있습니다. 기존 대상 내부에는 관리 위치를 만들 수 없습니다.",
            ));
        }
    }
    target::check_catalog_writes(target, home)
}

fn same_target(a: &Snapshot, b: &Snapshot) -> bool {
    a.path == b.path || a.identity == b.identity
}
fn encode(record: &Record) -> Result<Vec<u8>> {
    record.validate()?;
    let raw = serde_json::to_vec(record).map_err(|_| corrupt())?;
    if raw.len() > MAX_BYTES {
        return Err(home::limit());
    }
    Ok(raw)
}
fn decode(raw: &[u8]) -> Result<Record> {
    if raw.len() > MAX_BYTES {
        return Err(home::limit());
    }
    let record: Record = serde_json::from_value(decode_strict_json(raw).map_err(|_| corrupt())?)
        .map_err(|_| corrupt())?;
    record.validate()?;
    Ok(record)
}
fn valid_label(label: &str) -> bool {
    !label.trim().is_empty()
        && label == label.trim()
        && label.len() <= 320
        && label.chars().count() <= 80
        && !label.chars().any(char::is_control)
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn unique_id(entries: &[Entry]) -> Result<String> {
    let mut source = std::fs::File::open("/dev/urandom").map_err(|_| corrupt())?;
    for _ in 0..4 {
        let mut bytes = [0u8; 16];
        source.read_exact(&mut bytes).map_err(|_| corrupt())?;
        let id = hex::encode(bytes);
        if entries.iter().all(|entry| entry.id != id) {
            return Ok(id);
        }
    }
    Err(corrupt())
}
fn now() -> Result<String> {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .map_err(|_| corrupt())?
        .format(&Rfc3339)
        .map_err(|_| corrupt())
}
fn valid_time(text: &str) -> bool {
    if text.len() != 20 || !text.is_ascii() {
        return false;
    }
    let bytes = text.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return false;
    }
    let number = |start: usize, end: usize| text[start..end].parse::<u8>().ok();
    let Some(year) = text[..4].parse::<i32>().ok() else {
        return false;
    };
    let Some(month) = number(5, 7).and_then(|value| Month::try_from(value).ok()) else {
        return false;
    };
    let Some(day) = number(8, 10) else {
        return false;
    };
    let (Some(hour), Some(minute), Some(second)) = (number(11, 13), number(14, 16), number(17, 19))
    else {
        return false;
    };
    Date::from_calendar_date(year, month, day).is_ok()
        && Time::from_hms(hour, minute, second).is_ok()
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn corrupt() -> BxdlError {
    error(
        "CONSOLE_CATALOG_CORRUPT",
        "저장된 목록을 검증할 수 없습니다. 목록을 초기화하지 않았습니다. 기존 CLI로 명시 경로를 열 수 있습니다.",
    )
}
fn missing() -> BxdlError {
    error(
        "CONSOLE_ENTRY_MISSING",
        "선택한 목록 항목이 없습니다. 목록을 다시 여세요.",
    )
}
fn invalid_label() -> BxdlError {
    error(
        "CONSOLE_LABEL_INVALID",
        "표시 이름은 앞뒤 공백이나 제어문자 없이 1~80자로 입력하세요.",
    )
}

#[cfg(test)]
mod tests;
