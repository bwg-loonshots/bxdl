use super::{FIELDS, error, workflow::Workflow};
use crate::{
    error::Result,
    operator::prompt::{self, Confirmation, Input, Policy, Selection, say},
};
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
        "BXDL 설치 도우미 — macOS arm64 개발 버전\n설치·등록, DB 초기화, 시작을 각각 선택합니다. :help 도움말 / :back 이전 항목·화면 / :cancel 또는 EOF 저장 후 중단\n비밀번호 원문 대신 준비한 파일 경로만 입력하세요. 상대 경로는 최초 시작 위치를 유지합니다.\n",
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
        super::wizard::show_review(&w.draft, out)?;
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
        if w.can_wait() {
            say(
                out,
                "  wait 현재 시작 시도의 준비 상태 기다리기 (최대 120초)\n",
            )?;
        }
        if w.has_plan() {
            say(out, "  check 저장 결과 재검증·실행 상태 확인\n")?;
        }
        say(out, "선택 [finish] > ")?;
        let answer = match prompt::read(input, Policy::Install)? {
            Input::Cancel | Input::Eof => return Ok(false),
            Input::Help => {
                prompt::help(
                    out,
                    "plan으로 검토한 뒤 apply 설치·등록, init 초기화, start 시작을 각각 선택하고 승인합니다. inputs/edit은 한 항목만 수정합니다. UNKNOWN 작업은 자동으로 반복하지 않습니다.",
                )?;
                continue;
            }
            Input::Back => {
                say(
                    out,
                    "설치 검토 화면입니다. inputs/edit으로 항목을 선택하거나 finish로 종료하세요.\n",
                )?;
                continue;
            }
            Input::Value(value) => value,
        };
        let result = match answer.as_str() {
            "" | "finish" => return Ok(true),
            "plan" if w.can_edit() => match prepare(w, input, out) {
                Ok(false) => return Ok(false),
                other => other.map(|_| ()),
            },
            "inputs" if w.can_edit() => {
                let labels = SUPPLY_FIELDS.map(|(_, label, _)| label);
                match prompt::select_field(input, out, Policy::Install, &labels)? {
                    Selection::Paused => return Ok(false),
                    Selection::Back => continue,
                    Selection::Index(index) => {
                        if !inputs(w, input, out, Some(index))? {
                            return Ok(false);
                        }
                    }
                }
                w.check_supply()
            }
            "edit" if w.can_edit() => {
                let labels: Vec<_> = FIELDS.iter().map(|field| field.label).collect();
                match prompt::select_field(input, out, Policy::Install, &labels)? {
                    Selection::Paused => return Ok(false),
                    Selection::Back => continue,
                    Selection::Index(index) => {
                        if !fields(w, input, out, index, true)? {
                            return Ok(false);
                        }
                    }
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
                    "표시한 새 데이터 경로에 이 체인의 제네시스를 초기화할까요? 체인·자산 등록 모드·허용목록을 먼저 확인하세요.",
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
            "wait" if w.can_wait() => {
                say(
                    out,
                    "현재 시작 시도만 관측합니다. 새 초기화·시작 요청 없이 최대 120초 기다립니다.\n",
                )?;
                w.wait()
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
    if !inputs(w, input, out, None)? {
        return Ok(false);
    }
    // Fail before collecting fourteen answers or writing customer destinations.
    w.check_supply()?;
    if !fields(w, input, out, w.draft.next_index(), false)? {
        return Ok(false);
    }
    w.prepare()?;
    Ok(true)
}

const SUPPLY_FIELDS: [(&str, &str, &str); 7] = [
    (
        "archive",
        "패키지 archive 경로",
        "공급자가 제공한 macOS arm64 BXDL archive 파일입니다. 입력만으로 설치하지 않습니다.",
    ),
    (
        "trust",
        "패키지 신뢰 방식",
        "signed는 패키지와 별도로 신뢰한 공개키가 필요합니다. unsigned-development는 서명 없는 개발 후보에만 사용합니다.",
    ),
    (
        "public_key",
        "패키지 밖의 신뢰한 공개키 경로",
        "패키지와 독립적으로 신뢰한 공급자 공개키입니다. 파일 존재만으로 신뢰를 확정하지 않습니다.",
    ),
    (
        "lock",
        "엔진 lock JSON 경로",
        "공급 엔진의 고정 identity와 일치하는 lock 파일입니다.",
    ),
    (
        "native",
        "NIGO native node.json 경로",
        "기존 승인 native 설정 또는 setup --network로 준비한 native 파일입니다.",
    ),
    (
        "destination",
        "새 패키지 설치 폴더",
        "기존 폴더를 채택하거나 덮어쓰지 않습니다. instance·data·작업 폴더와 분리하세요.",
    ),
    (
        "instance",
        "새 인스턴스 기록 폴더",
        "설치 등록과 실행 기록을 보관할 새 폴더입니다. package·data·작업 폴더와 분리하세요.",
    ),
];

fn inputs(
    w: &mut Workflow,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    selected: Option<usize>,
) -> Result<bool> {
    let mut index = selected.unwrap_or(0);
    let mut backtracking = false;
    while let Some(&(name, label, hint)) = SUPPLY_FIELDS.get(index) {
        let mut next = w.inputs().clone();
        if name == "public_key" && next.unsigned == Some(true) {
            if selected.is_some() {
                say(
                    out,
                    "unsigned-development에서는 공개키를 선택하지 않습니다. 신뢰 방식을 signed로 바꾼 뒤 공개키를 지정하세요.\n",
                )?;
                return Ok(true);
            }
            index += 1;
            continue;
        }
        let current = match name {
            "archive" => next.archive.as_ref(),
            "public_key" => next.public_key.as_ref(),
            "lock" => next.lock.as_ref(),
            "native" => next.native.as_ref(),
            "destination" => next.destination.as_ref(),
            "instance" => next.instance.as_ref(),
            _ => None,
        };
        let answered = if name == "trust" {
            next.unsigned.is_some()
        } else {
            current.is_some()
        };
        if selected.is_none() && !backtracking && answered {
            index += 1;
            continue;
        }
        let default = if !answered && matches!(name, "destination" | "instance") {
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
        if name == "trust" {
            say(
                out,
                "signed (외부 공개키) / unsigned-development (서명 없는 개발 후보)\nEnter: 저장값 또는 signed > ",
            )?;
        } else if answered {
            say(out, "Enter: 저장값 유지 > ")?;
        } else if let Some(default) = &default {
            say(out, &format!("Enter: {} > ", default.display()))?;
        } else {
            say(out, "> ")?;
        }
        let answer = match prompt::read(input, Policy::Install)? {
            Input::Cancel | Input::Eof => return Ok(false),
            Input::Help => {
                prompt::help(out, hint)?;
                continue;
            }
            Input::Back if selected.is_some() => return Ok(true),
            Input::Back => {
                index = index.saturating_sub(1);
                if index == 2 && next.unsigned == Some(true) {
                    index = 1;
                }
                backtracking = true;
                continue;
            }
            Input::Value(value) => value,
        };
        if name == "trust" {
            next.unsigned = match answer.as_str() {
                "" => Some(next.unsigned.unwrap_or(false)),
                "signed" => Some(false),
                "unsigned-development" => Some(true),
                _ => {
                    say(out, "signed 또는 unsigned-development를 입력하세요.\n")?;
                    continue;
                }
            };
            if next.unsigned == Some(true) {
                next.public_key = None;
            }
        } else {
            let slot = match name {
                "archive" => &mut next.archive,
                "public_key" => &mut next.public_key,
                "lock" => &mut next.lock,
                "native" => &mut next.native,
                "destination" => &mut next.destination,
                _ => &mut next.instance,
            };
            if answer.is_empty() {
                if slot.is_none() {
                    *slot = default;
                }
                if slot.is_none() {
                    say(out, "파일 경로를 입력하세요.\n")?;
                    continue;
                }
            } else {
                match w.resolve(&answer) {
                    Ok(path) => *slot = Some(path),
                    Err(failure) => {
                        show_error(out, &failure)?;
                        continue;
                    }
                }
            }
        }
        w.set_inputs(next)?;
        if selected.is_some() {
            return Ok(true);
        }
        index += 1;
    }
    Ok(true)
}

fn fields(
    w: &mut Workflow,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    mut index: usize,
    one: bool,
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
        let answer = match prompt::read(input, Policy::Install)? {
            Input::Cancel | Input::Eof => return Ok(false),
            Input::Help => {
                prompt::help(out, field.hint)?;
                continue;
            }
            Input::Back if one => return Ok(true),
            Input::Back => {
                index = index.saturating_sub(1);
                continue;
            }
            Input::Value(value) => value,
        };
        if answer.is_empty() && w.draft.has_answer(field.name) {
            if one {
                return Ok(true);
            }
            index += 1;
            continue;
        }
        let value = if answer.is_empty() {
            default.as_deref().unwrap_or("")
        } else {
            &answer
        };
        match w.set_field(field.name, value) {
            Ok(()) => {
                if one {
                    return Ok(true);
                }
                index += 1;
            }
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
            "\n저장 상태: 설치 {} / 등록 {} / 초기화 {}\n",
            s.installation, s.registration, s.initialization
        ),
    )?;
    let projection = serde_json::to_value(s).map_err(|_| {
        error(
            "SETUP_OUTPUT_FAILED",
            "현재 관측 결과를 구성하지 못했습니다.",
        )
    })?;
    crate::cli::print_service_summary(out, &projection)
        .and_then(|_| out.flush())
        .map_err(|_| {
            error(
                "SETUP_OUTPUT_FAILED",
                "안내를 출력하지 못했습니다. 작업 기록을 보존했습니다.",
            )
        })
}
fn confirm(input: &mut dyn BufRead, out: &mut dyn Write, text: &str) -> Result<Option<bool>> {
    Ok(match prompt::confirm(input, out, Policy::Install, text)? {
        Confirmation::Paused => None,
        Confirmation::No => Some(false),
        Confirmation::Yes => Some(true),
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
