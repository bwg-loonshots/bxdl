//! Resumable product configuration drafts. This is not an installed instance,
//! an engine initialization journal, or a NIGO validation contract.
pub(crate) mod paths;
mod store;
mod wizard;

pub use wizard::{Finish, interact};

use crate::{
    config,
    error::{BxdlError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};
use store::Store;

pub struct Field {
    pub name: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub default: Option<&'static str>,
}

pub const FIELDS: &[Field] = &[
    Field {
        name: "instanceId",
        label: "인스턴스 이름",
        hint: "영문 소문자로 시작하는 1~32자의 소문자·숫자·하이픈",
        default: Some("validator-one"),
    },
    Field {
        name: "nodeId",
        label: "공개 노드 ID",
        hint: "운영자가 준비한 공개 식별자. 엔진의 실제 identity 검사는 이후 수행",
        default: None,
    },
    Field {
        name: "dataDirectory",
        label: "데이터 저장 위치",
        hint: "DB를 새로 만들거나 열지 않습니다. setup 작업 폴더와 분리하세요",
        default: None,
    },
    Field {
        name: "httpAddress",
        label: "로컬 콘솔 IP",
        hint: "127.0.0.1 또는 ::1",
        default: Some("127.0.0.1"),
    },
    Field {
        name: "httpPort",
        label: "로컬 콘솔 포트",
        hint: "1~65535, P2P 포트와 달라야 합니다",
        default: Some("18080"),
    },
    Field {
        name: "p2pAddress",
        label: "P2P IP",
        hint: "명시적인 unicast IP. Mac 로컬 시험은 127.0.0.1 사용 가능",
        default: None,
    },
    Field {
        name: "p2pPort",
        label: "P2P 포트",
        hint: "1~65535, 로컬 콘솔 포트와 달라야 합니다",
        default: Some("19090"),
    },
    Field {
        name: "chainDescription",
        label: "공통 체인 자료 파일",
        hint: "파일 경로만 입력하세요. 새 네트워크를 생성하지 않습니다",
        default: None,
    },
    Field {
        name: "validatorKeystore",
        label: "Validator keystore 파일",
        hint: "기존 파일 경로만 입력",
        default: None,
    },
    Field {
        name: "validatorPasswordFile",
        label: "Validator 비밀번호 파일",
        hint: "비밀번호 원문 대신 password-file 경로만 입력",
        default: None,
    },
    Field {
        name: "tlsKeyStore",
        label: "TLS keystore 파일",
        hint: "기존 파일 경로만 입력",
        default: None,
    },
    Field {
        name: "tlsKeyPasswordFile",
        label: "TLS keystore 비밀번호 파일",
        hint: "비밀번호 원문 대신 password-file 경로만 입력",
        default: None,
    },
    Field {
        name: "tlsTrustStore",
        label: "TLS truststore 파일",
        hint: "기존 파일 경로만 입력",
        default: None,
    },
    Field {
        name: "tlsTrustPasswordFile",
        label: "TLS truststore 비밀번호 파일",
        hint: "비밀번호 원문 대신 password-file 경로만 입력",
        default: None,
    },
];

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Draft {
    schema_version: u32,
    kind: String,
    input_base: String,
    #[serde(deserialize_with = "read_answers")]
    answers: BTreeMap<String, String>,
}

// A map is convenient for a partial form, but serde's default map decoder
// silently accepts duplicate keys. Reject them before any checkpoint is used.
fn read_answers<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique setup field names and string values")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if !FIELDS.iter().any(|field| field.name == key)
                    || result.insert(key, value).is_some()
                {
                    return Err(serde::de::Error::custom("invalid setup field"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Visitor)
}

pub struct Session {
    store: Store,
    draft: Draft,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub workspace: String,
    pub completed_fields: Vec<String>,
    pub next_field: Option<String>,
    pub total_fields: usize,
    pub draft_complete: bool,
    pub installation: &'static str,
    pub engine_validation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preflight: Option<config::Report>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
}

impl Session {
    pub fn create(path: &Path, from: Option<&Path>) -> Result<Self> {
        // Validate imports before creating the setup workspace. Relative input
        // references are bound to their original config, not the new workspace.
        let imported = from.map(config::normalized_file).transpose()?;
        let input_base = absolute(
            &std::env::current_dir()
                .map_err(|_| error("SETUP_PATH_INVALID", "작업 경로를 확인할 수 없습니다."))?,
        )?;
        let workspace = absolute(path)?;
        let mut draft = Draft {
            schema_version: 1,
            kind: "BXDL_SETUP_DRAFT".into(),
            input_base: text(&input_base)?,
            answers: BTreeMap::new(),
        };
        if let Some(raw) = imported {
            draft.answers = answers_from_config(&raw)?;
        }
        check_draft(&draft, &workspace)?;
        let store = Store::create(&workspace)?;
        let mut session = Self { store, draft };
        session.save()?;
        Ok(session)
    }

    pub fn resume(path: &Path) -> Result<Self> {
        let store = Store::open(path)?;
        let raw = store.read()?.ok_or_else(|| {
            error(
                "SETUP_CHECKPOINT_MISSING",
                "완료된 초안이 없습니다. 다른 새 작업 폴더를 지정하세요.",
            )
        })?;
        let draft: Draft = serde_json::from_slice(&raw).map_err(|_| {
            error(
                "SETUP_DRAFT_INVALID",
                "초안 형식이 잘못되었습니다. 기존 파일은 변경하지 않았습니다.",
            )
        })?;
        check_draft(&draft, store.path())?;
        Ok(Self { store, draft })
    }

    pub fn path(&self) -> &Path {
        self.store.path()
    }
    pub fn next_index(&self) -> usize {
        FIELDS
            .iter()
            .position(|f| !self.draft.answers.contains_key(f.name))
            .unwrap_or(FIELDS.len())
    }
    pub fn has_answer(&self, name: &str) -> bool {
        self.draft.answers.contains_key(name)
    }
    pub fn complete(&self) -> bool {
        self.next_index() == FIELDS.len()
    }
    pub fn default_for(&self, index: usize) -> Option<String> {
        let field = FIELDS.get(index)?;
        if field.name == "dataDirectory" {
            let id = self.draft.answers.get("instanceId")?;
            // Keep an explicit --workspace usable on Linux for tests and future
            // adapters. This default is a proposed path; no data dir is created.
            #[cfg(target_os = "macos")]
            if let Some(home) = std::env::var_os("HOME") {
                let home = PathBuf::from(home);
                if home.is_absolute() {
                    return home
                        .join("Library/Application Support/BXDL/instances")
                        .join(id)
                        .join("data")
                        .to_str()
                        .map(str::to_owned);
                }
            }
            return Path::new(&self.draft.input_base)
                .join("bxdl-instances")
                .join(id)
                .join("data")
                .to_str()
                .map(str::to_owned);
        }
        field.default.map(str::to_owned)
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<()> {
        if !FIELDS.iter().any(|field| field.name == name)
            || value.is_empty()
            || value.len() > 4096
            || value.chars().any(char::is_control)
        {
            return Err(error(
                "SETUP_INPUT_INVALID",
                "입력값은 비어 있지 않은 일반 텍스트여야 합니다.",
            ));
        }
        let old = self.draft.answers.insert(name.into(), value.into());
        if let Err(failure) = check_draft(&self.draft, self.store.path()).and_then(|_| self.save())
        {
            match old {
                Some(value) => {
                    self.draft.answers.insert(name.into(), value);
                }
                None => {
                    self.draft.answers.remove(name);
                }
            }
            return Err(failure);
        }
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        let mut raw = serde_json::to_vec_pretty(&self.draft)
            .map_err(|_| error("SETUP_DRAFT_INVALID", "초안을 구성할 수 없습니다."))?;
        raw.push(b'\n');
        self.store.save(&raw)
    }

    pub fn config_bytes(&self) -> Result<Vec<u8>> {
        self.store.read()?;
        if !self.complete() {
            return Err(error(
                "SETUP_DRAFT_INCOMPLETE",
                "필수 입력이 남아 있습니다. 대화형 setup --resume으로 계속하세요.",
            ));
        }
        config::normalize_bytes(
            &config_bytes(&self.draft.answers, false)?,
            &Path::new(&self.draft.input_base).join(".bxdl-setup-config.json"),
        )
    }

    pub fn preflight(&self) -> Result<config::Report> {
        config::preflight_bytes(
            &self.config_bytes()?,
            &Path::new(&self.draft.input_base).join(".bxdl-setup-config.json"),
        )
    }

    pub fn summary(&self) -> Summary {
        Summary {
            workspace: self.store.path().to_string_lossy().into_owned(),
            completed_fields: FIELDS
                .iter()
                .filter(|f| self.has_answer(f.name))
                .map(|f| f.name.into())
                .collect(),
            next_field: FIELDS.get(self.next_index()).map(|f| f.name.into()),
            total_fields: FIELDS.len(),
            draft_complete: self.complete(),
            installation: "NOT_PERFORMED",
            engine_validation: "NOT_CHECKED",
            preflight: None,
            config_path: None,
        }
    }

    pub fn export(&self, output: &Path) -> Result<PathBuf> {
        let mut output = absolute(output)?;
        let reserved = self.path().join("instance.json");
        if paths::same(&output, &reserved)? {
            // The checkpoint inventory reserves this exact spelling, including
            // when a case-insensitive filesystem aliases the requested path.
            output = reserved;
        }
        // Revalidation uses the destination to reject self-referencing config.
        // References have already been made absolute so relocation is harmless.
        let raw = config::normalize_bytes(&self.config_bytes()?, &output)?;
        let value: Value = serde_json::from_slice(&raw)
            .map_err(|_| error("SETUP_DRAFT_INVALID", "초안을 구성할 수 없습니다."))?;
        let data = Path::new(value["storage"]["dataDirectory"].as_str().unwrap_or(""));
        if paths::overlaps(&output, data)?
            || (paths::overlaps(&output, self.path())?
                && !paths::same(&output, &self.path().join("instance.json"))?)
        {
            return Err(error(
                "SETUP_OUTPUT_CONFLICT",
                "설정 출력은 데이터 위치 또는 초안 기록 경로와 겹칠 수 없습니다.",
            ));
        }
        for reference in std::iter::once(&value["chainDescription"]).chain(
            value["secrets"]
                .as_object()
                .into_iter()
                .flat_map(|object| object.values()),
        ) {
            if paths::overlaps(&output, Path::new(reference.as_str().unwrap_or("")))? {
                return Err(error(
                    "SETUP_OUTPUT_CONFLICT",
                    "설정 출력은 체인·키 자료 경로와 겹칠 수 없습니다.",
                ));
            }
        }
        self.store.read()?;
        store::write_new(&output, &raw)?;
        self.store.read()?;
        Ok(output)
    }
}

fn check_draft(draft: &Draft, workspace: &Path) -> Result<()> {
    let base = Path::new(&draft.input_base);
    if draft.schema_version != 1
        || draft.kind != "BXDL_SETUP_DRAFT"
        || !base.is_absolute()
        || absolute(base)? != base
        || draft.answers.iter().any(|(name, value)| {
            !FIELDS.iter().any(|field| field.name == name)
                || value.is_empty()
                || value.len() > 4096
                || value.chars().any(char::is_control)
        })
    {
        return Err(error(
            "SETUP_DRAFT_INVALID",
            "초안 형식 또는 입력 기준 경로가 잘못되었습니다.",
        ));
    }
    // Product schema is the single source of field rules, even while a form is
    // partial. Placeholder values never leave this local validation operation.
    let raw = config::normalize_bytes(
        &config_bytes(&draft.answers, true)?,
        &base.join(".bxdl-setup-config.json"),
    )?;
    let config: Value = serde_json::from_slice(&raw)
        .map_err(|_| error("SETUP_DRAFT_INVALID", "초안을 해석할 수 없습니다."))?;
    if draft.answers.contains_key("dataDirectory")
        && paths::overlaps(
            workspace,
            Path::new(config["storage"]["dataDirectory"].as_str().unwrap_or("")),
        )?
    {
        return Err(error(
            "SETUP_PATH_CONFLICT",
            "초안 작업 폴더와 데이터 위치는 서로 분리해야 합니다.",
        ));
    }
    for name in [
        "chainDescription",
        "validatorKeystore",
        "validatorPasswordFile",
        "tlsKeyStore",
        "tlsKeyPasswordFile",
        "tlsTrustStore",
        "tlsTrustPasswordFile",
    ] {
        if draft.answers.contains_key(name) {
            let reference = if name == "chainDescription" {
                &config[name]
            } else {
                &config["secrets"][name]
            };
            if paths::overlaps(Path::new(reference.as_str().unwrap_or("")), workspace)? {
                return Err(error(
                    "SETUP_PATH_CONFLICT",
                    "체인·키 자료는 초안 작업 폴더 밖의 기존 경로를 참조하세요.",
                ));
            }
        }
    }
    Ok(())
}

fn config_bytes(answers: &BTreeMap<String, String>, partial: bool) -> Result<Vec<u8>> {
    let value = |name: &str, placeholder: &str| -> Result<String> {
        answers
            .get(name)
            .cloned()
            .or_else(|| partial.then(|| placeholder.into()))
            .ok_or_else(|| error("SETUP_DRAFT_INCOMPLETE", "필수 입력이 남아 있습니다."))
    };
    let http = value("httpPort", "18080")?;
    let p2p = value("p2pPort", if http == "19090" { "19091" } else { "19090" })?;
    let port = |text: &str| {
        text.parse::<u16>().ok().filter(|n| *n > 0).ok_or_else(|| {
            error(
                "PORTS_INVALID",
                "포트는 서로 다른 1~65535 범위의 정수여야 합니다.",
            )
        })
    };
    let mut secrets = serde_json::Map::new();
    for field in &FIELDS[8..] {
        secrets.insert(
            field.name.into(),
            Value::String(value(
                field.name,
                &format!(".bxdl-placeholder/{}", field.name),
            )?),
        );
    }
    serde_json::to_vec(&json!({
        "schemaVersion": 1, "instanceId": value("instanceId", "placeholder")?, "role": "validator",
        "nodeId": value("nodeId", "placeholder")?, "chainDescription": value("chainDescription", ".bxdl-placeholder/chain.json")?,
        "storage": {"backend": "rocksdb", "dataDirectory": value("dataDirectory", ".bxdl-placeholder/data")?},
        "secrets": secrets,
        "http": {"address": value("httpAddress", "127.0.0.1")?, "port": port(&http)?},
        "p2p": {"address": value("p2pAddress", "127.0.0.1")?, "port": port(&p2p)?}
    })).map_err(|_| error("SETUP_DRAFT_INVALID", "설정을 구성할 수 없습니다."))
}

fn answers_from_config(raw: &[u8]) -> Result<BTreeMap<String, String>> {
    let value: Value = serde_json::from_slice(raw)
        .map_err(|_| error("SETUP_DRAFT_INVALID", "설정을 읽을 수 없습니다."))?;
    let mut result = BTreeMap::new();
    for field in FIELDS {
        let v = match field.name {
            "dataDirectory" => &value["storage"]["dataDirectory"],
            "httpAddress" => &value["http"]["address"],
            "httpPort" => &value["http"]["port"],
            "p2pAddress" => &value["p2p"]["address"],
            "p2pPort" => &value["p2p"]["port"],
            "instanceId" | "nodeId" | "chainDescription" => &value[field.name],
            _ => &value["secrets"][field.name],
        };
        result.insert(
            field.name.into(),
            if field.name.ends_with("Port") {
                v.to_string()
            } else {
                v.as_str()
                    .ok_or_else(|| error("SETUP_DRAFT_INVALID", "설정 필드가 잘못되었습니다."))?
                    .into()
            },
        );
    }
    Ok(result)
}

fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| error("SETUP_PATH_INVALID", "경로는 UTF-8 텍스트여야 합니다."))
}

pub fn absolute(path: &Path) -> Result<PathBuf> {
    let s = text(path)?;
    if s.is_empty()
        || s.len() > 4096
        || s.starts_with('~')
        || s.contains('\\')
        || s.chars().any(char::is_control)
    {
        return Err(error(
            "SETUP_PATH_INVALID",
            "경로에는 제어문자나 ~ 축약을 사용할 수 없습니다.",
        ));
    }
    let source = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| error("SETUP_PATH_INVALID", "작업 경로를 확인할 수 없습니다."))?
            .join(path)
    };
    let mut result = PathBuf::new();
    for part in source.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

pub fn default_workspace() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home);
        if path.is_absolute() {
            return absolute(&path.join("Library/Application Support/BXDL/setup"));
        }
    }
    Err(error(
        "SETUP_WORKSPACE_REQUIRED",
        "--workspace로 기존 부모 폴더 아래의 새 작업 폴더를 지정하세요.",
    ))
}

fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}

#[cfg(test)]
mod tests;
