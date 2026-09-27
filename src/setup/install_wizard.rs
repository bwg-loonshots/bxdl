use super::{FIELDS, error, workflow::Workflow};
use crate::error::Result;
use std::{
    io::{BufRead, Write},
    path::PathBuf,
};

/// true means the operator chose to finish at the current stage; false is
/// EOF/cancel. Neither outcome schedules later initialization or service work.
pub(super) fn interact(
    w: &mut Workflow,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<bool> {
    say(
        out,
        "BXDL 설치 도우미 — macOS arm64 개발 버전\n설치·등록, DB 초기화, 시작을 각각 선택합니다. :cancel 또는 EOF는 저장 후 중단합니다.\n비밀번호 원문 대신 준비한 파일 경로만 입력하세요. 상대 경로는 최초 시작 위치를 유지합니다.\n",
    )?;
    if w.has_plan() {
        if let Err(failure) = w.reconcile() {
            show_error(out, &failure)?;
        }
    } else {
        match prepare(w, input, out) {
            Ok(false) => return Ok(false),
            Err(failure) => show_error(out, &failure)?,
            Ok(true) => {}
        }
    }
    loop {
        if w.has_plan() {
            w.show_plan(out)?;
        }
        show_status(w, out)?;
        say(out, "\n선택: finish 저장 후 종료 / :cancel 중단\n")?;
        if w.can_edit() {
            say(
                out,
                "  plan 계획 검사 / inputs 설치 자료·목적지 수정 / edit 노드 설정 수정\n",
            )?;
        }
        if w.can_apply() {
            say(out, "  apply 설정 파일·패키지 설치 및 인스턴스 등록\n")?;
        }
        if w.can_init() {
            say(out, "  init 새 데이터 초기화 (별도 확인)\n")?;
        }
        if w.can_resume_init() {
            say(
                out,
                "  resume-init 중단 초기화의 재개 조건 검사 (별도 확인, 자동 복구 아님)\n",
            )?;
        }
        if w.can_start() {
            say(out, "  start 지금 LaunchAgent 시작 (별도 확인)\n")?;
        }
        if w.has_plan() {
            say(out, "  check 저장 결과 재검증·실행 상태 확인\n")?;
        }
        say(out, "선택 [finish] > ")?;
        let Some(answer) = line(input)? else {
            return Ok(false);
        };
        let result = match answer.as_str() {
            "" | "finish" => return Ok(true),
            ":cancel" => return Ok(false),
            "plan" if w.can_edit() => match prepare(w, input, out) {
                Ok(false) => return Ok(false),
                other => other.map(|_| ()),
            },
            "inputs" if w.can_edit() => {
                w.edit()?;
                if !inputs(w, input, out, true)? {
                    return Ok(false);
                }
                w.check_supply()
            }
            "edit" if w.can_edit() => {
                w.edit()?;
                if !fields(w, input, out, 0)? {
                    return Ok(false);
                }
                Ok(())
            }
            "apply" if w.can_apply() => {
                match confirm(
                    input,
                    out,
                    "표시한 새 경로에 설정·패키지를 저장하고 cold 검사·등록을 진행할까요? DB 초기화·시작은 다음 선택입니다.",
                )? {
                    None => return Ok(false),
                    Some(false) => continue,
                    Some(true) => {}
                }
                w.apply()
            }
            "init" if w.can_init() => {
                match confirm(
                    input,
                    out,
                    "표시한 새 데이터 경로에 이 체인의 제네시스를 초기화할까요? 체인·발행자 허용목록을 먼저 확인하세요.",
                )? {
                    None => return Ok(false),
                    Some(false) => continue,
                    Some(true) => {}
                }
                w.initialize(false)
            }
            "resume-init" if w.can_resume_init() => {
                match confirm(
                    input,
                    out,
                    "현재 등록의 미완료 초기화 재개 조건을 검사하고, 조건이 맞을 때만 resume-init을 실행할까요?",
                )? {
                    None => return Ok(false),
                    Some(false) => continue,
                    Some(true) => {}
                }
                w.initialize(true)
            }
            "start" if w.can_start() => {
                match confirm(
                    input,
                    out,
                    "지금 Mac 사용자 LaunchAgent로 노드를 시작할까요? 터미널을 닫아도 실행될 수 있으며 사용자 로그아웃 시 종료됩니다.",
                )? {
                    None => return Ok(false),
                    Some(false) => continue,
                    Some(true) => {}
                }
                w.start()
            }
            "check" if w.has_plan() => w.reconcile(),
            _ => {
                say(out, "표시된 선택 중 하나를 입력하세요.\n")?;
                continue;
            }
        };
        if let Err(failure) = result {
            show_error(out, &failure)?;
        }
    }
}

fn prepare(w: &mut Workflow, input: &mut dyn BufRead, out: &mut dyn Write) -> Result<bool> {
    if !inputs(w, input, out, false)? {
        return Ok(false);
    }
    // Fail before collecting fourteen answers or writing customer destinations.
    w.check_supply()?;
    if !fields(w, input, out, w.draft.next_index())? {
        return Ok(false);
    }
    w.prepare()?;
    Ok(true)
}

fn inputs(
    w: &mut Workflow,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    edit: bool,
) -> Result<bool> {
    for name in [
        "archive",
        "trust",
        "public_key",
        "lock",
        "native",
        "destination",
        "instance",
    ] {
        let mut next = w.inputs().clone();
        if name == "trust" {
            if !edit && next.unsigned.is_some() {
                continue;
            }
            say(
                out,
                "\n패키지 신뢰: signed (외부 공개키) / unsigned-development (서명 없는 개발 후보)\nEnter: 저장값 또는 signed > ",
            )?;
            loop {
                let Some(answer) = line(input)? else {
                    return Ok(false);
                };
                next.unsigned = match answer.as_str() {
                    ":cancel" => return Ok(false),
                    "" => Some(next.unsigned.unwrap_or(false)),
                    "signed" => Some(false),
                    "unsigned-development" => Some(true),
                    _ => {
                        say(out, "signed 또는 unsigned-development를 입력하세요.\n> ")?;
                        continue;
                    }
                };
                if next.unsigned == Some(true) {
                    next.public_key = None;
                }
                w.set_inputs(next)?;
                break;
            }
            continue;
        }
        if name == "public_key" && next.unsigned == Some(true) {
            continue;
        }
        let (slot, label) = match name {
            "archive" => (&mut next.archive, "패키지 archive 경로"),
            "public_key" => (&mut next.public_key, "패키지 밖의 신뢰한 공개키 경로"),
            "lock" => (&mut next.lock, "엔진 lock JSON 경로"),
            "native" => (&mut next.native, "NIGO native node.json 경로"),
            "destination" => (&mut next.destination, "새 패키지 설치 폴더"),
            _ => (&mut next.instance, "새 인스턴스 기록 폴더"),
        };
        if !edit && slot.is_some() {
            continue;
        }
        let default = if slot.is_none() && matches!(name, "destination" | "instance") {
            Some(sibling(
                w,
                if name == "destination" {
                    "package"
                } else {
                    "instance"
                },
            )?)
        } else {
            None
        };
        say(out, &format!("\n{label}\n"))?;
        if slot.is_some() {
            say(out, "Enter: 저장값 유지 > ")?;
        } else if let Some(default) = &default {
            say(out, &format!("Enter: {} > ", default.display()))?;
        } else {
            say(out, "> ")?;
        }
        loop {
            let Some(answer) = line(input)? else {
                return Ok(false);
            };
            if answer == ":cancel" {
                return Ok(false);
            }
            if answer.is_empty() {
                if slot.is_none() {
                    *slot = default.clone();
                }
                if slot.is_none() {
                    say(out, "파일 경로를 입력하세요.\n> ")?;
                    continue;
                }
            } else {
                match w.resolve(&answer) {
                    Ok(path) => *slot = Some(path),
                    Err(failure) => {
                        show_error(out, &failure)?;
                        say(out, "> ")?;
                        continue;
                    }
                }
            }
            w.set_inputs(next)?;
            break;
        }
    }
    Ok(true)
}

fn fields(
    w: &mut Workflow,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    mut index: usize,
) -> Result<bool> {
    while let Some(field) = FIELDS.get(index) {
        let default = if field.name == "dataDirectory" {
            sibling(w, "data")?.to_str().map(str::to_owned)
        } else {
            w.draft.default_for(index)
        };
        say(
            out,
            &format!(
                "\n[{}/{}] {}\n{}\n",
                index + 1,
                FIELDS.len(),
                field.label,
                field.hint
            ),
        )?;
        if w.draft.has_answer(field.name) {
            say(out, "Enter: 저장값 유지 > ")?;
        } else if let Some(default) = &default {
            say(out, &format!("Enter: 기본값 {default} > "))?;
        } else {
            say(out, "> ")?;
        }
        let Some(answer) = line(input)? else {
            return Ok(false);
        };
        if answer == ":cancel" {
            return Ok(false);
        }
        if answer == ":back" {
            index = index.saturating_sub(1);
            continue;
        }
        if answer == ":help" {
            say(
                out,
                ":back 이전 항목, :cancel 저장 후 나가기. 키·비밀번호는 파일 경로만 입력하세요.\n",
            )?;
            continue;
        }
        if answer.is_empty() && w.draft.has_answer(field.name) {
            index += 1;
            continue;
        }
        let value = if answer.is_empty() {
            default.as_deref().unwrap_or("")
        } else {
            &answer
        };
        match w.set_field(field.name, value) {
            Ok(()) => index += 1,
            Err(failure) => show_error(out, &failure)?,
        }
    }
    Ok(true)
}

fn sibling(w: &Workflow, suffix: &str) -> Result<PathBuf> {
    let parent = w
        .workspace()
        .parent()
        .ok_or_else(|| error("SETUP_PATH_INVALID", "작업 경로를 확인하세요."))?;
    let name = w
        .workspace()
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| error("SETUP_PATH_INVALID", "작업 경로를 확인하세요."))?;
    Ok(parent.join(format!("{name}-{suffix}")))
}
fn show_status(w: &Workflow, out: &mut dyn Write) -> Result<()> {
    let s = w.summary();
    say(
        out,
        &format!(
            "\n저장 상태: 설치 {} / 등록 {} / 초기화 {}\n현재 관측: 서비스 {} / 로컬 readiness {} / 네트워크 합의 미검사\n",
            s.installation, s.registration, s.initialization, s.service_state, s.runtime_readiness
        ),
    )
}
fn confirm(input: &mut dyn BufRead, out: &mut dyn Write, text: &str) -> Result<Option<bool>> {
    say(out, &format!("{text}\n[y/N] > "))?;
    Ok(match line(input)? {
        None => None,
        Some(s) if s == ":cancel" => None,
        Some(s) => Some(s == "y" || s == "yes"),
    })
}
fn show_error(out: &mut dyn Write, failure: &crate::error::BxdlError) -> Result<()> {
    say(
        out,
        &format!(
            "{} [{}]\n완료 여부를 확인하지 못한 작업은 자동으로 반복하지 않습니다.\n",
            failure.message, failure.code
        ),
    )
}
fn say(out: &mut dyn Write, text: &str) -> Result<()> {
    out.write_all(text.as_bytes())
        .and_then(|_| out.flush())
        .map_err(|_| {
            error(
                "SETUP_OUTPUT_FAILED",
                "안내를 출력하지 못했습니다. 작업 기록을 보존했습니다.",
            )
        })
}
fn line(input: &mut dyn BufRead) -> Result<Option<String>> {
    let mut raw = Vec::new();
    let n = std::io::Read::take(input, 4098)
        .read_until(b'\n', &mut raw)
        .map_err(|_| {
            error(
                "SETUP_INPUT_FAILED",
                "입력을 읽지 못했습니다. 작업 기록을 보존했습니다.",
            )
        })?;
    if n == 0 {
        return Ok(None);
    }
    if raw.last() == Some(&b'\n') {
        raw.pop();
    }
    if raw.last() == Some(&b'\r') {
        raw.pop();
    }
    if raw.len() > 4096 {
        return Err(error("SETUP_INPUT_INVALID", "입력 길이가 너무 깁니다."));
    }
    let value = String::from_utf8(raw)
        .map_err(|_| error("SETUP_INPUT_INVALID", "일반 텍스트를 입력하세요."))?;
    if value.chars().any(char::is_control) {
        return Err(error(
            "SETUP_INPUT_INVALID",
            "제어 문자를 입력할 수 없습니다.",
        ));
    }
    Ok(Some(value.trim().to_owned()))
}
