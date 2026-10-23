//! Shared preparation controller for direct setup commands and console menus.
//! This stage never installs, registers, initializes or starts a node.
use super::{
    catalog::Catalog,
    home::Home,
    prompt::{self, Confirmation, Input, Policy, Selection},
};
use crate::{
    config::network::{NetworkMode, RuntimeOptions, StoreType},
    error::{BxdlError, Result},
    setup::{self, preparation::Preparation},
};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Finish {
    Saved,
    Paused,
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn invalid_name() -> BxdlError {
    BxdlError::new(
        "PREPARATION_NAME_INVALID",
        "이름은 소문자로 시작하는 1~32자의 소문자·숫자·하이픈이어야 합니다.",
    )
}
fn state_error() -> BxdlError {
    BxdlError::new(
        "PREPARATION_HOME_INVALID",
        "관리 루트 또는 준비 세션을 확인할 수 없습니다. 기존 자료는 자동 채택하지 않습니다.",
    )
}

pub(crate) fn run(
    home_path: &Path,
    name: &str,
    resume: bool,
    from: Option<&Path>,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    if !valid_name(name) {
        return Err(invalid_name());
    }
    let home_path = setup::absolute(home_path)?;
    let workspace = home_path.join("sessions").join(name);
    let mut home = Home::open(&home_path)?;
    if !resume {
        validate_creation(&home_path, &workspace, from)?;
        if std::fs::symlink_metadata(&workspace).is_ok() {
            return Err(BxdlError::new(
                "PREPARATION_EXISTS",
                "같은 이름의 세션이 있습니다. setup resume으로 이어가거나 새 이름을 사용하세요.",
            ));
        }
        prompt::say(
            out,
            &format!(
                "새 설치 준비 · {name}\n관리 위치: {}\n준비 세션: {}\n승인하면 준비 기록과 설정 초안만 저장합니다. 패키지·DB·서비스는 생성하지 않습니다.\n",
                prompt::safe_default(&home_path.to_string_lossy()),
                prompt::safe_default(&workspace.to_string_lossy())
            ),
        )?;
        if prompt::confirm(input, out, Policy::Install, "이 준비 세션을 만들까요?")?
            != Confirmation::Yes
        {
            return Ok(Finish::Paused);
        }
        validate_creation(&home_path, &workspace, from)?;
        if home.is_none() {
            Catalog::create(&home_path)?;
            home = Home::open(&home_path)?;
        }
        home.as_ref().ok_or_else(state_error)?.ensure_sessions()?;
    }
    let home = home.ok_or_else(state_error)?;
    home.recheck()?;
    let mut session = if resume {
        Preparation::resume(&workspace)?
    } else {
        Preparation::create(&workspace, from)?
    };
    home.recheck()?;
    if !resume {
        if from.is_none() {
            session.set_field("instanceId", name)?;
            session.set_field(
                "dataDirectory",
                path_text(&home_path.join("data").join(name))?,
            )?;
        }
        let mut inputs = session.inputs().clone();
        inputs.destination = Some(home_path.join("packages").join(name));
        inputs.instance = Some(home_path.join("instances").join(name));
        session.set_inputs(inputs)?;
    }
    prompt::say(
        out,
        "BXDL 설치 준비 · 자료 → 네트워크 → 제품 설정 → 검토\n각 답을 저장합니다. :help 도움말 / :back 검토 / :cancel·EOF 중단\n설치·등록·초기화·시작은 다음 단계이며 이 화면에서 실행하지 않습니다.\n",
    )?;
    let mut ui = Wizard {
        session: &mut session,
        input,
        out,
    };
    loop {
        home.recheck()?;
        ui.review()?;
        prompt::say(
            ui.out,
            "\n1  제품 설정 채우기·수정\n2  공급 자료 수정 (package·신뢰키·lock)\n3  네트워크 준비 (공개 목록 또는 기존 native)\n4  설치·제어·데이터 경로 수정\n5  검토하고 새 설정 파일 준비\n6  상세 경로와 재개 명령\n7  제품 항목 하나 수정 (미완성 초안도 가능)\n0  저장하고 종료\n선택 [0] > ",
        )?;
        let continuing = match ui.read()? {
            Input::Cancel | Input::Eof => return Ok(Finish::Paused),
            Input::Back => return Ok(Finish::Saved),
            Input::Help => {
                ui.help("공급 자료와 네트워크를 선택한 뒤 제품의 나머지 항목을 채우세요. 5번은 설정 파일 준비만 승인하며 설치를 승인하지 않습니다.")?;
                true
            }
            Input::Value(value) => match value.as_str() {
                "" | "0" => return Ok(Finish::Saved),
                "1" => ui.product()?,
                "2" => ui.supply()?,
                "3" => ui.network()?,
                "4" => ui.destinations()?,
                "7" => ui.edit_product()?,
                "5" => ui.prepare()?,
                "6" => {
                    ui.details(&home_path, name)?;
                    true
                }
                _ => {
                    prompt::say(ui.out, "표시된 번호를 입력하세요.\n")?;
                    true
                }
            },
        };
        if !continuing {
            return Ok(Finish::Paused);
        }
    }
}

fn validate_creation(home: &Path, workspace: &Path, from: Option<&Path>) -> Result<()> {
    Preparation::validate_new(workspace, from)?;
    // Guard every home write, including newly created namespace parents, against
    // imported data/credentials. A sibling managed configs/ input remains valid.
    for candidate in [
        home.join("catalog"),
        home.join(".bxdl-home.json"),
        home.join(".catalog.lock"),
    ] {
        Preparation::validate_new(&candidate, from)?;
    }
    let sessions = home.join("sessions");
    if !sessions.exists() {
        Preparation::validate_new(&sessions, from)?;
    }
    Ok(())
}
fn path_text(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(state_error)
}

/// Entry within the console. Session names are scoped to the chosen management
/// root; duplicate display labels from the node catalog are never interpreted.
pub(crate) fn menu(home: &Path, input: &mut dyn BufRead, out: &mut dyn Write) -> Result<Finish> {
    prompt::say(
        out,
        "\n설치 준비\n1  새 준비 세션\n2  준비 세션 재개\n0  홈으로\n선택 [0] > ",
    )?;
    let resume = loop {
        match prompt::read(input, Policy::Install)? {
            Input::Cancel | Input::Eof => return Ok(Finish::Paused),
            Input::Back => return Ok(Finish::Saved),
            Input::Help => prompt::help(
                out,
                "새 세션과 기존 준비 세션 재개는 직접 setup new/resume과 같은 흐름입니다. 기존 setup --install 세션은 기존 메뉴에서 열 수 있습니다.",
            )?,
            Input::Value(value) => match value.as_str() {
                "" | "0" => return Ok(Finish::Saved),
                "1" => break false,
                "2" => break true,
                _ => prompt::say(out, "1, 2, 0 중 하나를 입력하세요.\n선택 [0] > ")?,
            },
        }
    };
    if resume {
        if let Some(root) = Home::open(home)? {
            let names = root.session_names()?;
            prompt::say(out, "관리 위치의 세션 이름 (선택 후 기록 확인):\n")?;
            for name in names {
                prompt::say(out, &format!("  {name}\n"))?;
            }
        }
    }
    let name = loop {
        prompt::say(out, "준비 세션 이름 (Enter·:back 홈으로) > ")?;
        match prompt::read(input, Policy::Install)? {
            Input::Cancel | Input::Eof => return Ok(Finish::Paused),
            Input::Back => return Ok(Finish::Saved),
            Input::Help => prompt::help(
                out,
                "소문자로 시작하는 1~32자의 소문자·숫자·하이픈. 같은 관리 위치 안에서 고유하며 경로를 입력하지 않습니다.",
            )?,
            Input::Value(value) if value.is_empty() => return Ok(Finish::Saved),
            Input::Value(value) if valid_name(&value) => break value,
            Input::Value(_) => prompt::say(out, "소문자로 시작하는 1~32자 이름을 입력하세요.\n")?,
        }
    };
    let from = if resume {
        None
    } else {
        loop {
            prompt::say(out, "기존 제품 설정 파일 (Enter 새 초안, :back 홈으로) > ")?;
            match prompt::read(input, Policy::Install)? {
                Input::Cancel | Input::Eof => return Ok(Finish::Paused),
                Input::Back => return Ok(Finish::Saved),
                Input::Help => prompt::help(
                    out,
                    "기존 instance.json을 가져오면 모든 제품 값을 다시 입력하지 않습니다. 상대 참조는 원본 설정 위치에 고정합니다.",
                )?,
                Input::Value(value) => {
                    break if value.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(value))
                    };
                }
            }
        }
    };
    run(home, &name, resume, from.as_deref(), input, out)
}

struct Wizard<'a> {
    session: &'a mut Preparation,
    input: &'a mut dyn BufRead,
    out: &'a mut dyn Write,
}
impl Wizard<'_> {
    fn read(&mut self) -> Result<Input> {
        prompt::read(self.input, Policy::Install)
    }
    fn help(&mut self, text: &str) -> Result<()> {
        prompt::help(self.out, text)
    }
    fn problem(&mut self, error: BxdlError) -> Result<()> {
        if matches!(
            error.code.as_str(),
            "SETUP_INPUT_FAILED" | "SETUP_OUTPUT_FAILED"
        ) {
            return Err(error);
        }
        let code = if error.code.len() <= 80
            && error
                .code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            &error.code
        } else {
            "PREPARATION_FAILED"
        };
        prompt::say(
            self.out,
            &format!(
                "선택한 자료·필수 항목·권한·경로 충돌을 확인하세요. 기존 답과 파일은 보존됩니다. [{code}]\n"
            ),
        )
    }
    fn review(&mut self) -> Result<()> {
        let data = serde_json::to_value(self.session.summary()).map_err(|_| state_error())?;
        prompt::say(self.out, "\n설치 준비 검토\n")?;
        for (label, value) in self.session.review_rows() {
            prompt::say(
                self.out,
                &format!("  {label}: {}\n", prompt::safe_default(&value)),
            )?;
        }
        let inputs = self.session.inputs();
        prompt::say(
            self.out,
            &format!(
                "공급 자료: package {} · 신뢰 정책 {} · lock {}\n네트워크: {}\n저장된 준비 기록: {}\n",
                chosen(inputs.archive.is_some()),
                chosen(inputs.unsigned.is_some()),
                chosen(inputs.lock.is_some()),
                if inputs.network.is_some() {
                    "공개 목록 선택됨"
                } else if inputs.native.is_some() {
                    "기존 native 선택됨"
                } else {
                    "미선택"
                },
                data["preparation"].as_str().unwrap_or("DRAFT")
            ),
        )?;
        prompt::say(
            self.out,
            &format!(
                "신뢰 정책: {}\n",
                match inputs.unsigned {
                    Some(true) => "서명 없는 개발 자료 허용",
                    Some(false) if inputs.public_key.is_some() => "외부 신뢰 공개키 서명 검증",
                    _ => "미선택",
                }
            ),
        )?;
        if let Some(options) = &inputs.runtime_options {
            prompt::say(
                self.out,
                &format!(
                    "준비 정책: TLS {} / trust {} · {} · 합의 참여 {} · archive {}\n",
                    option_default(options, 0),
                    option_default(options, 1),
                    option_default(options, 2),
                    option_default(options, 3),
                    option_default(options, 4)
                ),
            )?;
        } else if inputs.network.is_some() {
            prompt::say(
                self.out,
                &format!(
                    "준비 정책: {}/5개 답변 저장 · 모두 선택해야 파일 준비 가능\n",
                    inputs.runtime_answers.len()
                ),
            )?;
        }
        if data["preparation"].as_str() == Some("PREPARED_RECORDED") {
            match self.session.check() {
                Ok(()) => prompt::say(
                    self.out,
                    "현재 자료와 저장한 준비 기록 일치 · 실행 검증 아님\n",
                )?,
                Err(error) => {
                    self.problem(error)?;
                    prompt::say(
                        self.out,
                        "저장 기록을 현재 유효한 준비 결과로 사용하지 마세요. 항목을 수정하고 5번으로 다시 검토하세요.\n",
                    )?;
                }
            }
            if data["cliMatch"].as_str() == Some("MISMATCH") {
                prompt::say(
                    self.out,
                    "준비 당시 package의 CLI가 현재 도구와 다릅니다. 실제 설치·시작 전 지원 실행기 profile 또는 CLI 일치를 검증합니다.\n",
                )?;
            }
        }
        prompt::say(
            self.out,
            "설치 NOT_PERFORMED · 엔진/키 identity/네트워크 READY NOT_CHECKED\n",
        )
    }
    fn product(&mut self) -> Result<bool> {
        if !self.session.draft().complete() {
            // Only missing answers are asked; a selected network supplies node/p2p.
            for index in 0..setup::FIELDS.len() {
                if !self.session.draft().has_answer(setup::FIELDS[index].name)
                    && !self.field(index)?
                {
                    return Ok(false);
                }
                if !self.session.draft().has_answer(setup::FIELDS[index].name) {
                    return Ok(true);
                }
            }
            return Ok(true);
        }
        self.edit_product()
    }
    fn edit_product(&mut self) -> Result<bool> {
        let labels: Vec<_> = setup::FIELDS.iter().map(|f| f.label).collect();
        match prompt::select_field(self.input, self.out, Policy::Install, &labels)? {
            Selection::Paused => Ok(false),
            Selection::Back => Ok(true),
            Selection::Index(index) => self.field(index),
        }
    }
    fn field(&mut self, index: usize) -> Result<bool> {
        let field = &setup::FIELDS[index];
        loop {
            prompt::say(self.out, &format!("{}\n{}\n", field.label, field.hint))?;
            let saved = self.session.draft().has_answer(field.name);
            let default = self.session.draft().default_for(index);
            prompt::say(
                self.out,
                if saved {
                    "Enter 저장값 유지 / :back 검토 > "
                } else {
                    "값 입력 / :back 검토 > "
                },
            )?;
            if !saved {
                if let Some(value) = &default {
                    prompt::say(
                        self.out,
                        &format!("기본값(Enter): {}\n> ", prompt::safe_default(value)),
                    )?;
                }
            }
            let answer = match self.read()? {
                Input::Cancel | Input::Eof => return Ok(false),
                Input::Back => return Ok(true),
                Input::Help => {
                    self.help(field.hint)?;
                    continue;
                }
                Input::Value(value) if value.is_empty() && saved => return Ok(true),
                Input::Value(value) if value.is_empty() => match default {
                    Some(value) => value,
                    None => {
                        prompt::say(self.out, "값을 입력하거나 :back으로 검토에 돌아가세요.\n")?;
                        continue;
                    }
                },
                Input::Value(value) => value,
            };
            match self.session.set_field(field.name, &answer) {
                Ok(()) => return Ok(true),
                Err(error) => self.problem(error)?,
            }
        }
    }
    fn supply(&mut self) -> Result<bool> {
        match prompt::select_field(
            self.input,
            self.out,
            Policy::Install,
            &["패키지 archive", "검증 신뢰 정책", "engine lock"],
        )? {
            Selection::Paused => Ok(false),
            Selection::Back => Ok(true),
            Selection::Index(0) => self.input_path("archive"),
            Selection::Index(2) => self.input_path("lock"),
            Selection::Index(_) => {
                prompt::say(
                    self.out,
                    "1  외부 신뢰 공개키\n2  서명 없는 개발 자료 허용\n0  검토로\n선택 [0] > ",
                )?;
                loop {
                    match self.read()? {
                        Input::Cancel | Input::Eof => return Ok(false), Input::Back => return Ok(true),
                        Input::Help => self.help("공개키는 package와 독립적으로 신뢰한 경로에서 받으세요. 서명 없는 자료 허용은 개발 전용이며 묵시적으로 선택하지 않습니다.")?,
                        Input::Value(value) if value == "1" => return self.input_path("public_key"),
                        Input::Value(value) if value == "2" => {
                            match prompt::confirm(self.input, self.out, Policy::Install, "서명 없는 개발 자료를 이 준비 세션에서 명시적으로 허용할까요?")? {
                                Confirmation::Paused => return Ok(false), Confirmation::No => return Ok(true), Confirmation::Yes => {}
                            }
                            let mut inputs = self.session.inputs().clone(); inputs.unsigned = Some(true); inputs.public_key = None;
                            if let Err(error) = self.session.set_inputs(inputs) { self.problem(error)?; }
                            return Ok(true);
                        },
                        Input::Value(value) if value.is_empty() || value == "0" => return Ok(true),
                        _ => prompt::say(self.out, "1, 2, 0 중 하나를 선택하세요.\n")?,
                    }
                }
            }
        }
    }
    fn input_path(&mut self, field: &str) -> Result<bool> {
        loop {
            prompt::say(
                self.out,
                "파일/폴더 경로 (Enter 유지, :back 검토; 상대경로는 세션 생성 위치 기준) > ",
            )?;
            let value = match self.read()? {
                Input::Cancel | Input::Eof => return Ok(false),
                Input::Back => return Ok(true),
                Input::Help => {
                    self.help("기존 자료를 가리키는 경로만 입력하세요. 비밀번호 원문을 입력하지 않습니다. 출력 경로는 기존 운영 자료와 겹치지 않아야 합니다.")?;
                    continue;
                }
                Input::Value(value) if value.is_empty() => return Ok(true),
                Input::Value(value) => value,
            };
            let path = match self.session.resolve(&value) {
                Ok(path) => path,
                Err(error) => {
                    self.problem(error)?;
                    continue;
                }
            };
            let mut inputs = self.session.inputs().clone();
            match field {
                "archive" => inputs.archive = Some(path),
                "lock" => inputs.lock = Some(path),
                "public_key" => {
                    inputs.public_key = Some(path);
                    inputs.unsigned = Some(false);
                }
                "network" => {
                    inputs.network = Some(path);
                    inputs.native = None;
                }
                "native" => {
                    inputs.native = Some(path);
                    inputs.network = None;
                    inputs.runtime_options = None;
                }
                "destination" => inputs.destination = Some(path),
                "instance" => inputs.instance = Some(path),
                _ => return Err(state_error()),
            }
            match self.session.set_inputs(inputs) {
                Ok(()) => return Ok(true),
                Err(error) => self.problem(error)?,
            }
        }
    }
    fn network(&mut self) -> Result<bool> {
        prompt::say(
            self.out,
            "1  공통 공개 목록에서 내 노드 선택\n2  기존 native 설정 사용\n0  검토로\n선택 [0] > ",
        )?;
        loop {
            match self.read()? {
                Input::Cancel | Input::Eof => return Ok(false), Input::Back => return Ok(true),
                Input::Help => self.help("승인한 chain·network를 제공받아 자기 노드를 선택하세요. 입력 형식 검사이며 승인 주체·인증서 identity·실제 네트워크의 검증은 아닙니다.")?,
                Input::Value(value) if value == "2" => return self.input_path("native"),
                Input::Value(value) if value == "1" => break,
                Input::Value(value) if value.is_empty() || value == "0" => return Ok(true),
                _ => prompt::say(self.out, "1, 2, 0 중 하나를 선택하세요.\n")?,
            }
        }
        if !self.session.draft().has_answer("chainDescription") && !self.field(7)? {
            return Ok(false);
        }
        if !self.session.draft().has_answer("chainDescription") {
            return Ok(true);
        }
        prompt::say(self.out, "공통 network.json 선택\n")?;
        if !self.input_path("network")? {
            return Ok(false);
        }
        let choices = match self.session.node_choices() {
            Ok(value) => value,
            Err(error) => {
                self.problem(error)?;
                return Ok(true);
            }
        };
        for (index, node) in choices.iter().enumerate() {
            prompt::say(
                self.out,
                &format!(
                    "{}  node {} · validator {} · {}:{}\n",
                    index + 1,
                    node.node_id,
                    node.validator_id,
                    node.host,
                    node.port
                ),
            )?;
        }
        loop {
            prompt::say(self.out, "내 노드 번호 (:back 검토) > ")?;
            match self.read()? {
                Input::Cancel | Input::Eof => return Ok(false), Input::Back => return Ok(true),
                Input::Help => self.help("본인에게 배정된 공개 node ID와 validator ID를 확인하세요. 선택하면 nodeId와 P2P 주소/포트를 제품 초안에 함께 저장합니다. 기존 signer/TLS 키는 이동하지 않습니다.")?,
                Input::Value(value) => {
                    let Some(index) = value.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).filter(|i| *i < choices.len()) else { prompt::say(self.out, "표시된 번호를 선택하세요.\n")?; continue; };
                    if let Err(error) = self.session.select_node(&choices[index].node_id) { self.problem(error)?; return Ok(true); }
                    break;
                }
            }
        }
        self.options()
    }
    fn options(&mut self) -> Result<bool> {
        let prompts = [
            "TLS keystore 형식: PKCS12 / JKS",
            "TLS truststore 형식: PKCS12 / JKS",
            "시작 정책: new-network / join-network",
            "나중에 엔진 시작 시 합의 참여: yes / no",
            "블록 아카이브 사용: yes / no",
        ];
        for (index, label) in prompts.iter().enumerate() {
            loop {
                let options = self
                    .session
                    .inputs()
                    .runtime_options
                    .clone()
                    .unwrap_or_default();
                let key = setup::preparation::RUNTIME_FIELDS[index];
                let default = self
                    .session
                    .inputs()
                    .runtime_answers
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| option_default(&options, index).to_owned());
                prompt::say(
                    self.out,
                    &format!("{label} (Enter {default}, :back 검토) > "),
                )?;
                let value = match self.read()? {
                    Input::Cancel | Input::Eof => return Ok(false),
                    Input::Back => return Ok(true),
                    Input::Help => {
                        self.help("정책은 답마다 저장하며 다섯 항목을 모두 선택해야 파일 준비가 가능합니다. 합의 참여 yes도 나중에 노드를 명시 시작했을 때 적용됩니다.")?;
                        continue;
                    }
                    Input::Value(value) if value.is_empty() => default,
                    Input::Value(value) => value,
                };
                match self.session.set_runtime_answer(index, &value) {
                    Ok(()) => break,
                    Err(error) => self.problem(error)?,
                }
            }
        }
        Ok(true)
    }
    fn destinations(&mut self) -> Result<bool> {
        match prompt::select_field(
            self.input,
            self.out,
            Policy::Install,
            &["패키지 설치 위치", "인스턴스 제어 위치", "데이터 위치"],
        )? {
            Selection::Paused => Ok(false),
            Selection::Back => Ok(true),
            Selection::Index(0) => self.input_path("destination"),
            Selection::Index(1) => self.input_path("instance"),
            Selection::Index(_) => self.field(2),
        }
    }
    fn prepare(&mut self) -> Result<bool> {
        self.review()?;
        self.material_details()?;
        match prompt::confirm(
            self.input,
            self.out,
            Policy::Install,
            "공급 자료를 검증하고 새로운 제품/native 설정 revision을 준비할까요?\n참조 경로를 포함한 0600 설정 파일을 세션에 보존합니다. 기존 출력은 덮어쓰지 않습니다.\n설치·등록·초기화·시작은 수행하지 않습니다.",
        )? {
            Confirmation::Paused => return Ok(false),
            Confirmation::No => return Ok(true),
            Confirmation::Yes => {}
        }
        match self.session.prepare() {
            Ok(_) => prompt::say(
                self.out,
                "설정 파일 준비를 마쳤습니다. 이 세션과 참조 자료를 보존하세요. 실제 설치 실행은 UX-04에서 연결합니다.\n",
            )?,
            Err(error) => self.problem(error)?,
        }
        Ok(true)
    }
    fn material_details(&mut self) -> Result<()> {
        prompt::say(
            self.out,
            "명시 자료·출력 경로 검토 (credential 파일 경로는 제외)\n",
        )?;
        let inputs = self.session.inputs();
        for (label, path) in [
            ("package archive", &inputs.archive),
            ("외부 신뢰 공개키", &inputs.public_key),
            ("engine lock", &inputs.lock),
            ("공통 network", &inputs.network),
            ("기존 native", &inputs.native),
            ("설치 예정", &inputs.destination),
            ("제어 예정", &inputs.instance),
        ] {
            if let Some(path) = path {
                prompt::say(
                    self.out,
                    &format!(
                        "{label}: {}\n",
                        prompt::safe_default(&path.to_string_lossy())
                    ),
                )?;
            }
        }
        for (label, path) in self.session.public_locations()? {
            prompt::say(
                self.out,
                &format!(
                    "{label}: {}\n",
                    prompt::safe_default(&path.to_string_lossy())
                ),
            )?;
        }
        prompt::say(
            self.out,
            &format!(
                "설정 출력 위치: {}\n",
                prompt::safe_default(&self.session.path().join("generated").to_string_lossy())
            ),
        )
    }
    fn details(&mut self, home: &Path, name: &str) -> Result<()> {
        prompt::say(
            self.out,
            &format!(
                "준비 세션: {}\n",
                prompt::safe_default(&self.session.path().to_string_lossy())
            ),
        )?;
        self.material_details()?;
        prompt::say(
            self.out,
            &format!(
                "재개: bxdl setup resume {name} --home {}\n관리 이름은 instanceId와 별개이며 이 단계에서는 노드 실행 별칭이 아닙니다.\n",
                shell_quote(home)
            ),
        )
    }
}
fn chosen(present: bool) -> &'static str {
    if present { "선택됨" } else { "미선택" }
}
fn option_default(options: &RuntimeOptions, index: usize) -> &'static str {
    match index {
        0 => {
            if options.key_store_type == StoreType::Jks {
                "JKS"
            } else {
                "PKCS12"
            }
        }
        1 => {
            if options.trust_store_type == StoreType::Jks {
                "JKS"
            } else {
                "PKCS12"
            }
        }
        2 => {
            if options.mode == NetworkMode::JoinNetwork {
                "join-network"
            } else {
                "new-network"
            }
        }
        3 => {
            if options.consensus_auto_start {
                "yes"
            } else {
                "no"
            }
        }
        _ => {
            if options.archive_enabled {
                "yes"
            } else {
                "no"
            }
        }
    }
}
fn shell_quote(path: &Path) -> String {
    format!(
        "'{}'",
        prompt::safe_default(&path.to_string_lossy()).replace('\'', "'\\''")
    )
}
