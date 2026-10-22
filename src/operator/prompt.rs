//! Shared terminal input rules. This layer never writes checkpoints or runs work.
use crate::error::{BxdlError, Result};
use std::io::{BufRead, Write};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Input {
    Value(String),
    Help,
    Back,
    Cancel,
    Eof,
}

/// Retain the existing installers' whitespace and overlong-input contracts.
#[derive(Clone, Copy)]
pub(crate) enum Policy {
    Product,
    Install,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Confirmation {
    Yes,
    No,
    Paused,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Index(usize),
    Back,
    Paused,
}

pub(crate) fn read(input: &mut dyn BufRead, policy: Policy) -> Result<Input> {
    let mut raw = Vec::new();
    let count = std::io::Read::take(input, 4098)
        .read_until(b'\n', &mut raw)
        .map_err(|_| {
            BxdlError::new(
                "SETUP_INPUT_FAILED",
                "입력을 읽지 못했습니다. 저장한 작업은 보존됩니다.",
            )
        })?;
    if count == 0 {
        return Ok(Input::Eof);
    }
    if raw.last() == Some(&b'\n') {
        raw.pop();
    }
    if raw.last() == Some(&b'\r') {
        raw.pop();
    }
    if raw.len() > 4096 {
        let code = match policy {
            Policy::Product => "SETUP_INPUT_TOO_LARGE",
            Policy::Install => "SETUP_INPUT_INVALID",
        };
        return Err(BxdlError::new(
            code,
            "입력이 너무 깁니다. 저장한 작업은 보존됩니다.",
        ));
    }
    let value = String::from_utf8(raw)
        .map_err(|_| BxdlError::new("SETUP_INPUT_INVALID", "입력은 UTF-8 텍스트여야 합니다."))?;
    if value.chars().any(char::is_control) {
        return Err(BxdlError::new(
            "SETUP_INPUT_INVALID",
            "입력에 제어문자를 사용할 수 없습니다.",
        ));
    }
    let value = match policy {
        Policy::Product => value,
        Policy::Install => value.trim().to_owned(),
    };
    Ok(match value.as_str() {
        ":help" => Input::Help,
        ":back" => Input::Back,
        ":cancel" => Input::Cancel,
        _ => Input::Value(value),
    })
}

pub(crate) fn say(out: &mut dyn Write, text: &str) -> Result<()> {
    out.write_all(text.as_bytes())
        .and_then(|_| out.flush())
        .map_err(|_| {
            BxdlError::new(
                "SETUP_OUTPUT_FAILED",
                "안내를 출력하지 못했습니다. 저장한 작업은 보존됩니다.",
            )
        })
}

/// Defaults may include environment-derived paths; never emit terminal controls.
pub(crate) fn safe_default(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control()).collect()
}

pub(crate) fn help(out: &mut dyn Write, context: &str) -> Result<()> {
    say(
        out,
        &format!(
            "{context}\n:help 도움말 / :back 이전 항목·화면 / :cancel 저장한 작업을 보존하고 중단\nEOF도 중단합니다. 실제 파일명이 :help, :back, :cancel이면 ./ 접두어(예: ./:help) 또는 절대 경로를 사용하세요.\n"
        ),
    )
}

pub(crate) fn confirm(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    policy: Policy,
    text: &str,
) -> Result<Confirmation> {
    loop {
        say(out, &format!("{text}\n[y/N] > "))?;
        match read(input, policy)? {
            Input::Help => help(
                out,
                "y 또는 yes만 승인합니다. Enter·다른 값·:back은 실행하지 않고 검토로 돌아갑니다.",
            )?,
            Input::Cancel | Input::Eof => return Ok(Confirmation::Paused),
            Input::Back => return Ok(Confirmation::No),
            Input::Value(value) => {
                return Ok(if value == "y" || value == "yes" {
                    Confirmation::Yes
                } else {
                    Confirmation::No
                });
            }
        }
    }
}

pub(crate) fn select_field(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    policy: Policy,
    labels: &[&str],
) -> Result<Selection> {
    loop {
        for (index, label) in labels.iter().enumerate() {
            say(out, &format!("{:2}. {label}\n", index + 1))?;
        }
        say(out, "수정할 항목 번호 (Enter·:back 검토로 돌아가기) > ")?;
        match read(input, policy)? {
            Input::Help => help(
                out,
                "항목 번호를 선택하면 그 항목만 수정한 뒤 검토로 돌아갑니다. 다른 답은 보존합니다.",
            )?,
            Input::Back => return Ok(Selection::Back),
            Input::Cancel | Input::Eof => return Ok(Selection::Paused),
            Input::Value(value) if value.is_empty() => return Ok(Selection::Back),
            Input::Value(value) => {
                if let Some(number) = value
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=labels.len()).contains(n))
                {
                    return Ok(Selection::Index(number - 1));
                }
                say(out, "목록의 번호를 입력하세요.\n")?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn commands_and_explicit_paths_are_distinct_without_changing_value_rules() {
        for (raw, expected) in [
            (":help\n", Input::Help),
            (":back\r\n", Input::Back),
            (":cancel\n", Input::Cancel),
            ("", Input::Eof),
            ("./:help\n", Input::Value("./:help".into())),
            ("/tmp/:back\n", Input::Value("/tmp/:back".into())),
            (" file \n", Input::Value(" file ".into())),
        ] {
            assert_eq!(
                read(&mut Cursor::new(raw), Policy::Product).unwrap(),
                expected
            );
        }
        assert_eq!(
            read(&mut Cursor::new("  :help  \n"), Policy::Install).unwrap(),
            Input::Help
        );
    }

    #[test]
    fn generated_defaults_cannot_emit_environment_terminal_controls() {
        assert_eq!(
            safe_default("/Users/a\n\u{1b}[31m/Library/BXDL"),
            "/Users/a[31m/Library/BXDL"
        );
    }

    #[test]
    fn input_bounds_and_invalid_text_keep_legacy_error_codes() {
        for (policy, code) in [
            (Policy::Product, "SETUP_INPUT_TOO_LARGE"),
            (Policy::Install, "SETUP_INPUT_INVALID"),
        ] {
            assert_eq!(
                read(&mut Cursor::new("x".repeat(4097)), policy)
                    .unwrap_err()
                    .code,
                code
            );
            assert!(matches!(
                read(&mut Cursor::new("x".repeat(4096)), policy).unwrap(),
                Input::Value(_)
            ));
            for bytes in [vec![0xff], vec![b'x', 0x1b, b'\n']] {
                assert_eq!(
                    read(&mut Cursor::new(bytes), policy).unwrap_err().code,
                    "SETUP_INPUT_INVALID"
                );
            }
        }
    }

    #[test]
    fn approval_requires_explicit_yes_after_help_and_defaults_to_no() {
        for (raw, expected) in [
            ("\n", Confirmation::No),
            (":back\n", Confirmation::No),
            (":help\n\n", Confirmation::No),
            (":cancel\ny\n", Confirmation::Paused),
            ("", Confirmation::Paused),
            (":help\nyes\n", Confirmation::Yes),
        ] {
            assert_eq!(
                confirm(
                    &mut Cursor::new(raw),
                    &mut Vec::new(),
                    Policy::Product,
                    "승인"
                )
                .unwrap(),
                expected
            );
        }
    }
}
