//! Direct CLI preparation: ask missing inputs in dependency order and finish.
//! Navigation is transient; accepted answers use the existing durable journal.
use super::*;
use crate::setup::preparation::RUNTIME_FIELDS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Archive,
    Trust,
    Lock,
    Source,
    Product(usize),
    Node,
    Policy(usize),
    Destination,
    Instance,
}
#[derive(PartialEq, Eq)]
enum Answer {
    Next,
    Back,
    Paused,
}

const POLICY_LABELS: [&str; 5] = [
    "TLS keystore 형식: PKCS12 / JKS",
    "TLS truststore 형식: PKCS12 / JKS",
    "시작 정책: new-network / join-network",
    "나중에 엔진 시작 시 합의 참여: yes / no",
    "블록 아카이브 사용: yes / no",
];

fn steps(session: &Preparation) -> Vec<Step> {
    let mut result = vec![Step::Archive, Step::Trust, Step::Lock, Step::Source];
    let network = session.inputs().network.is_some();
    if network {
        result.extend([Step::Product(7), Step::Node]);
    }
    result.extend((0..setup::FIELDS.len()).filter_map(|index| {
        (!network || ![1, 5, 6, 7].contains(&index)).then_some(Step::Product(index))
    }));
    if network {
        result.extend((0..5).map(Step::Policy));
    }
    result.extend([Step::Destination, Step::Instance]);
    result
}

fn node_matches(session: &Preparation) -> bool {
    session.selected_node_matches().unwrap_or(false)
}

fn complete(session: &Preparation, step: Step) -> bool {
    let inputs = session.inputs();
    match step {
        Step::Archive => inputs.archive.is_some(),
        Step::Trust => matches!(
            (inputs.unsigned, &inputs.public_key),
            (Some(true), None) | (Some(false), Some(_))
        ),
        Step::Lock => inputs.lock.is_some(),
        Step::Source => inputs.network.is_some() || inputs.native.is_some(),
        Step::Product(index) => session.draft().has_answer(setup::FIELDS[index].name),
        Step::Node => node_matches(session),
        Step::Policy(index) => {
            inputs.runtime_options.is_some()
                || inputs.runtime_answers.contains_key(RUNTIME_FIELDS[index])
        }
        Step::Destination => inputs.destination.is_some(),
        Step::Instance => inputs.instance.is_some(),
    }
}

pub(super) fn run(
    home: &Home,
    home_path: &Path,
    name: &str,
    session: &mut Preparation,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    prompt::say(
        out,
        "BXDL 설치 준비 · 자료 → 네트워크 → 필요한 설정 → 검토\n저장된 답은 재사용하고 부족한 항목부터 이어갑니다. 각 답은 바로 저장됩니다.\n:help 도움말 / :back 이전 질문 / :cancel·EOF 중단\n",
    )?;
    let mut ui = Wizard {
        session,
        input,
        out,
    };
    let mut current = steps(ui.session)
        .into_iter()
        .find(|step| !complete(ui.session, *step));
    loop {
        home.recheck()?;
        ui.session.recheck()?;
        if let Some(step) = current {
            let answer = ask(&mut ui, home, step)?;
            home.recheck()?;
            let order = steps(ui.session);
            let index = order
                .iter()
                .position(|candidate| *candidate == step)
                .ok_or_else(state_error)?;
            match answer {
                Answer::Paused => return finish(&mut ui, home_path, name, Finish::Paused),
                Answer::Back if index == 0 => {
                    return finish(&mut ui, home_path, name, Finish::Saved);
                }
                Answer::Back => current = Some(order[index - 1]),
                Answer::Next => {
                    current = order
                        .into_iter()
                        .skip(index + 1)
                        .find(|next| !complete(ui.session, *next));
                }
            }
            continue;
        }
        ui.review()?;
        for (label, path) in [
            ("설치 예정", &ui.session.inputs().destination),
            ("제어 예정", &ui.session.inputs().instance),
        ] {
            if let Some(path) = path {
                prompt::say(
                    ui.out,
                    &format!(
                        "{label}: {}\n",
                        prompt::safe_default(&path.to_string_lossy())
                    ),
                )?;
            }
        }
        prompt::say(
            ui.out,
            "\n이 내용으로 설정 파일을 준비할까요? [y/N]\ny/yes 준비 · Enter/n 저장 후 종료 · edit <항목> 한 항목 수정 · details 경로 확인\n:help 항목 안내 / :back 이전 질문 / :cancel 중단\n> ",
        )?;
        let answer = ui.read()?;
        home.recheck()?;
        ui.session.recheck()?;
        match answer {
            Input::Cancel | Input::Eof => return finish(&mut ui, home_path, name, Finish::Paused),
            Input::Back => current = steps(ui.session).last().copied(),
            Input::Help => review_help(&mut ui)?,
            Input::Value(value) if value.is_empty() || value == "n" || value == "no" => {
                return finish(&mut ui, home_path, name, Finish::Saved);
            }
            Input::Value(value) if value == "details" => ui.material_details()?,
            Input::Value(value) if value == "edit" => review_help(&mut ui)?,
            Input::Value(value) if value.starts_with("edit ") => {
                let field = value.trim_start_matches("edit ").trim();
                let Some(step) = editable(ui.session, field) else {
                    prompt::say(
                        ui.out,
                        "수정할 항목을 확인하세요. :help에서 항목 이름을 볼 수 있습니다.\n",
                    )?;
                    continue;
                };
                let answer = if field == "network" || field == "native" {
                    ask_path(&mut ui, home, field)?
                } else {
                    ask(&mut ui, home, step)?
                };
                if answer == Answer::Paused {
                    return finish(&mut ui, home_path, name, Finish::Paused);
                }
                // A source or dependency change can require new answers. Reuse
                // everything still valid, then return to the same review.
                current = steps(ui.session)
                    .into_iter()
                    .find(|step| !complete(ui.session, *step));
            }
            Input::Value(value) if value == "y" || value == "yes" => {
                // A repeated resume/approval of an unchanged result is a no-op.
                if ui.session.summary().preparation == "PREPARED_RECORDED"
                    && ui.session.check().is_ok()
                {
                    prompt::say(
                        ui.out,
                        "기존 준비 결과가 현재 자료와 일치합니다. 새 파일을 만들지 않았습니다.\n",
                    )?;
                    return finish(&mut ui, home_path, name, Finish::Saved);
                }
                match ui.session.prepare() {
                    Ok(_) => {
                        prompt::say(
                            ui.out,
                            "설정 파일 준비를 마쳤습니다. 설치·등록·초기화·시작은 수행하지 않았습니다.\n",
                        )?;
                        return finish(&mut ui, home_path, name, Finish::Saved);
                    }
                    Err(error) => {
                        ui.problem(error)?;
                        prompt::say(
                            ui.out,
                            "준비를 완료하지 못했습니다. details로 자료를 확인하고 edit <항목>으로 수정하세요. 다시 준비하려면 명시적으로 y를 입력하세요.\n",
                        )?;
                    }
                }
            }
            Input::Value(_) => {
                prompt::say(ui.out, "y, n, edit <항목>, details 중 하나를 입력하세요.\n")?
            }
        }
    }
}

fn finish(ui: &mut Wizard<'_>, home: &Path, name: &str, finish: Finish) -> Result<Finish> {
    let action =
        if finish == Finish::Saved && ui.session.summary().preparation == "PREPARED_RECORDED" {
            "show"
        } else {
            "resume"
        };
    prompt::say(
        ui.out,
        &format!(
            "다음: bxdl setup {action} {name} --home {}\n",
            shell_quote(home)
        ),
    )?;
    Ok(finish)
}

fn editable(session: &Preparation, field: &str) -> Option<Step> {
    if let Some(index) = setup::FIELDS.iter().position(|f| f.name == field) {
        return Some(Step::Product(index));
    }
    match field {
        "archive" => Some(Step::Archive),
        "trust" => Some(Step::Trust),
        "lock" => Some(Step::Lock),
        "network" | "native" | "source" => Some(Step::Source),
        "node" if session.inputs().network.is_some() => Some(Step::Node),
        "destination" => Some(Step::Destination),
        "instance" => Some(Step::Instance),
        _ if session.inputs().network.is_some() => RUNTIME_FIELDS
            .iter()
            .position(|name| *name == field)
            .map(Step::Policy),
        _ => None,
    }
}

fn review_help(ui: &mut Wizard<'_>) -> Result<()> {
    ui.help("y/yes는 설정 파일 준비만 승인합니다. Enter/n은 답을 저장한 채 종료합니다. 실제 설치·엔진 실행은 수행하지 않습니다.")?;
    prompt::say(ui.out, "수정 예: edit httpPort\n")?;
    for field in setup::FIELDS {
        prompt::say(ui.out, &format!("  {} — {}\n", field.name, field.label))?;
    }
    prompt::say(
        ui.out,
        "자료/경로: archive trust lock source network native node destination instance\n",
    )?;
    if ui.session.inputs().network.is_some() {
        prompt::say(ui.out, &format!("정책: {}\n", RUNTIME_FIELDS.join(" ")))?;
    }
    Ok(())
}

fn ask(ui: &mut Wizard<'_>, home: &Home, step: Step) -> Result<Answer> {
    match step {
        Step::Archive => ask_path(ui, home, "archive"),
        Step::Trust => ask_trust(ui, home),
        Step::Lock => ask_path(ui, home, "lock"),
        Step::Source => ask_source(ui, home),
        Step::Product(index) => ask_product(ui, home, index),
        Step::Node => ask_node(ui, home),
        Step::Policy(index) => ask_policy(ui, home, index),
        Step::Destination => ask_path(ui, home, "destination"),
        Step::Instance => ask_path(ui, home, "instance"),
    }
}

fn ask_product(ui: &mut Wizard<'_>, home: &Home, index: usize) -> Result<Answer> {
    let field = &setup::FIELDS[index];
    loop {
        let saved = ui.session.draft().has_answer(field.name);
        let default = ui.session.draft().default_for(index);
        prompt::say(
            ui.out,
            &format!("\n{} ({})\n{}\n", field.label, field.name, field.hint),
        )?;
        if saved {
            let value = ui.session.review_rows()[index].1.clone();
            prompt::say(
                ui.out,
                &format!(
                    "현재값: {}\nEnter 저장값 유지 > ",
                    prompt::safe_default(&value)
                ),
            )?;
        } else if let Some(value) = &default {
            prompt::say(
                ui.out,
                &format!("기본값 [{}] > ", prompt::safe_default(value)),
            )?;
        } else {
            prompt::say(ui.out, "값 입력 > ")?;
        }
        let value = match read_answer(ui, home, field.hint)? {
            Input::Cancel | Input::Eof => return Ok(Answer::Paused),
            Input::Back => return Ok(Answer::Back),
            Input::Value(value) if value.is_empty() && saved => return Ok(Answer::Next),
            Input::Value(value) if value.is_empty() => match default {
                Some(value) => value,
                None => {
                    prompt::say(ui.out, "필수 항목입니다. 값을 입력하세요.\n")?;
                    continue;
                }
            },
            Input::Value(value) => value,
            Input::Help => continue,
        };
        match ui.session.set_field(field.name, &value) {
            Ok(()) => return Ok(Answer::Next),
            Err(error) => ui.problem(error)?,
        }
    }
}

fn read_answer(ui: &mut Wizard<'_>, home: &Home, help: &str) -> Result<Input> {
    let answer = ui.read()?;
    home.recheck()?;
    ui.session.recheck()?;
    if answer == Input::Help {
        ui.help(help)?;
    }
    Ok(answer)
}

fn ask_path(ui: &mut Wizard<'_>, home: &Home, field: &str) -> Result<Answer> {
    loop {
        let inputs = ui.session.inputs();
        let (label, existing) = match field {
            "archive" => ("패키지 archive 파일", &inputs.archive),
            "public_key" => ("독립적으로 신뢰한 공개키 파일", &inputs.public_key),
            "lock" => ("엔진 버전 고정 파일 (engine lock)", &inputs.lock),
            "network" => ("공통 network.json 파일", &inputs.network),
            "native" => ("기존 native 설정 파일", &inputs.native),
            "destination" => ("패키지 설치 예정 위치", &inputs.destination),
            "instance" => ("인스턴스 제어 예정 위치", &inputs.instance),
            _ => return Err(state_error()),
        };
        let saved = existing.is_some();
        prompt::say(
            ui.out,
            &format!(
                "\n{label}\n{} > ",
                if saved {
                    "Enter 저장값 유지"
                } else {
                    "파일/폴더 경로"
                }
            ),
        )?;
        let value = match read_answer(
            ui,
            home,
            "경로만 입력하세요. 상대경로는 준비 세션을 만든 위치 기준입니다. 기존 입력/데이터와 출력 경로는 겹칠 수 없습니다.",
        )? {
            Input::Cancel | Input::Eof => return Ok(Answer::Paused),
            Input::Back => return Ok(Answer::Back),
            Input::Value(value) if value.is_empty() && saved => {
                return Ok(Answer::Next);
            }
            Input::Value(value) if value.is_empty() => {
                prompt::say(ui.out, "필수 경로를 입력하세요.\n")?;
                continue;
            }
            Input::Value(value) => value,
            Input::Help => continue,
        };
        let path = match ui.session.resolve(&value) {
            Ok(path) => path,
            Err(error) => {
                ui.problem(error)?;
                continue;
            }
        };
        let mut inputs = ui.session.inputs().clone();
        match field {
            "archive" => inputs.archive = Some(path),
            "public_key" => {
                inputs.public_key = Some(path);
                inputs.unsigned = Some(false);
            }
            "lock" => inputs.lock = Some(path),
            "network" => {
                inputs.network = Some(path);
                inputs.native = None;
            }
            "native" => {
                inputs.native = Some(path);
                inputs.network = None;
                inputs.runtime_options = None;
                inputs.runtime_answers.clear();
            }
            "destination" => inputs.destination = Some(path),
            "instance" => inputs.instance = Some(path),
            _ => return Err(state_error()),
        }
        match ui.session.set_inputs(inputs) {
            Ok(()) => return Ok(Answer::Next),
            Err(error) => ui.problem(error)?,
        }
    }
}

fn ask_trust(ui: &mut Wizard<'_>, home: &Home) -> Result<Answer> {
    loop {
        prompt::say(
            ui.out,
            "\n패키지 검증 방법을 선택하세요.\n1  독립적으로 신뢰한 공개키\n2  서명 없는 개발 자료 (명시 승인 필요)\nEnter 저장값 유지 / :back 이전 질문 > ",
        )?;
        match read_answer(
            ui,
            home,
            "공개키는 패키지와 별도로 신뢰한 경로에서 받으세요. 서명 없는 자료는 개발 용도에 한해 명시적으로 허용합니다.",
        )? {
            Input::Cancel | Input::Eof => return Ok(Answer::Paused),
            Input::Back => return Ok(Answer::Back),
            Input::Help => {}
            Input::Value(value) if value.is_empty() && complete(ui.session, Step::Trust) => {
                return Ok(Answer::Next);
            }
            Input::Value(value) if value == "1" => match ask_path(ui, home, "public_key")? {
                Answer::Back => {}
                answer => return Ok(answer),
            },
            Input::Value(value) if value == "2" => {
                let approval = prompt::confirm(
                    ui.input,
                    ui.out,
                    Policy::Install,
                    "서명 없는 개발 자료를 이 준비 세션에서 허용할까요?",
                )?;
                home.recheck()?;
                ui.session.recheck()?;
                match approval {
                    Confirmation::Paused => return Ok(Answer::Paused),
                    Confirmation::No => continue,
                    Confirmation::Yes => {}
                }
                let mut inputs = ui.session.inputs().clone();
                inputs.public_key = None;
                inputs.unsigned = Some(true);
                match ui.session.set_inputs(inputs) {
                    Ok(()) => return Ok(Answer::Next),
                    Err(error) => ui.problem(error)?,
                }
            }
            Input::Value(_) => prompt::say(ui.out, "1 또는 2를 선택하세요.\n")?,
        }
    }
}

fn ask_source(ui: &mut Wizard<'_>, home: &Home) -> Result<Answer> {
    loop {
        prompt::say(
            ui.out,
            "\n네트워크 설정 자료를 선택하세요.\n1  공통 공개 목록에서 내 노드 선택\n2  기존 native 설정 사용\nEnter 저장값 유지 / :back 이전 질문 > ",
        )?;
        let field = match read_answer(
            ui,
            home,
            "운영자가 제공한 network.json 또는 기존 native 설정을 선택하세요. 이 단계에서 네트워크나 키를 생성하지 않습니다.",
        )? {
            Input::Cancel | Input::Eof => return Ok(Answer::Paused),
            Input::Back => return Ok(Answer::Back),
            Input::Help => continue,
            Input::Value(value) if value.is_empty() && complete(ui.session, Step::Source) => {
                return Ok(Answer::Next);
            }
            Input::Value(value) if value == "1" => "network",
            Input::Value(value) if value == "2" => "native",
            Input::Value(_) => {
                prompt::say(ui.out, "1 또는 2를 선택하세요.\n")?;
                continue;
            }
        };
        match ask_path(ui, home, field)? {
            Answer::Back => {}
            answer => return Ok(answer),
        }
    }
}

fn ask_node(ui: &mut Wizard<'_>, home: &Home) -> Result<Answer> {
    let choices = match ui.session.node_choices() {
        Ok(choices) if !choices.is_empty() => choices,
        Ok(_) => {
            prompt::say(
                ui.out,
                "선택할 노드가 없습니다. 체인·네트워크 자료를 확인하세요.\n",
            )?;
            return Ok(Answer::Back);
        }
        Err(error) => {
            ui.problem(error)?;
            prompt::say(
                ui.out,
                "체인·네트워크 자료를 확인하세요. :back으로 앞 자료를 수정할 수 있습니다.\n",
            )?;
            return Ok(Answer::Back);
        }
    };
    loop {
        prompt::say(ui.out, "\n이 장비에 배정된 노드를 선택하세요.\n")?;
        for (index, node) in choices.iter().enumerate() {
            prompt::say(
                ui.out,
                &format!(
                    "{}  node {} · validator {} · {}:{}\n",
                    index + 1,
                    prompt::safe_default(&node.node_id),
                    prompt::safe_default(&node.validator_id),
                    prompt::safe_default(&node.host),
                    node.port
                ),
            )?;
        }
        prompt::say(ui.out, "내 노드 번호 (Enter 일치하는 저장값 유지) > ")?;
        match read_answer(
            ui,
            home,
            "배정받은 공개 노드를 고르면 nodeId와 P2P 주소/포트를 함께 저장합니다. 기존 키는 이동하지 않습니다.",
        )? {
            Input::Cancel | Input::Eof => return Ok(Answer::Paused),
            Input::Back => return Ok(Answer::Back),
            Input::Help => {}
            Input::Value(value) if value.is_empty() && node_matches(ui.session) => {
                return Ok(Answer::Next);
            }
            Input::Value(value) => {
                let Some(index) = value
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .filter(|i| *i < choices.len())
                else {
                    prompt::say(ui.out, "표시된 노드 번호를 입력하세요.\n")?;
                    continue;
                };
                match ui.session.select_node(&choices[index].node_id) {
                    Ok(()) => return Ok(Answer::Next),
                    Err(error) => ui.problem(error)?,
                }
            }
        }
    }
}

fn ask_policy(ui: &mut Wizard<'_>, home: &Home, index: usize) -> Result<Answer> {
    loop {
        let inputs = ui.session.inputs();
        let options = inputs.runtime_options.clone().unwrap_or_default();
        let default = inputs
            .runtime_answers
            .get(RUNTIME_FIELDS[index])
            .cloned()
            .unwrap_or_else(|| option_default(&options, index).to_owned());
        prompt::say(
            ui.out,
            &format!("\n{} [{}] > ", POLICY_LABELS[index], default),
        )?;
        let value = match read_answer(
            ui,
            home,
            "각 정책 답변을 저장합니다. 합의 참여 설정도 나중에 노드를 명시적으로 시작할 때 적용됩니다.",
        )? {
            Input::Cancel | Input::Eof => return Ok(Answer::Paused),
            Input::Back => return Ok(Answer::Back),
            Input::Help => continue,
            Input::Value(value) if value.is_empty() => default,
            Input::Value(value) => value,
        };
        match ui.session.set_runtime_answer(index, &value) {
            Ok(()) => return Ok(Answer::Next),
            Err(error) => ui.problem(error)?,
        }
    }
}
