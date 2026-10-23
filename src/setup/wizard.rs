use super::{FIELDS, Session};
use crate::{
    config,
    error::Result,
    operator::prompt::{self, Confirmation, Input, Policy, Selection, say},
};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

pub enum Finish {
    Paused,
    Saved,
    Exported(PathBuf),
}

/// Accepted answers are checkpoints; inspection and declined confirmations
/// never create exported configuration, engine data or service work.
pub fn interact(
    session: &mut Session,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    output: Option<&Path>,
) -> Result<Finish> {
    say(
        out,
        "BXDL 설정 도우미 — macOS 우선 개발 버전\n설정 초안을 준비합니다. 설치·키 생성·DB 초기화·노드 시작은 수행하지 않습니다.\n값마다 저장합니다. :back 이전 항목, :cancel 저장 후 나가기, :help 도움말\n상대 경로는 처음 시작한 작업 디렉터리 기준이며 재개해도 유지됩니다.\n비밀번호 원문을 입력하지 마세요. 준비한 키·비밀번호 파일의 경로만 사용합니다.\n",
    )?;
    if !fields(session, input, out, session.next_index(), false)? {
        return Ok(Finish::Paused);
    }
    loop {
        let report = session.preflight()?;
        show_review(session, out)?;
        show_checks(out, &report)?;
        say(
            out,
            "\n설정 입력이 완료되었습니다. 엔진 검사는 미실행이며 노드를 시작할 수 있다는 뜻은 아닙니다.\n  save: 초안만 저장하고 종료\n  export: 설정 JSON 저장\n  edit: 항목 하나 수정\n  check: 로컬 검사 다시 실행\n  :cancel: 초안을 보존하고 나가기\n선택 [save] > ",
        )?;
        match prompt::read(input, Policy::Product)? {
            Input::Cancel | Input::Eof => return Ok(Finish::Paused),
            Input::Help => prompt::help(
                out,
                "save는 초안만 보존합니다. export는 경로를 확인하고 y/yes로 승인한 경우에만 새 설정 파일을 저장합니다. edit은 항목 하나를 수정합니다.",
            )?,
            Input::Back => {
                if !edit(session, input, out)? {
                    return Ok(Finish::Paused);
                }
            }
            Input::Value(answer) => match answer.as_str() {
                "" | "save" => return Ok(Finish::Saved),
                "check" => continue,
                "edit" => {
                    if !edit(session, input, out)? {
                        return Ok(Finish::Paused);
                    }
                }
                "export" => {
                    let destination = match output {
                        Some(path) => Some(path.to_path_buf()),
                        None => loop {
                            say(
                                out,
                                "출력 파일 경로 (Enter: 작업 폴더의 instance.json, :back 검토, :cancel 나가기) > ",
                            )?;
                            match prompt::read(input, Policy::Product)? {
                                Input::Cancel | Input::Eof => return Ok(Finish::Paused),
                                Input::Back => break None,
                                Input::Help => prompt::help(
                                    out,
                                    "새 설정 JSON을 저장할 경로입니다. 기존 파일은 덮어쓰지 않습니다. Enter는 작업 폴더의 instance.json입니다.",
                                )?,
                                Input::Value(value) => {
                                    break Some(if value.is_empty() {
                                        session.path().join("instance.json")
                                    } else {
                                        PathBuf::from(value)
                                    });
                                }
                            }
                        },
                    };
                    let Some(destination) = destination else {
                        continue;
                    };
                    let destination = super::absolute(&destination)?;
                    match prompt::confirm(
                        input,
                        out,
                        Policy::Product,
                        &format!(
                            "새 설정 파일: {}\n기존 파일은 덮어쓰지 않습니다. 설치·초기화·시작은 수행하지 않습니다.\n이 파일을 저장할까요?",
                            destination.display()
                        ),
                    )? {
                        Confirmation::Paused => return Ok(Finish::Paused),
                        Confirmation::No => {
                            say(out, "파일 저장을 취소했습니다. 초안은 보존됩니다.\n")?;
                            continue;
                        }
                        Confirmation::Yes => {}
                    }
                    match session.export(&destination) {
                        Ok(path) => return Ok(Finish::Exported(path)),
                        Err(failure) => say(
                            out,
                            &format!(
                                "설정 파일을 저장하지 못했습니다. 초안은 보존됩니다. {} [{}]\n다른 경로를 선택하거나 --resume으로 다시 실행하세요.\n",
                                failure.message, failure.code
                            ),
                        )?,
                    }
                }
                _ => say(out, "save, export, edit, check 중 하나를 선택하세요.\n")?,
            },
        }
    }
}

fn edit(session: &mut Session, input: &mut dyn BufRead, out: &mut dyn Write) -> Result<bool> {
    let labels: Vec<_> = FIELDS.iter().map(|field| field.label).collect();
    match prompt::select_field(input, out, Policy::Product, &labels)? {
        Selection::Paused => Ok(false),
        Selection::Back => Ok(true),
        Selection::Index(index) => fields(session, input, out, index, true),
    }
}

fn fields(
    session: &mut Session,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    mut index: usize,
    one: bool,
) -> Result<bool> {
    while let Some(field) = FIELDS.get(index) {
        let default = session.default_for(index);
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
        if session.has_answer(field.name) {
            say(out, "Enter: 저장값 유지 > ")?;
        } else if let Some(default) = &default {
            say(
                out,
                &format!("Enter: 기본값 {} > ", prompt::safe_default(default)),
            )?;
        } else {
            say(out, "> ")?;
        }
        let answer = match prompt::read(input, Policy::Product)? {
            Input::Cancel | Input::Eof => return Ok(false),
            Input::Back if one => return Ok(true),
            Input::Back => {
                index = index.saturating_sub(1);
                continue;
            }
            Input::Help => {
                prompt::help(out, field.hint)?;
                continue;
            }
            Input::Value(value) => value,
        };
        if answer.is_empty() && session.has_answer(field.name) {
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
        if value.is_empty() {
            say(
                out,
                "이 항목은 필수입니다. 값을 입력하거나 :cancel로 나가세요.\n",
            )?;
            continue;
        }
        match session.set(field.name, value) {
            Ok(()) => {
                if one {
                    return Ok(true);
                }
                index += 1;
            }
            Err(failure) if is_input_error(&failure.code) => say(
                out,
                &format!("{} [{}]\n", correction(&failure.code), failure.code),
            )?,
            Err(failure) => return Err(failure),
        }
    }
    Ok(true)
}

pub(super) fn show_review(session: &Session, out: &mut dyn Write) -> Result<()> {
    say(out, "\n입력 검토 — 공개 값과 자료 선택 상태\n")?;
    for (label, value) in session.review_rows() {
        say(out, &format!("  {label}: {value}\n"))?;
    }
    say(
        out,
        "파일 경로·자격 자료의 원문은 표시하지 않습니다. 선택됨은 파일 내용·키 identity 검증 완료가 아닙니다.\n",
    )
}

fn show_checks(out: &mut dyn Write, report: &config::Report) -> Result<()> {
    say(out, "\n로컬 사전 검사 (파일 종류·권한만 확인)\n")?;
    for check in &report.checks {
        if check.status == "FAIL" {
            say(
                out,
                &format!(
                    "  • {}: {}\n",
                    check_label(&check.name),
                    check_correction(&check.reason_code)
                ),
            )?;
        }
    }
    if report.outcome == "FAIL" {
        say(
            out,
            "누락된 자료를 준비하거나 edit으로 경로를 수정한 뒤 check로 다시 검사하세요. 실패 상태에서도 설정 초안은 저장할 수 있습니다.\n",
        )?;
    } else {
        say(
            out,
            "로컬 항목 검사가 끝났습니다. 엔진·키 내용·DB·포트 가용성·서비스 검사는 남아 있습니다.\n",
        )?;
    }
    Ok(())
}

fn check_label(name: &str) -> &str {
    match name {
        "configMetadata" => "설정 파일",
        "chainDescriptionMetadata" => "공통 체인 자료",
        "validatorKeystoreMetadata" => "Validator keystore",
        "validatorPasswordMetadata" => "Validator 비밀번호 파일",
        "tlsKeyStoreMetadata" => "TLS keystore",
        "tlsKeyPasswordMetadata" => "TLS keystore 비밀번호 파일",
        "tlsTrustStoreMetadata" => "TLS truststore",
        "tlsTrustPasswordMetadata" => "TLS truststore 비밀번호 파일",
        "dataDirectoryMetadata" => "데이터 저장 위치",
        _ => "입력 자료",
    }
}

fn check_correction(code: &str) -> &str {
    match code {
        "REFERENCE_MISSING" => "파일이 없습니다. 준비한 자료의 경로를 확인하세요.",
        "REFERENCE_SYMLINK" => "링크가 아닌 실제 파일 경로를 지정하세요.",
        "REFERENCE_NOT_REGULAR" => "디렉터리 대신 일반 파일을 지정하세요.",
        "REFERENCE_PERMISSIONS_UNSAFE" => {
            "파일 권한을 확인하세요. 키·비밀번호 파일은 본인만 읽고 쓸 수 있게 두는 것이 좋습니다."
        }
        "SECRET_DIRECTORY_PERMISSIONS_UNSAFE" => {
            "키·비밀번호 파일의 상위 폴더 접근 권한을 제한하세요."
        }
        "DATA_PERMISSIONS_UNSAFE" => "데이터 폴더의 접근 권한을 제한하세요.",
        "DATA_NOT_DIRECTORY" => "데이터 위치는 디렉터리여야 합니다.",
        _ => "파일 종류와 상위 경로의 접근 권한을 확인하세요.",
    }
}

fn is_input_error(code: &str) -> bool {
    matches!(
        code,
        "SETUP_INPUT_INVALID"
            | "SETUP_PATH_INVALID"
            | "SETUP_PATH_UNAVAILABLE"
            | "INSTANCE_ID_INVALID"
            | "NODE_ID_INVALID"
            | "HTTP_ADDRESS_INVALID"
            | "P2P_ADDRESS_INVALID"
            | "PORTS_INVALID"
            | "REFERENCE_PATH_INVALID"
            | "REFERENCE_PATH_CONFLICT"
            | "SETUP_PATH_CONFLICT"
            | "CONFIG_SCHEMA_INVALID"
    )
}

fn correction(code: &str) -> &'static str {
    match code {
        "INSTANCE_ID_INVALID" => {
            "이름은 소문자로 시작하고 소문자·숫자·하이픈으로 1~32자만 사용하세요."
        }
        "NODE_ID_INVALID" => {
            "공개 노드 ID 형식을 확인하세요. 영숫자로 시작하는 1~128자의 영숫자·점·밑줄·콜론·하이픈을 사용합니다."
        }
        "HTTP_ADDRESS_INVALID" => "노드 HTTP IP는 127.0.0.1 또는 ::1이어야 합니다.",
        "P2P_ADDRESS_INVALID" => "P2P에는 wildcard·multicast가 아닌 명시적인 IP 주소를 입력하세요.",
        "PORTS_INVALID" => "포트는 1~65535의 정수이며 HTTP와 P2P가 서로 달라야 합니다.",
        "SETUP_PATH_CONFLICT" => "데이터·체인·키 자료는 setup 작업 폴더와 분리하세요.",
        "REFERENCE_PATH_CONFLICT" => "데이터 디렉터리와 입력 파일 경로를 서로 다르게 지정하세요.",
        "REFERENCE_PATH_INVALID" | "SETUP_PATH_INVALID" | "SETUP_PATH_UNAVAILABLE" => {
            "실제 파일 경로를 입력하세요. ~ 축약·제어문자·설정 자기 참조는 사용할 수 없습니다."
        }
        _ => "입력 형식을 확인한 뒤 다시 입력하세요.",
    }
}
