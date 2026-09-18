//! Explicit development-candidate inspection and cold validation only.
//! Never installs, initializes, starts a node, or infers production readiness.
mod files;
mod json;
mod process;
mod product;

pub use product::{ProductReport, preflight_product};

use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Options {
    pub jar: PathBuf,
    pub java: PathBuf,
    pub lock: PathBuf,
    pub timeout: Duration,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub build_info_status: String,
    pub engine: String,
    pub version: String,
    pub source: Source,
    pub java: Java,
    pub console: Console,
    pub contract: Contract,
    pub distribution: Distribution,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub commit: String,
    pub dirty: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Java {
    pub required_major: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Console {
    pub node: String,
    pub npm: String,
    pub source_fingerprint: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contract {
    pub status: String,
    pub revision: String,
    pub documentation_sha256: String,
    pub definition_sha256: String,
    pub runtime_documentation_sha256: String,
    pub runtime_fixture_sha256: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Distribution {
    pub channel: String,
    pub official_release: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lock {
    schema_version: u32,
    jar_sha256: String,
    jar_size_bytes: u64,
    java_sha256: String,
    expected: Identity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub outcome: &'static str,
    pub engine_exit_code: i32,
    pub development_only: bool,
    pub identity: Identity,
    pub jar_sha256: String,
    pub java_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_file_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preflight: Option<ColdResult>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColdResult {
    pub command: String,
    pub status: String,
    pub reason: String,
    pub contract_status: String,
    pub backend: String,
    pub chain_fingerprint: String,
    pub node_identity: String,
    pub checks: Vec<Check>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub check: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    status: String,
    reason: String,
}

pub fn inspect(options: &Options) -> Result<Report> {
    execute(options, None, None)
}
pub fn preflight(options: &Options, native_config: &Path) -> Result<Report> {
    execute(options, Some(native_config), None)
}

fn execute(
    options: &Options,
    config: Option<&Path>,
    product: Option<&product::ProductInput>,
) -> Result<Report> {
    if options.timeout.is_zero()
        || options.timeout > Duration::from_secs(120)
        || !options.java.is_absolute()
    {
        return Err(fail(
            "ENGINE_OPTIONS_INVALID",
            "실행 시간과 Java 절대 경로를 확인하세요.",
        ));
    }
    let lock_bytes = files::Input::read(&options.lock, 65_536, false)?;
    let lock: Lock = json::decode(&lock_bytes.raw).map_err(|_| {
        fail(
            "ENGINE_LOCK_INVALID",
            "개발 엔진 lock 형식이 잘못되었습니다.",
        )
    })?;
    validate_lock(&lock)?;
    let native = config.map(files::NativeInput::load).transpose()?;
    if let Some(product) = product {
        product.check(native.as_ref().ok_or_else(response_invalid)?)?;
    }
    let workspace = files::Workspace::create(native.as_ref())?;
    let jar = workspace.snapshot_jar(&options.jar, &lock.jar_sha256, lock.jar_size_bytes)?;
    let java = files::Binary::open(&options.java, &lock.java_sha256)?;
    if let Some(product) = product {
        product.recheck()?;
    }
    let raw = process::run(
        &java,
        &jar,
        &workspace,
        "engine-info",
        None,
        options.timeout,
    )?;
    if raw.exit != 0 {
        return Err(response_invalid());
    }
    let identity: Identity = json::decode(&raw.stdout).map_err(|_| response_invalid())?;
    if identity != lock.expected {
        return Err(fail(
            "ENGINE_IDENTITY_MISMATCH",
            "엔진의 실제 build identity가 lock과 일치하지 않습니다.",
        ));
    }
    let mut report = Report {
        outcome: "INSPECTED_DEVELOPMENT",
        engine_exit_code: 0,
        development_only: true,
        identity,
        jar_sha256: lock.jar_sha256,
        java_sha256: lock.java_sha256,
        config_sha256: None,
        chain_file_sha256: None,
        preflight: None,
    };
    if let Some(native) = native {
        let config = native.snapshot(&workspace)?;
        native.recheck()?;
        if let Some(product) = product {
            product.recheck()?;
        }
        let output = process::run(
            &java,
            &jar,
            &workspace,
            "preflight",
            Some(&config),
            options.timeout,
        )?;
        native.recheck()?;
        let cold = consume_cold(output.exit, &output.stdout)?;
        if let Some(product) = product {
            product.check(&native)?;
            product.check_result(&cold)?;
        }
        report.outcome = "INCOMPLETE";
        report.engine_exit_code = 3;
        report.config_sha256 = Some(files::digest(&native.config.raw));
        report.chain_file_sha256 = Some(files::digest(&native.chain.raw));
        report.preflight = Some(cold);
    }
    lock_bytes.recheck()?;
    if let Some(product) = product {
        product.recheck()?;
    }
    Ok(report)
}

fn validate_lock(lock: &Lock) -> Result<()> {
    let info = &lock.expected;
    if lock.schema_version != 1
        || !hash(&lock.jar_sha256)
        || !hash(&lock.java_sha256)
        || lock.jar_size_bytes == 0
        || lock.jar_size_bytes > files::MAX_JAR
        || info.build_info_status != "AVAILABLE"
        || info.engine != "NIGO"
        || !token(&info.version)
        || !commit(&info.source.commit)
        || info.java.required_major != 21
        || !token(&info.console.node)
        || !token(&info.console.npm)
        || !hash(&info.console.source_fingerprint)
        || info.contract.status != "PROPOSED"
        || info.contract.revision != info.source.commit
        || [
            &info.contract.documentation_sha256,
            &info.contract.definition_sha256,
            &info.contract.runtime_documentation_sha256,
            &info.contract.runtime_fixture_sha256,
            &info.contract.fingerprint,
        ]
        .iter()
        .any(|s| !hash(s))
        || info.distribution.channel != "development"
        || info.distribution.official_release
    {
        return Err(fail(
            "ENGINE_LOCK_INVALID",
            "개발 후보 identity와 고정 hash가 있는 lock이 필요합니다.",
        ));
    }
    Ok(())
}

fn consume_cold(exit: i32, raw: &[u8]) -> Result<ColdResult> {
    if matches!(exit, 64 | 74) {
        let error: Failure = json::decode(raw).map_err(|_| response_invalid())?;
        return match (exit, error.status.as_str(), error.reason.as_str()) {
            (64, "INVALID_CONFIGURATION", "INVALID_ARGUMENTS_OR_CONFIGURATION") => Err(fail(
                "ENGINE_INVALID_CONFIGURATION",
                "NIGO가 native 설정을 거부했습니다. 설정 계약을 확인하세요.",
            )),
            (74, "FAILED", "PRECONDITION_OR_IO_FAILURE") => Err(fail(
                "ENGINE_PRECONDITION_FAILED",
                "NIGO의 입력 자료 읽기 또는 사전 조건 검사가 실패했습니다.",
            )),
            _ => Err(response_invalid()),
        };
    }
    if exit != 3 {
        return Err(response_invalid());
    }
    let result: ColdResult = json::decode(raw).map_err(|_| response_invalid())?;
    if result.command != "preflight"
        || result.status != "INCOMPLETE"
        || result.reason != "RUNTIME_CHECKS_REQUIRED"
        || result.contract_status != "PROPOSED"
        || !matches!(result.backend.as_str(), "h2" | "rocksdb")
        || !hash(&result.chain_fingerprint)
        || !node_identity(&result.node_identity)
        || result.checks.len() != 5
    {
        return Err(response_invalid());
    }
    let expected = [
        ("CONFIGURATION", "PASS", None),
        (
            "KEY_MATERIAL",
            if result.node_identity == "INSTANT" {
                "NOT_APPLICABLE"
            } else {
                "PASS"
            },
            None,
        ),
        (
            "DATABASE_AND_WAL",
            "NOT_CHECKED",
            Some("REQUIRES_EXCLUSIVE_OPEN"),
        ),
        (
            "PORTS_AND_PEERS",
            "NOT_CHECKED",
            Some("NETWORK_NOT_ACCESSED"),
        ),
        (
            "NATIVE_RUNTIME",
            "NOT_CHECKED",
            Some("REQUIRES_RUNTIME_LOAD"),
        ),
    ];
    for (name, status, reason) in expected {
        let matches: Vec<_> = result.checks.iter().filter(|c| c.check == name).collect();
        if matches.len() != 1
            || matches[0].status != status
            || matches[0].reason.as_deref() != reason
        {
            return Err(response_invalid());
        }
    }
    Ok(result)
}

fn node_identity(value: &str) -> bool {
    if value == "INSTANT" {
        return true;
    }
    let Some((node, validator)) = value.split_once(':') else {
        return false;
    };
    fn hexadecimal(s: &str, length: usize) -> bool {
        let Some(raw) = s.strip_prefix("0x") else {
            return false;
        };
        raw.len() == length
            && raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
    hexadecimal(node, 64) && (validator == "OBSERVER" || hexadecimal(validator, 40))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+_-".contains(&b))
}
fn response_invalid() -> BxdlError {
    fail(
        "ENGINE_RESPONSE_INVALID",
        "엔진 출력과 종료 코드가 고정된 개발 계약에 맞지 않습니다.",
    )
}
fn fail(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}

#[cfg(test)]
mod tests;
