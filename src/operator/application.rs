//! Approved named installation. Preparation, execution and navigation records
//! have separate lifecycles; a catalog failure never undoes a registration.
use super::{
    catalog::{self, Catalog},
    home::Home,
    prompt::{self, Confirmation, Policy},
    target,
};
use crate::{
    error::{BxdlError, Result},
    setup::{
        self,
        preparation::Preparation,
        workflow::{ApplicationSummary, Workflow},
    },
};
use serde::Serialize;
use std::{
    io::{BufRead, Write},
    path::Path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub record: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ApplicationSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub name_binding: &'static str,
    pub runtime: &'static str,
}
impl View {
    pub(crate) fn partial(&self) -> bool {
        self.record == "UNAVAILABLE"
            || self.summary.as_ref().is_some_and(|s| {
                s.source_match != "MATCH"
                    || s.verification == "FAILED"
                    || (s.registration_confirmed && self.name_binding != "CONNECTED")
                    || [&s.installation, &s.registration]
                        .iter()
                        .any(|s| matches!(s.as_str(), "UNKNOWN" | "IN_PROGRESS"))
            })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Summary {
    pub schema_version: u32,
    pub name: String,
    pub preparation_revision: u32,
    pub application: Option<View>,
    pub next_command: Vec<String>,
    pub fallback_command: Vec<String>,
}
pub(crate) struct Finish {
    pub outcome: &'static str,
    pub code: String,
    pub message: &'static str,
    pub exit: i32,
    pub summary: Summary,
}

fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(BxdlError::new(
            "SETUP_APPLICATION_UNAVAILABLE",
            "설치 실행 기록을 확인하지 못했습니다. 기존 자료를 보존하세요.",
        )),
    }
}
fn name_binding(home: &Path, name: &str, instance: &Path) -> &'static str {
    match target::resolve_instance(Some(home), Some(name), None) {
        Ok(node) if node.snapshot().path == instance => "CONNECTED",
        Ok(_) => "CONFLICT",
        Err(error) if error.code == "NODE_NAME_MISSING" => "MISSING",
        Err(_) => "UNAVAILABLE",
    }
}

/// No locks, journal saves, reconciliation, cold checks or runtime probes.
pub(crate) fn inspect(home: &Path, name: &str, revalidate: bool) -> Result<Option<View>> {
    let operation = home.join("operations").join(name);
    if !exists(&operation)? {
        return Ok(None);
    }
    let source = Preparation::inspect_application_binding(&home.join("sessions").join(name)).ok();
    let view = match Workflow::inspect_application(&operation, source.as_ref(), revalidate) {
        Ok(summary) => View {
            record: "READABLE",
            name_binding: name_binding(home, name, &summary.instance),
            summary: Some(summary),
            reason_code: None,
            runtime: "NOT_OBSERVED",
        },
        Err(error) => View {
            record: "UNAVAILABLE",
            summary: None,
            reason_code: Some(error.code),
            name_binding: "UNAVAILABLE",
            runtime: "NOT_OBSERVED",
        },
    };
    Ok(Some(view))
}

fn named_command(action: &str, name: &str, home: &Path) -> Vec<String> {
    let mut words = vec!["bxdl".into(), action.into(), name.into()];
    if action == "init" {
        words.push("--confirm-initialize".into());
    }
    words.extend(["--home".into(), home.to_string_lossy().into_owned()]);
    words
}
pub(crate) fn next_command(home: &Path, name: &str, view: &View) -> Vec<String> {
    if let Some(s) = &view.summary {
        if s.registration_confirmed
            && s.source_match == "MATCH"
            && s.verification == "PASSED"
            && view.name_binding == "MISSING"
        {
            return vec![
                "bxdl".into(),
                "setup".into(),
                "apply".into(),
                name.into(),
                "--home".into(),
                home.to_string_lossy().into_owned(),
            ];
        }
        if s.registration_confirmed
            && s.source_match == "MATCH"
            && s.verification != "FAILED"
            && view.name_binding == "CONNECTED"
        {
            // Initialization may have happened through the explicit engine
            // command. Never infer permission to initialize from a stale journal.
            let action = target::resolve_instance(Some(home), Some(name), None)
                .ok()
                .and_then(|node| node.recheck().ok())
                .filter(|snapshot| snapshot.state == target::RecordState::Registered)
                .map_or("status", |_| "init");
            return named_command(action, name, home);
        }
        if s.registration_confirmed {
            return vec![
                "bxdl".into(),
                "instance".into(),
                "show".into(),
                "--instance".into(),
                s.instance.to_string_lossy().into_owned(),
            ];
        }
    }
    vec![
        "bxdl".into(),
        "setup".into(),
        "show".into(),
        name.into(),
        "--home".into(),
        home.to_string_lossy().into_owned(),
    ]
}

pub(crate) fn run(
    home: &Path,
    name: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    if !catalog::valid_name(name) {
        return Err(BxdlError::new(
            "PREPARATION_NAME_INVALID",
            "유효한 설치 준비 이름을 지정하세요.",
        ));
    }
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(BxdlError::new(
            "INSTALL_PLATFORM_UNSUPPORTED",
            "설치 적용은 macOS arm64에서 지원합니다.",
        ));
    }
    run_supported(home, name, input, out)
}

fn run_supported(
    home: &Path,
    name: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    let home_path = setup::absolute(home)?;
    let home = Home::open(&home_path)?.ok_or_else(|| {
        BxdlError::new(
            "PREPARATION_NOT_FOUND",
            "설치 준비가 없습니다. setup new로 먼저 준비하세요.",
        )
    })?;
    let preparation = Preparation::resume(&home.path().join("sessions").join(name))?;
    let operations = home.path().join("operations");
    let operation = operations.join(name);
    let existing = exists(&operation)?;
    let source = if existing {
        preparation.application_binding()?
    } else {
        preparation.application_source()?
    };
    preparation.check_catalog_location(&operations)?;
    let mut workflow = if existing {
        Some(Workflow::resume_prepared(&operation, &source)?)
    } else {
        None
    };
    let mut catalog = Catalog::open(home.path())?.ok_or_else(|| {
        BxdlError::new("CONSOLE_HOME_INVALID", "관리 목록을 확인하지 못했습니다.")
    })?;
    if let Some(entry) = catalog
        .entries()
        .iter()
        .find(|entry| entry.alias.as_deref() == Some(name))
    {
        let verified = if existing {
            Workflow::inspect_prepared(&operation, &source, true).ok()
        } else {
            None
        };
        if !verified.is_some_and(|s| s.registration_confirmed && s.verification == "PASSED")
            || entry.target.path != source.instance()
            || name_binding(home.path(), name, source.instance()) != "CONNECTED"
        {
            return Err(BxdlError::new(
                "NODE_NAME_EXISTS",
                "이 이름은 기존 노드에 연결되어 있습니다. 설치 준비와 노드 목록을 확인하세요.",
            ));
        }
    }
    // Reserved management records must never become package/control/data
    // outputs. Compare filesystem identities and Mac name equivalence too.
    for (namespace, path) in [
        ("packages", source.destination()),
        ("instances", source.instance()),
        ("data", source.data_directory.as_path()),
    ] {
        if setup::paths::overlaps(home.path(), path)?
            && !path
                .parent()
                .map(|parent| setup::paths::same(parent, &home.path().join(namespace)))
                .transpose()?
                .unwrap_or(false)
        {
            return Err(BxdlError::new(
                "SETUP_MANAGED_OUTPUT_INVALID",
                "관리 홈 안의 출력은 packages/instances/data 각각의 바로 아래에 두세요. 관리 기록 경로는 설치 대상으로 사용할 수 없습니다.",
            ));
        }
    }
    prompt::say(
        out,
        &format!(
            "{name} · 준비 버전 {} 설치 적용\n패키지: {}\ncontrol: {}\n데이터(초기화 시 생성): {}\n패키지 설치·cold 검사·인스턴스 등록·이름 연결을 수행합니다. 초기화와 시작은 별도 명령입니다.\n",
            source.revision,
            prompt::safe_default(&source.destination().to_string_lossy()),
            prompt::safe_default(&source.instance().to_string_lossy()),
            prompt::safe_default(&source.data_directory.to_string_lossy())
        ),
    )?;
    let approval = prompt::confirm(
        input,
        out,
        Policy::Install,
        "위 준비 버전으로 설치를 적용하거나 완료된 결과를 확인하고 이름을 연결할까요?",
    )?;
    let mut summary = Summary {
        schema_version: 1,
        name: name.into(),
        preparation_revision: source.revision,
        application: None,
        next_command: vec![
            "bxdl".into(),
            "setup".into(),
            "apply".into(),
            name.into(),
            "--home".into(),
            home.path().to_string_lossy().into_owned(),
        ],
        fallback_command: vec![
            "bxdl".into(),
            "instance".into(),
            "show".into(),
            "--instance".into(),
            source.instance().to_string_lossy().into_owned(),
        ],
    };
    if approval != Confirmation::Yes {
        return Ok(Finish {
            outcome: "INCOMPLETE",
            code: "SETUP_APPLICATION_PAUSED".into(),
            message: "설치 적용을 승인하지 않았습니다. 기존 자료를 보존했습니다.",
            exit: 5,
            summary,
        });
    }
    home.recheck()?;
    preparation.recheck()?;
    catalog.check_current()?;
    source.recheck(!existing)?;
    for (namespace, path) in [
        ("packages", source.destination()),
        ("instances", source.instance()),
        ("data", source.data_directory.as_path()),
    ] {
        if path.parent() == Some(home.path().join(namespace).as_path()) {
            home.ensure_namespace(namespace)?;
        }
    }
    home.ensure_namespace("operations")?;
    if workflow.is_none() {
        workflow = Some(Workflow::create_prepared(&operation, &source)?);
    }
    let workflow = workflow.as_mut().expect("created or resumed application");
    let applied = workflow.apply_prepared(&source, || {
        home.recheck()?;
        preparation.recheck()?;
        catalog.check_current()
    });
    let state = match applied {
        Ok(state) => state,
        Err(error) => {
            summary.application = match inspect(home.path(), name, false) {
                Ok(view) => view,
                Err(failure) => Some(View {
                    record: "UNAVAILABLE",
                    summary: None,
                    reason_code: Some(failure.code),
                    name_binding: "UNAVAILABLE",
                    runtime: "NOT_OBSERVED",
                }),
            };
            let unknown = error.code.contains("UNKNOWN")
                || error.code == "ENGINE_TIMEOUT"
                || summary
                    .application
                    .as_ref()
                    .and_then(|v| v.summary.as_ref())
                    .is_none_or(|s| {
                        [&s.installation, &s.registration]
                            .iter()
                            .any(|phase| matches!(phase.as_str(), "UNKNOWN" | "IN_PROGRESS"))
                    });
            summary.next_command = vec![
                "bxdl".into(),
                "setup".into(),
                "show".into(),
                name.into(),
                "--home".into(),
                home.path().to_string_lossy().into_owned(),
            ];
            return Ok(Finish {
                outcome: if unknown { "UNKNOWN" } else { "INCOMPLETE" },
                code: error.code,
                message: "설치 적용을 완료하지 못했습니다. 실행 기록과 기존 출력은 보존했습니다. setup show로 확인하세요.",
                exit: if unknown { 6 } else { 5 },
                summary,
            });
        }
    };
    let bind = (|| {
        home.recheck()?;
        preparation.recheck()?;
        let target = target::resolve_instance(None, None, Some(source.instance()))?;
        catalog.add_named(name, target.snapshot().clone())
    })();
    let binding_code = bind.err().map(|error| error.code);
    let connected = binding_code.is_none();
    let view = View {
        record: "READABLE",
        summary: Some(state),
        reason_code: binding_code,
        name_binding: if connected { "CONNECTED" } else { "FAILED" },
        runtime: "NOT_OBSERVED",
    };
    summary.next_command = if connected {
        next_command(home.path(), name, &view)
    } else {
        vec![
            "bxdl".into(),
            "nodes".into(),
            "add".into(),
            name.into(),
            "--instance".into(),
            source.instance().to_string_lossy().into_owned(),
            "--home".into(),
            home.path().to_string_lossy().into_owned(),
        ]
    };
    summary.application = Some(view);
    Ok(Finish {
        outcome: if connected { "SUCCEEDED" } else { "INCOMPLETE" },
        code: if connected {
            "SETUP_APPLIED"
        } else {
            "SETUP_NAME_BINDING_PENDING"
        }
        .into(),
        message: if connected {
            "설치·검사·등록·이름 연결을 확인했습니다. 노드 실행 상태는 status로 확인하세요."
        } else {
            "설치·등록은 확인했지만 이름 저장을 완료하지 못했습니다. 기존 control 경로로 운용할 수 있습니다."
        },
        exit: if connected { 0 } else { 5 },
        summary,
    })
}
