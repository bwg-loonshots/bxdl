//! Sequential Linux preparation and explicit privileged apply. This process
//! retains the user's catalog; sudo receives only the reviewed immutable plan.
use super::{
    catalog::{self, Catalog},
    home::Home,
    prompt::{self, Confirmation, Input, Policy},
};
use crate::{
    engine::linux_service::{install, management},
    error::{BxdlError, Result},
    setup::{self, linux::Preparation},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) struct Finish {
    pub outcome: &'static str,
    pub code: &'static str,
    pub message: &'static str,
    pub data: Value,
    pub exit: i32,
}
pub(crate) struct Options<'a> {
    pub home: &'a Path,
    pub name: &'a str,
    pub resume: bool,
    pub from: Option<&'a Path>,
    pub runtime: Option<&'a Path>,
    pub jar: Option<&'a Path>,
    pub native: Option<&'a Path>,
}
fn invalid() -> BxdlError {
    BxdlError::new(
        "LINUX_SETUP_INVALID",
        "Linux 준비 이름·사용자 관리 위치·입력 자료를 확인하세요.",
    )
}
fn workspace(home: &Path, name: &str) -> Result<PathBuf> {
    if !catalog::valid_name(name) {
        return Err(invalid());
    }
    Ok(setup::absolute(home)?.join("sessions").join(name))
}
const PUBLIC_FIELDS: &[(&str, &str)] = &[
    ("nodeId", "노드 ID"),
    ("httpAddress", "HTTP 주소"),
    ("httpPort", "HTTP 포트"),
    ("p2pAddress", "P2P 주소"),
    ("p2pPort", "P2P 포트"),
];
fn next(home: &Path, verb: &str, name: &str) -> String {
    format!(
        "bxdl {verb} {name} --home '{}'",
        home.to_string_lossy().replace('\'', "'\"'\"'")
    )
}
fn public(s: &Preparation) -> Result<Value> {
    let mut value = serde_json::to_value(s.summary()?).map_err(|_| invalid())?;
    value["fields"] = json!(
        PUBLIC_FIELDS
            .iter()
            .map(|(name, label)| json!({"name":name,"label":label,"value":s.field_value(name)}))
            .collect::<Vec<_>>()
    );
    Ok(value)
}
fn saved(s: &Preparation, paused: bool) -> Result<Finish> {
    Ok(Finish {
        outcome: if paused { "INCOMPLETE" } else { "SUCCEEDED" },
        code: if paused {
            "LINUX_SETUP_PAUSED"
        } else {
            "LINUX_SETUP_SAVED"
        },
        message: if paused {
            "준비 내용을 보존했습니다. setup resume으로 이어가세요."
        } else {
            "Linux 설치 준비를 저장했습니다. setup apply로 별도 승인하세요."
        },
        data: public(s)?,
        exit: if paused { 5 } else { 0 },
    })
}
fn paused() -> Finish {
    Finish {
        outcome: "INCOMPLETE",
        code: "LINUX_SETUP_NOT_CREATED",
        message: "준비를 만들지 않았습니다.",
        data: json!({"platform":"linux-amd64","installation":"NOT_STARTED"}),
        exit: 5,
    }
}
fn home(path: &Path, create: bool) -> Result<Option<Home>> {
    let existing = Home::open(path)?;
    if existing.is_some() || !create {
        return Ok(existing);
    }
    Catalog::create(path)?;
    Home::open(path)
}
fn check_name(home: &Path, name: &str, control: &Path) -> Result<()> {
    if let Some(catalog) = Catalog::open(home)? {
        if catalog
            .entries()
            .iter()
            .any(|e| e.alias.as_deref() == Some(name) && e.target.path != control)
        {
            return Err(BxdlError::new(
                "NODE_NAME_CONFLICT",
                "다른 노드에 연결된 이름입니다. 기존 연결을 유지했으니 다른 준비 이름을 사용하세요.",
            ));
        }
    }
    Ok(())
}
fn say_review(s: &Preparation, out: &mut dyn Write) -> Result<()> {
    let summary = s.summary()?;
    prompt::say(
        out,
        &format!(
            "\nLinux 설치 검토 · {}\n시스템 노드: {}\n데이터: {}\n전용 계정: {} (관리자 승인 뒤 준비)\n",
            summary.name,
            summary.control.display(),
            summary.data_directory.display(),
            summary.service_account
        ),
    )?;
    for &(field, label) in PUBLIC_FIELDS {
        if let Some(value) = s.field_value(field) {
            prompt::say(out, &format!("{label}: {}\n", prompt::safe_default(&value)))?;
        }
    }
    for (label, path) in [
        ("Linux JRE", s.sources().runtime_root.as_ref()),
        ("RC.4 JAR", s.sources().engine_jar.as_ref()),
        ("native 설정 원본", s.sources().native_config.as_ref()),
    ] {
        if let Some(path) = path {
            prompt::say(
                out,
                &format!(
                    "{label}: {}\n",
                    prompt::safe_default(&path.to_string_lossy())
                ),
            )?;
        }
    }
    prompt::say(
        out,
        "고정 Linux JRE·RC.4 JAR를 검증해 보관합니다. 원본 설정은 바꾸지 않습니다.\n개발용 공급물이며 실제 Linux 인수·정식 runtime package 승인은 별도입니다.\n",
    )
}
pub(crate) fn run(
    options: Options<'_>,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    let path = workspace(options.home, options.name)?;
    if options.resume
        && (options.from.is_some()
            || options.runtime.is_some()
            || options.jar.is_some()
            || options.native.is_some())
    {
        return Err(invalid());
    }
    let mut opened = home(options.home, false)?;
    if !options.resume {
        Preparation::validate_new(&path, options.name, options.from)?;
        if std::fs::symlink_metadata(&path).is_ok() {
            return Err(BxdlError::new(
                "PREPARATION_EXISTS",
                "같은 이름의 준비가 있습니다. setup resume으로 이어가세요.",
            ));
        }
        check_name(
            options.home,
            options.name,
            &setup::linux::control(options.name)?,
        )?;
        prompt::say(
            out,
            &format!(
                "Linux 설치 준비 · {}\n일반 사용자 준비 기록만 만듭니다. 계정·시스템 파일·DB·서비스는 변경하지 않습니다.\n",
                options.name
            ),
        )?;
        if prompt::confirm(input, out, Policy::Install, "설치 준비를 만들까요?")?
            != Confirmation::Yes
        {
            return Ok(paused());
        }
        Preparation::validate_new(&path, options.name, options.from)?;
        if opened.is_none() {
            opened = home(options.home, true)?;
        }
        opened.as_ref().ok_or_else(invalid)?.ensure_sessions()?;
    }
    let owner = opened.ok_or_else(invalid)?;
    owner.recheck()?;
    let mut session = if options.resume {
        Preparation::open(&path)?
    } else {
        Preparation::create(&path, options.name, options.from)?
    };
    if session.name() != options.name {
        return Err(invalid());
    }
    for (key, value) in [
        ("runtimeRoot", options.runtime),
        ("engineJar", options.jar),
        ("nativeConfig", options.native),
    ] {
        if let Some(path) = value {
            session.set_source(key, path)?;
        }
    }
    prompt::say(
        out,
        "자료 → 제품 설정 → 검토 순으로 진행합니다. 입력은 즉시 저장됩니다.\n:help 도움말 / :back 이전 항목 / :cancel·EOF 중단\n",
    )?;
    if session.summary()?.installation != "NOT_STARTED" {
        return saved(&session, false);
    }
    let source_fields = [
        ("runtimeRoot", "선정 Linux JRE를 풀어 둔 runtime 폴더"),
        ("engineJar", "고정 RC.4 JAR 파일"),
        ("nativeConfig", "기존 NIGO native 설정 JSON 파일"),
    ];
    let fields: Vec<_> = source_fields
        .iter()
        .map(|(name, label)| (*name, *label, true))
        .chain(
            setup::FIELDS
                .iter()
                .filter(|f| !matches!(f.name, "instanceId" | "dataDirectory"))
                .map(|f| (f.name, f.label, false)),
        )
        .collect();
    let complete = |s: &Preparation, name: &str, source: bool| {
        if source {
            match name {
                "runtimeRoot" => s.sources().runtime_root.is_some(),
                "engineJar" => s.sources().engine_jar.is_some(),
                _ => s.sources().native_config.is_some(),
            }
        } else {
            s.draft().has_answer(name)
        }
    };
    let mut index = fields
        .iter()
        .position(|(n, _, source)| !complete(&session, n, *source))
        .unwrap_or(fields.len());
    while index < fields.len() {
        owner.recheck()?;
        session.recheck()?;
        let (name, label, source) = fields[index];
        let default = if source {
            match name {
                "runtimeRoot" => session.sources().runtime_root.as_ref(),
                "engineJar" => session.sources().engine_jar.as_ref(),
                _ => session.sources().native_config.as_ref(),
            }
            .map(|p| p.to_string_lossy().into_owned())
        } else {
            setup::FIELDS
                .iter()
                .position(|f| f.name == name)
                .and_then(|i| session.draft().default_for(i))
        };
        let suffix = default
            .as_ref()
            .map(|s| format!(" [{}]", prompt::safe_default(s)))
            .unwrap_or_default();
        prompt::say(out, &format!("{label}{suffix} > "))?;
        match prompt::read(input, Policy::Install)? {
            Input::Help => prompt::help(
                out,
                if source {
                    "파일/폴더 경로를 입력하세요. 비밀번호 원문을 입력하지 않습니다."
                } else {
                    "제품 항목을 입력하세요. 다른 답은 보존되며 init/start는 별도 승인입니다."
                },
            )?,
            Input::Cancel | Input::Eof => return saved(&session, true),
            Input::Back => {
                if index == 0 {
                    return saved(&session, true);
                }
                index -= 1;
            }
            Input::Value(raw) => {
                let value = if raw.is_empty() {
                    default.unwrap_or_default()
                } else {
                    raw
                };
                let change = if source {
                    session.set_source(name, Path::new(&value))
                } else {
                    session.set_field(name, &value)
                };
                match change {
                    Ok(()) => {
                        index += 1;
                        while index < fields.len()
                            && complete(&session, fields[index].0, fields[index].2)
                        {
                            index += 1
                        }
                    }
                    Err(error) => {
                        prompt::say(out, &format!("{} [{}]\n", error.message, error.code))?
                    }
                }
            }
        }
    }
    say_review(&session, out)?;
    if prompt::confirm(
        input,
        out,
        Policy::Install,
        "이 자료를 검증하고 설치 준비를 저장할까요? (시스템 설치는 하지 않음)",
    )? != Confirmation::Yes
    {
        return saved(&session, true);
    }
    session.prepare()?;
    owner.recheck()?;
    prompt::say(
        out,
        &format!(
            "다음: {}\n",
            next(options.home, "setup apply", options.name)
        ),
    )?;
    saved(&session, false)
}
pub(crate) fn list(home_path: &Path) -> Result<Value> {
    let Some(owner) = home(home_path, false)? else {
        return Ok(json!({"platform":"linux-amd64","entries":[]}));
    };
    let entries = owner
        .preparation_names()?
        .into_iter()
        .map(|name| {
            match workspace(home_path, &name).and_then(|path| Preparation::inspect(&path)) {
                Ok(summary) => serde_json::to_value(summary)
                    .unwrap_or_else(|_| json!({"name":name,"preparation":"UNKNOWN"})),
                Err(_) => json!({"name":name,"platform":"UNKNOWN","preparation":"UNAVAILABLE"}),
            }
        })
        .collect::<Vec<_>>();
    owner.recheck()?;
    Ok(json!({"platform":"linux-amd64","entries":entries,"runtimeObservation":"NOT_CHECKED"}))
}
pub(crate) fn show(home_path: &Path, name: &str) -> Result<Value> {
    let owner = home(home_path, false)?.ok_or_else(invalid)?;
    let session = Preparation::open(&workspace(home_path, name)?)?;
    let mut value = public(&session)?;
    value["reviewedInputs"] = json!(if session.summary()?.preparation == "REVIEWED" {
        match session
            .reviewed_request()
            .and_then(|(_, _, r)| install::recheck(&r))
        {
            Ok(()) => "MATCHED",
            Err(_) => "CHANGED_OR_UNAVAILABLE",
        }
    } else {
        "NOT_REVIEWED"
    });
    owner.recheck()?;
    Ok(value)
}
pub(crate) fn edit(
    home_path: &Path,
    name: &str,
    field: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    let owner = home(home_path, false)?.ok_or_else(invalid)?;
    let mut session = Preparation::open(&workspace(home_path, name)?)?;
    let source = matches!(field, "runtimeRoot" | "engineJar" | "nativeConfig");
    if !source && !setup::FIELDS.iter().any(|f| f.name == field) {
        return Err(invalid());
    }
    prompt::say(
        out,
        &format!(
            "Linux 준비 · {name} · {field}\n다른 입력은 유지합니다. 바꾸면 이전 설치 승인이 해제됩니다.\n새 값 > "
        ),
    )?;
    match prompt::read(input, Policy::Install)? {
        Input::Value(value) if !value.is_empty() => {
            if source {
                session.set_source(field, Path::new(&value))?
            } else {
                session.set_field(field, &value)?
            };
            owner.recheck()?;
            saved(&session, false)
        }
        Input::Help => {
            prompt::help(
                out,
                "한 항목만 바꾸고 종료합니다. 준비 검토는 setup resume으로 이어가세요.",
            )?;
            saved(&session, true)
        }
        _ => saved(&session, true),
    }
}
pub(crate) fn apply(
    home_path: &Path,
    name: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        return Err(BxdlError::new(
            "LINUX_INSTALL_PLATFORM_UNSUPPORTED",
            "설치 적용은 실제 Linux GNU x86_64 머신에서 실행하세요. 준비 기록은 보존했습니다.",
        ));
    }
    let owner = home(home_path, false)?.ok_or_else(invalid)?;
    let mut session = Preparation::open(&workspace(home_path, name)?)?;
    let control = setup::linux::control(name)?;
    check_name(home_path, name, &control)?;
    if let Some(descriptor) = session.installed() {
        if management::descriptor_sha256(descriptor)?
            != management::descriptor_sha256(&management::inspect(&control)?)?
        {
            return Err(invalid());
        }
        super::nodes::add(home_path, name, &control)?;
        return Ok(Finish {
            outcome: "SUCCEEDED",
            code: "LINUX_SETUP_INSTALLED",
            message: "설치 기록을 확인하고 노드 이름을 연결했습니다. 엔진을 다시 실행하지 않았습니다.",
            data: public(&session)?,
            exit: 0,
        });
    }
    let recovering = session.summary()?.installation == "UNRESOLVED";
    let (_, _, request) = session.reviewed_request()?;
    if !recovering {
        install::recheck(&request)?;
    }
    install::check_installer()?;
    say_review(&session, out)?;
    if recovering {
        prompt::say(
            out,
            "이전 요청의 결과를 같은 고정 계획으로 확인합니다. 부분 설치는 보존하며 자동 재설치하지 않습니다.\n",
        )?;
    }
    prompt::say(
        out,
        "이 승인으로 전용 계정·시스템 파일·등록 기록을 준비합니다.\n초기화·서비스 시작은 포함하지 않습니다. 관리자 인증은 sudo가 직접 처리합니다.\n",
    )?;
    if prompt::confirm(
        input,
        out,
        Policy::Install,
        "Linux 시스템 설치를 진행할까요?",
    )? != Confirmation::Yes
    {
        return saved(&session, true);
    }
    owner.recheck()?;
    if !recovering {
        install::recheck(&request)?;
    }
    management::authenticate()?;
    let (path, sha, request) = session.begin_apply()?;
    let receipt = install::invoke(&path, &sha, request.operator_uid, Duration::from_secs(600))?;
    if receipt.request_sha256 != sha || receipt.name != name || receipt.control != control {
        return Err(invalid());
    }
    session.mark_installed(receipt.descriptor)?;
    owner.recheck()?;
    super::nodes::add(home_path, name, &control)?;
    prompt::say(
        out,
        &format!(
            "설치 완료. 다음: {} --confirm-initialize\n서비스 시작: {}\n",
            next(home_path, "init", name),
            next(home_path, "start", name)
        ),
    )?;
    Ok(Finish {
        outcome: "SUCCEEDED",
        code: "LINUX_SETUP_INSTALLED",
        message: "Linux 시스템 등록과 노드 이름 연결을 완료했습니다. 초기화·시작은 별도입니다.",
        data: public(&session)?,
        exit: 0,
    })
}
