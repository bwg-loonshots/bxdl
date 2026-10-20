use super::{
    error,
    network::{FIELDS, Session},
};
use crate::{
    config::network::{NetworkMode, StoreType},
    error::Result,
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

pub fn interact(
    session: &mut Session,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    output: Option<&Path>,
) -> Result<Finish> {
    say(
        out,
        "BXDL 네트워크 준비 도우미 — macOS 우선 개발 버전\n기존 제품 설정·공통 네트워크 자료로 native node.json을 준비합니다.\n키·인증서를 발급하거나 설치·초기화·노드 시작을 수행하지 않습니다.\n각 선택을 저장합니다. :back 이전 항목, :cancel 저장 후 종료, :help 도움말\n비밀번호 원문을 입력하지 마세요. 기존 제품 설정의 파일 참조를 사용합니다.\n",
    )?;
    let mut index = session.next_index();
    loop {
        while let Some(field) = FIELDS.get(index) {
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
                say(out, "Enter: 저장한 선택 유지 > ")?;
            } else {
                say(
                    out,
                    &format!("Enter: 기본 선택 {} > ", field.default.unwrap_or("")),
                )?;
            }
            let Some(answer) = line(input)? else {
                return Ok(Finish::Paused);
            };
            match answer.as_str() {
                ":cancel" => return Ok(Finish::Paused),
                ":back" => {
                    index = index.saturating_sub(1);
                    continue;
                }
                ":help" => {
                    say(
                        out,
                        "표시한 정책을 확인하고 Enter로 기본 선택을 수락하거나 원하는 값을 입력하세요. :back으로 수정하고 --resume으로 이어갈 수 있습니다.\n",
                    )?;
                    continue;
                }
                "" if session.has_answer(field.name) => {
                    index += 1;
                    continue;
                }
                _ => {}
            }
            let value = if answer.is_empty() {
                field.default.unwrap_or("")
            } else {
                &answer
            };
            match session.set(field.name, value) {
                Ok(()) => index += 1,
                Err(failure) if failure.code == "SETUP_NETWORK_OPTION_INVALID" => {
                    say(out, "표시된 선택지 중 하나를 입력하세요.\n")?
                }
                Err(failure) => return Err(failure),
            }
        }
        session.check()?;
        show_summary(session, out)?;
        say(
            out,
            "\n선택: export native 파일 저장 / save 초안만 저장 / edit 선택 수정 / check 입력 재검사 / :cancel 중단\n선택 [save] > ",
        )?;
        let Some(answer) = line(input)? else {
            return Ok(Finish::Paused);
        };
        match answer.as_str() {
            "" | "save" => return Ok(Finish::Saved),
            ":cancel" => return Ok(Finish::Paused),
            "check" => continue,
            "edit" | ":back" => {
                index = 0;
            }
            "export" => {
                let destination = if let Some(output) = output {
                    output.to_owned()
                } else {
                    let default = session.default_output()?;
                    say(
                        out,
                        &format!(
                            "새 native 파일 경로 (Enter: {}, :cancel 중단) > ",
                            default.display()
                        ),
                    )?;
                    let Some(answer) = line(input)? else {
                        return Ok(Finish::Paused);
                    };
                    if answer == ":cancel" {
                        return Ok(Finish::Paused);
                    }
                    if answer.is_empty() {
                        default
                    } else {
                        session.resolve(Path::new(&answer))?
                    }
                };
                let destination = session.resolve(&destination)?;
                say(
                    out,
                    &format!(
                        "새 native 파일: {}\n기존 파일은 덮어쓰지 않습니다. 입력 자료의 파일 참조를 포함한 0600 설정 파일을 저장할까요? [y/N] > ",
                        destination.display()
                    ),
                )?;
                let Some(answer) = line(input)? else {
                    return Ok(Finish::Paused);
                };
                if answer == ":cancel" {
                    return Ok(Finish::Paused);
                }
                if answer != "y" && answer != "yes" {
                    say(out, "파일 저장을 취소했습니다. 초안은 보존됩니다.\n")?;
                    continue;
                }
                match session.export(&destination) {
                    Ok(path) => {
                        say(
                            out,
                            &format!(
                                "native 파일을 저장했습니다: {}\n다음 설치 도우미에서 이 파일을 native 입력으로 선택하세요.\n기존 제품 설정: {}\nbxdl setup --install --workspace <새-설치-작업-폴더> --from <위-제품-설정>\n입력 자료와 생성 파일은 설치 이후에도 보존하세요. 엔진 cold 검사와 실제 키·인증서·네트워크 검증은 설치 단계에 남아 있습니다.\n",
                                path.display(),
                                session.product_path().display()
                            ),
                        )?;
                        return Ok(Finish::Exported(path));
                    }
                    Err(failure) if retryable_output_error(&failure.code) => say(
                        out,
                        &format!(
                            "이 경로에 새 파일을 저장하지 않았습니다. 초안과 기존 파일은 보존합니다. {} [{}]\n",
                            failure.message, failure.code
                        ),
                    )?,
                    // Input changes, storage I/O and uncertain publication are
                    // terminal here. A later save must not report success over
                    // an output that may already have been published.
                    Err(failure) => return Err(failure),
                }
            }
            _ => say(out, "export, save, edit, check 중 하나를 선택하세요.\n")?,
        }
    }
}

fn show_summary(session: &Session, out: &mut dyn Write) -> Result<()> {
    let summary = session.summary();
    let config = summary
        .configuration
        .ok_or_else(|| error("SETUP_NETWORK_INCOMPLETE", "선택이 남아 있습니다."))?;
    say(
        out,
        &format!(
            "\n준비할 설정 요약\n인스턴스: {}\n공개 node ID: {}\n공개 validator ID: {}\n체인 validator 수: {} / 연결 peer 수: {}\n로컬 HTTP: {}:{}\nP2P: {}:{}\n거래 수신 정책: {}\n네트워크 시작 방식: {}\nTLS 형식: keystore {} / truststore {}\n엔진 실행 후 합의 시작: {} / 아카이브: {}\n키·인증서 실제 identity: 미검사\n엔진 설정·네트워크 연결·합의: 미검사\njoin-network는 기존 네트워크의 사용 가능한 archive source가 필요합니다. 공급 노드의 가용성·개별 pin 신뢰는 이 단계에서 검사하지 않습니다.\n아카이브를 켜면 저장 공간이 추가로 필요합니다. 네트워크 전체에서 끄면 archive 기반 따라잡기에 필요한 공급 노드가 없습니다.\n",
            config.instance_id,
            config.node_id,
            config.validator_id,
            config.validator_count,
            config.peer_count,
            config.http_address,
            config.http_port,
            config.p2p_address,
            config.p2p_port,
            config.transaction_permission,
            match config.options.mode {
                NetworkMode::NewNetwork => "new-network",
                NetworkMode::JoinNetwork => "join-network",
            },
            store_type(config.options.key_store_type),
            store_type(config.options.trust_store_type),
            if config.options.consensus_auto_start {
                "yes"
            } else {
                "no"
            },
            if config.options.archive_enabled {
                "yes"
            } else {
                "no"
            }
        ),
    )
}
fn store_type(value: StoreType) -> &'static str {
    match value {
        StoreType::Pkcs12 => "PKCS12",
        StoreType::Jks => "JKS",
    }
}
fn say(out: &mut dyn Write, text: &str) -> Result<()> {
    out.write_all(text.as_bytes())
        .and_then(|_| out.flush())
        .map_err(|_| {
            error(
                "SETUP_OUTPUT_FAILED",
                "안내를 출력하지 못했습니다. --resume으로 이어가세요.",
            )
        })
}
fn line(input: &mut dyn BufRead) -> Result<Option<String>> {
    let mut raw = Vec::new();
    let count = std::io::Read::take(input, 4098)
        .read_until(b'\n', &mut raw)
        .map_err(|_| {
            error(
                "SETUP_INPUT_FAILED",
                "입력을 읽지 못했습니다. 마지막 선택을 보존했습니다.",
            )
        })?;
    if count == 0 {
        return Ok(None);
    }
    if raw.last() == Some(&b'\n') {
        raw.pop();
    }
    if raw.last() == Some(&b'\r') {
        raw.pop();
    }
    if raw.len() > 4096 {
        return Err(error(
            "SETUP_INPUT_TOO_LARGE",
            "입력이 너무 깁니다. 마지막 선택을 보존했습니다.",
        ));
    }
    let answer = String::from_utf8(raw)
        .map_err(|_| error("SETUP_INPUT_INVALID", "입력은 UTF-8 텍스트여야 합니다."))?;
    if answer.chars().any(char::is_control) {
        return Err(error(
            "SETUP_INPUT_INVALID",
            "입력에 제어문자를 사용할 수 없습니다.",
        ));
    }
    Ok(Some(answer))
}

fn retryable_output_error(code: &str) -> bool {
    matches!(
        code,
        "OUTPUT_EXISTS"
            | "SETUP_OUTPUT_CONFLICT"
            | "SETUP_PATH_INVALID"
            | "SETUP_PATH_UNAVAILABLE"
            | "SETUP_PATH_CONFLICT"
    )
}
