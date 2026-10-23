//! Direct named preparation queries and one-field edits. Queries never acquire
//! workflow locks or create management/session state.
use super::{
    application,
    home::Home,
    install_flow,
    prompt::{self, Input, Policy},
};
use crate::{
    error::{BxdlError, Result},
    setup::{
        self,
        preparation::{Inspection, Preparation},
    },
};
use serde::Serialize;
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listing {
    pub schema_version: u32,
    pub home: PathBuf,
    pub partial: bool,
    pub entries: Vec<Entry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    pub name: String,
    pub record: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection: Option<Inspection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<application::View>,
    pub next_command: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Detail {
    pub schema_version: u32,
    pub name: String,
    pub inspection: Option<Inspection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<application::View>,
    pub next_command: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditResult {
    pub schema_version: u32,
    pub name: String,
    pub field: String,
    pub changed: bool,
    pub review_invalidated: bool,
    pub paused: bool,
    pub next_command: Vec<String>,
}

fn invalid_name() -> BxdlError {
    BxdlError::new(
        "PREPARATION_NAME_INVALID",
        "이름은 소문자로 시작하는 1~32자의 소문자·숫자·하이픈이어야 합니다.",
    )
}
fn missing() -> BxdlError {
    BxdlError::new(
        "PREPARATION_NOT_FOUND",
        "준비 세션이 없습니다. 이름과 --home을 확인하고 setup list로 목록을 확인하세요.",
    )
}
fn validate_name(name: &str) -> Result<()> {
    if install_flow::valid_name(name) {
        Ok(())
    } else {
        Err(invalid_name())
    }
}
fn command(action: &str, name: &str, home: &Path) -> Vec<String> {
    vec![
        "bxdl".into(),
        "setup".into(),
        action.into(),
        name.into(),
        "--home".into(),
        home.to_string_lossy().into_owned(),
    ]
}
fn open_selected(home: &Path, name: &str) -> Result<(Home, PathBuf)> {
    validate_name(name)?;
    let root = Home::open(home)?.ok_or_else(missing)?;
    let path = root.path().join("sessions").join(name);
    if std::fs::symlink_metadata(&path)
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Err(missing());
    }
    root.recheck()?;
    Ok((root, path))
}

pub(crate) fn list(home: &Path) -> Result<Listing> {
    let home = setup::absolute(home)?;
    let mut result = Listing {
        schema_version: 1,
        home: home.clone(),
        partial: false,
        entries: Vec::new(),
    };
    let Some(root) = Home::open(&home)? else {
        return Ok(result);
    };
    for name in root.preparation_names()? {
        root.recheck()?;
        let (inspection, reason_code) =
            match Preparation::inspect(&root.path().join("sessions").join(&name), false) {
                Ok(snapshot) => (Some(snapshot), None),
                Err(error) => {
                    result.partial = true;
                    (None, Some(error.code))
                }
            };
        let application = application::inspect(root.path(), &name, false)?;
        result.partial |= application.as_ref().is_some_and(application::View::partial);
        root.recheck()?;
        result.entries.push(Entry {
            record: if inspection.is_some() {
                "READABLE"
            } else {
                "UNAVAILABLE"
            },
            next_command: command("show", &name, &home),
            name,
            inspection,
            application,
            reason_code,
        });
    }
    root.recheck()?;
    Ok(result)
}

pub(crate) fn show(home: &Path, name: &str) -> Result<Detail> {
    let home = setup::absolute(home)?;
    validate_name(name)?;
    let root = Home::open(&home)?.ok_or_else(missing)?;
    let workspace = root.path().join("sessions").join(name);
    let application = application::inspect(root.path(), name, true)?;
    let (inspection, reason_code) = match Preparation::inspect(&workspace, application.is_none()) {
        Ok(value) => (Some(value), None),
        Err(error) if application.is_some() => (None, Some(error.code)),
        Err(error) => return Err(error),
    };
    root.recheck()?;
    let action = if inspection
        .as_ref()
        .is_some_and(|value| value.verification == "VERIFIED")
    {
        "apply"
    } else {
        "resume"
    };
    Ok(Detail {
        schema_version: 1,
        name: name.into(),
        inspection,
        reason_code,
        next_command: application.as_ref().map_or_else(
            || command(action, name, &home),
            |view| application::next_command(&home, name, view),
        ),
        application,
    })
}

/// The command and accepted answer authorize this one field only. A blank
/// answer preserves a saved value; cancellation never adopts a proposed value.
pub(crate) fn edit(
    home: &Path,
    name: &str,
    field_name: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<EditResult> {
    validate_name(name)?;
    let index = setup::FIELDS
        .iter()
        .position(|field| field.name == field_name)
        .ok_or_else(|| {
            BxdlError::new(
                "PREPARATION_FIELD_INVALID",
                "제품 항목 이름을 확인하세요. bxdl setup edit --help에 지원 항목이 있습니다.",
            )
        })?;
    let field = &setup::FIELDS[index];
    let home = setup::absolute(home)?;
    let (root, workspace) = open_selected(&home, name)?;
    let mut session = Preparation::resume(&workspace)?;
    root.recheck()?;
    let reviewed = session.summary().preparation == "PREPARED_RECORDED";
    let mut result = EditResult {
        schema_version: 1,
        name: name.into(),
        field: field_name.into(),
        changed: false,
        review_invalidated: false,
        paused: false,
        next_command: command("show", name, &home),
    };
    loop {
        root.recheck()?;
        session.recheck()?;
        prompt::say(
            out,
            &format!("{name} · {} 수정\n{}\n", field.label, field.hint),
        )?;
        let saved = session.draft().has_answer(field.name);
        let default = session.draft().default_for(index);
        if saved {
            prompt::say(
                out,
                &format!(
                    "현재값: {}\n",
                    prompt::safe_default(&session.review_rows()[index].1)
                ),
            )?;
            prompt::say(
                out,
                "Enter 저장값 유지 / :back 변경 없이 종료 / :cancel 중단 > ",
            )?;
        } else if let Some(default) = &default {
            prompt::say(
                out,
                &format!(
                    "기본값(Enter): {}\n값 입력 > ",
                    prompt::safe_default(default)
                ),
            )?;
        } else {
            prompt::say(out, "값 입력 / :back 변경 없이 종료 / :cancel 중단 > ")?;
        }
        let response = prompt::read(input, Policy::Install)?;
        root.recheck()?;
        session.recheck()?;
        let value = match response {
            Input::Cancel | Input::Eof => {
                result.paused = true;
                return Ok(result);
            }
            Input::Back => return Ok(result),
            Input::Help => {
                prompt::help(out, field.hint)?;
                continue;
            }
            Input::Value(value) if value.is_empty() && saved => return Ok(result),
            Input::Value(value) if value.is_empty() => match default {
                Some(default) => default,
                None => {
                    prompt::say(out, "값을 입력하거나 :back으로 변경 없이 종료하세요.\n")?;
                    continue;
                }
            },
            Input::Value(value) => value,
        };
        if let Err(error) = session.validate_field(field.name, &value) {
            prompt::say(
                out,
                &format!(
                    "입력 형식·포트·경로를 확인하세요. 기존 값은 보존됩니다. [{}]\n",
                    error.code
                ),
            )?;
            continue;
        }
        let unchanged = session.field_matches(field.name, &value);
        match session.set_field(field.name, &value) {
            Ok(()) => {
                root.recheck()?;
                session.recheck()?;
                result.changed = !unchanged;
                result.review_invalidated = reviewed && !unchanged;
                if result.changed {
                    result.next_command = command("resume", name, &home);
                }
                return Ok(result);
            }
            Err(error) => return Err(error),
        }
    }
}
