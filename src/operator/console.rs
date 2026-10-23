//! Read-only target navigation and explicitly approved advisory catalog writes.
//! Preparation shares a controller with setup new/resume; operational actions remain explicit.
use super::{
    catalog::{self, Catalog, Entry},
    prompt::{self, Confirmation, Input, Policy},
    target::{self, CliMatch, Kind, Snapshot},
};
use crate::error::{BxdlError, Result};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Finish {
    Closed,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Back,
    Paused,
}

pub(crate) fn interact(
    home: &Path,
    instance: Option<&Path>,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Finish> {
    let mut console = Console {
        home: home.to_owned(),
        input,
        out,
    };
    console.say("BXDL console · 홈·목록·설치 자료 준비\n설치·초기화·시작·중지는 기존 명령에서 수행합니다. 이 화면은 자동 실행하지 않습니다.\n:help 도움말 / :back 이전 화면 / :cancel·EOF 중단 / 승인 Enter=거절\n")?;
    if let Some(path) = instance {
        let snapshot = target::inspect(Kind::Instance, path)?;
        if console.direct(snapshot)? == Flow::Paused {
            return Ok(Finish::Paused);
        }
    }
    loop {
        console.say("\nBXDL · 홈\n")?;
        match Catalog::open(&console.home) {
            Ok(Some(catalog)) => console.say(&format!(
                "목록 {}개 · 저장된 기록이며 현재 노드 상태는 미조회\n",
                catalog.entries().len()
            ))?,
            Ok(None) => {
                console.say("저장된 목록이 없습니다. 아직 관리 폴더를 만들지 않았습니다.\n")?
            }
            Err(error) => {
                console.problem(&error)?;
                console.say("목록을 자동 복구하지 않습니다. 4번의 명시 경로 열기는 계속 사용할 수 있습니다.\n과거 setup만 있던 폴더는 자동 전환하지 않습니다. 필요하면 새 --home을 지정하세요.\n")?;
            }
        }
        console.say("1  새 설치 준비·준비 세션 재개\n2  설치·설정 세션 목록\n3  내 노드 목록\n4  기존 경로 열기\n5  도움말·관리 위치\n0  종료\n선택 [0] > ")?;
        let flow = match console.read()? {
            Input::Help => {
                console.help("목록 조회는 파일 생성·현재 서비스 조회를 하지 않습니다. 기존 자료는 4번으로 명시 연결합니다.")?;
                Flow::Back
            }
            Input::Cancel | Input::Eof => return Ok(Finish::Paused),
            Input::Back => return Ok(Finish::Closed),
            Input::Value(value) => match value.as_str() {
                "" | "0" => return Ok(Finish::Closed),
                "1" => match super::install_flow::menu(&console.home, console.input, console.out) {
                    Ok(super::install_flow::Finish::Paused) => Flow::Paused,
                    Ok(super::install_flow::Finish::Saved) => Flow::Back,
                    Err(error)
                        if matches!(
                            error.code.as_str(),
                            "SETUP_INPUT_FAILED" | "SETUP_OUTPUT_FAILED"
                        ) =>
                    {
                        return Err(error);
                    }
                    Err(error) => {
                        console.problem(&error)?;
                        Flow::Back
                    }
                },
                "2" => console.list(false)?,
                "3" => console.list(true)?,
                "4" => console.open_path()?,
                "5" => console.about()?,
                _ => {
                    console.say("표시된 번호를 입력하세요.\n")?;
                    Flow::Back
                }
            },
        };
        if flow == Flow::Paused {
            return Ok(Finish::Paused);
        }
    }
}

struct Console<'a> {
    home: PathBuf,
    input: &'a mut dyn BufRead,
    out: &'a mut dyn Write,
}

impl Console<'_> {
    fn say(&mut self, text: &str) -> Result<()> {
        prompt::say(self.out, text)
    }
    fn read(&mut self) -> Result<Input> {
        prompt::read(self.input, Policy::Install)
    }
    fn help(&mut self, text: &str) -> Result<()> {
        prompt::help(self.out, text)
    }
    fn confirm(&mut self, text: &str) -> Result<Confirmation> {
        prompt::confirm(self.input, self.out, Policy::Install, text)
    }
    fn problem(&mut self, error: &BxdlError) -> Result<()> {
        // Never relay file contents, engine reasons or credential paths from a failed read.
        let code = if error.code.len() <= 80
            && error
                .code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            error.code.as_str()
        } else {
            "CONSOLE_READ_FAILED"
        };
        let hint = if code.contains("WRITE_CONFLICT") || code.contains("OVERLAP") {
            "관리 위치가 기존 노드·세션·데이터·자료와 겹치거나 안전성을 확인하지 못했습니다. 별도의 --home을 지정하세요."
        } else if code.contains("BUSY") || code.contains("CONFLICT") {
            "다른 작업이 사용 중이거나 목록이 바뀌었습니다. 해당 작업이 끝난 뒤 다시 여세요."
        } else if code.contains("CHANGED") || code.contains("IDENTITY") {
            "저장한 대상과 현재 폴더가 다릅니다. 기존 기록을 덮어 연결하지 않습니다."
        } else {
            "자료의 형식·위치·권한 또는 저장 조건을 확인할 수 없습니다. 기존 자료는 자동 수정하지 않습니다."
        };
        self.say(&format!("{hint} [{code}]\n"))
    }
    fn pause(&mut self) -> Result<Flow> {
        loop {
            self.say("돌아가기 [Enter] > ")?;
            match self.read()? {
                Input::Cancel | Input::Eof => return Ok(Flow::Paused),
                Input::Help => self.help("Enter 또는 :back으로 이전 화면에 돌아갑니다.")?,
                _ => return Ok(Flow::Back),
            }
        }
    }
    fn about(&mut self) -> Result<Flow> {
        self.say("\n관리 위치 (명시 상세)\n")?;
        self.say(&format!(
            "{}\n",
            prompt::safe_default(&self.home.to_string_lossy())
        ))?;
        self.say("목록 저장을 승인할 때만 필요한 관리 폴더·목록을 만듭니다. 기존 폴더를 자동 채택하거나 권한을 바꾸지 않습니다.\n목록은 탐색용이며 실행 권한이나 현재 건강 상태의 근거가 아닙니다.\n노드·설치 세션 경로는 기본 목록에 표시하지 않고 상세에서만 보여줍니다.\n홈 1번은 이름을 붙인 자료 준비와 재개입니다. 실제 설치·반복 운용·지원 보고서는 다음 단계에서 연결합니다.\nMac LaunchAgent는 터미널 종료와 별개이며 사용자 로그아웃 시 종료됩니다.\nconsole 종료는 노드 중지 요청이 아닙니다.\n")?;
        self.pause()
    }
    fn open_path(&mut self) -> Result<Flow> {
        let kind = loop {
            self.say("\n기존 자료 종류\n1  등록된 노드 control\n2  설치 세션 (setup --install)\n3  제품 설정 초안 (setup)\n4  네트워크 준비 세션 (setup --network)\n5  이름 있는 설치 준비 세션 (setup new)\n0  홈으로\n선택 [0] > ")?;
            match self.read()? {
                Input::Help => self.help("직접 지정한 기존 BXDL 폴더만 엽니다. 디스크 검색·재등록·DB 채택·형식 변환은 하지 않습니다.")?,
                Input::Cancel | Input::Eof => return Ok(Flow::Paused),
                Input::Back => return Ok(Flow::Back),
                Input::Value(value) => match value.as_str() {
                    "" | "0" => return Ok(Flow::Back),
                    "1" => break Kind::Instance,
                    "2" => break Kind::InstallSession,
                    "3" => break Kind::DraftSession,
                    "4" => break Kind::NetworkSession,
                    "5" => break Kind::PreparationSession,
                    _ => self.say("표시된 번호를 입력하세요.\n")?,
                }
            }
        };
        loop {
            self.say("기존 폴더 경로 (Enter·:back 홈으로) > ")?;
            match self.read()? {
                Input::Help => self.help("control 또는 해당 setup 세션 폴더를 입력합니다. 키·비밀번호 파일이나 data 폴더를 입력하지 마세요. 상대경로는 현재 작업 폴더 기준입니다.")?,
                Input::Back => return Ok(Flow::Back),
                Input::Cancel | Input::Eof => return Ok(Flow::Paused),
                Input::Value(value) if value.is_empty() => return Ok(Flow::Back),
                Input::Value(value) => match target::inspect(kind, Path::new(&value)) {
                    Ok(snapshot) => return self.direct(snapshot),
                    Err(error) => self.problem(&error)?,
                },
            }
        }
    }
    fn direct(&mut self, snapshot: Snapshot) -> Result<Flow> {
        let catalog = match Catalog::open(&self.home) {
            Ok(value) => value,
            Err(error) => {
                self.problem(&error)?;
                None
            }
        };
        let entry = catalog
            .as_ref()
            .and_then(|c| {
                c.entries().iter().find(|entry| {
                    entry.target.kind == snapshot.kind
                        && entry.target.path == snapshot.path
                        && entry.target.identity == snapshot.identity
                })
            })
            .cloned();
        self.detail(snapshot, entry, catalog, true)
    }
    fn list(&mut self, nodes: bool) -> Result<Flow> {
        loop {
            let catalog = match Catalog::open(&self.home) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    self.say("저장된 목록이 없습니다. 홈 4번에서 기존 경로를 열 수 있습니다.\n")?;
                    return Ok(Flow::Back);
                }
                Err(error) => {
                    self.problem(&error)?;
                    self.say("홈 4번으로 명시 경로를 열 수 있습니다.\n")?;
                    return Ok(Flow::Back);
                }
            };
            let entries: Vec<Entry> = catalog
                .entries()
                .iter()
                .filter(|e| (e.target.kind == Kind::Instance) == nodes)
                .cloned()
                .collect();
            self.say(if nodes {
                "\n내 노드 목록\n"
            } else {
                "\n설치·설정 세션 목록\n"
            })?;
            self.say(
                "아래는 목록에 저장한 기록입니다. 현재 서비스 상태·입력 일치는 미조회입니다.\n",
            )?;
            for (i, entry) in entries.iter().enumerate() {
                self.say(&format!(
                    "{}  {} [{}]\n   {} · {} · 기록 {}\n",
                    i + 1,
                    entry.label,
                    &entry.id[..8],
                    entry.target.kind.label(),
                    entry.target.state.label(),
                    entry.recorded_at
                ))?;
            }
            if entries.is_empty() {
                self.say("아직 이 종류의 항목이 없습니다. 홈 4번에서 명시 연결하세요.\n")?;
            }
            self.say("0  홈으로\n선택 [0] > ")?;
            match self.read()? {
                Input::Help => self.help("번호로 선택한 대상만 검증한 뒤 저장 기록을 읽습니다. 노드·세션을 자동 실행하거나 현재 서비스를 조회하지 않습니다.")?,
                Input::Cancel | Input::Eof => return Ok(Flow::Paused),
                Input::Back => return Ok(Flow::Back),
                Input::Value(value) if value.is_empty() || value == "0" => return Ok(Flow::Back),
                Input::Value(value) => {
                    if let Some(entry) = value.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|n| entries.get(n)).cloned() {
                        let (snapshot, valid) = match target::recheck(&entry.target) {
                            Ok(snapshot) => (snapshot, true),
                            Err(error) => { self.problem(&error)?; (entry.target.clone(), false) }
                        };
                        if self.detail(snapshot, Some(entry), Some(catalog), valid)? == Flow::Paused { return Ok(Flow::Paused); }
                    } else { self.say("목록의 번호를 입력하세요.\n")?; }
                }
            }
        }
    }
    fn detail(
        &mut self,
        mut snapshot: Snapshot,
        mut entry: Option<Entry>,
        mut catalog: Option<Catalog>,
        mut valid: bool,
    ) -> Result<Flow> {
        loop {
            let title = entry.as_ref().map_or_else(
                || snapshot.name.clone(),
                |entry| format!("{} [{}]", entry.label, &entry.id[..8]),
            );
            self.say(&format!("\n{} · {}\n", title, snapshot.kind.label()))?;
            self.say(if valid {
                "저장 자료: 이번 화면에서 읽음 · 현재 서비스 상태는 미조회\n"
            } else {
                "저장한 대상 재검증 실패 · 아래는 과거 목록 기록이며 작업 근거가 아닙니다.\n"
            })?;
            self.say(&format!(
                "저장 단계: {}\n지원 형식: schema {}\n",
                snapshot.state.label(),
                snapshot.schema_version
            ))?;
            if snapshot.kind == Kind::Instance {
                if valid {
                    self.say(&format!("현재 CLI와 설치 CLI: {}\n", snapshot.cli.label()))?;
                    if snapshot.cli == CliMatch::Mismatch {
                        self.say("이 CLI로 기존 설치를 시작할 수 있다고 간주하지 않습니다. 설치본을 덮어쓰지 마세요.\n")?;
                    }
                } else {
                    self.say("CLI 일치: 미조회\n")?;
                }
                self.say("합의·정족수·runtime READY: 미확인\n")?;
            }
            self.say("설치·초기화·시작·중지는 이 화면에서 제공하지 않습니다.\n1  등록·세션 저장 기록 다시 읽기\n")?;
            self.say(if entry.is_some() {
                "2  목록의 기록 갱신\n"
            } else {
                "2  내 목록에 저장\n"
            })?;
            self.say("3  상세 위치·기존 명령\n")?;
            if entry.is_some() {
                self.say("4  목록에서 숨기기 (중지·삭제 아님)\n")?;
            }
            self.say("0  돌아가기\n선택 [0] > ")?;
            match self.read()? {
                Input::Help => self.help("저장 단계는 현재 서비스 상태가 아닙니다. 기록 다시 읽기는 엔진·launchd·HTTP를 호출하지 않습니다. 목록 저장/갱신/숨기기는 별도 승인합니다.")?,
                Input::Cancel | Input::Eof => return Ok(Flow::Paused),
                Input::Back => return Ok(Flow::Back),
                Input::Value(value) => match value.as_str() {
                    "" | "0" => return Ok(Flow::Back),
                    "1" => match target::recheck(&snapshot) {
                        Ok(current) => { snapshot = current; valid = true; self.say("등록·세션 저장 기록을 다시 읽었습니다. 목록 파일은 변경하지 않았습니다.\n")?; }
                        Err(error) => { valid = false; self.problem(&error)?; }
                    },
                    "2" => {
                        if !valid { self.say("대상을 확인하지 못해 저장·갱신을 차단했습니다. 기존 명령/위치를 확인하거나 목록에서만 숨길 수 있습니다.\n")?; continue; }
                        let label = if let Some(saved) = &entry { saved.label.clone() } else {
                            loop {
                                self.say(&format!("목록 표시 이름 [Enter={}] > ", snapshot.name))?;
                                match self.read()? {
                                    Input::Help => self.help("목록에서 구분할 별칭입니다. 노드 identity나 실제 설정은 바꾸지 않습니다. 비밀번호·키 경로를 넣지 마세요.")?,
                                    Input::Back => break String::new(),
                                    Input::Cancel | Input::Eof => return Ok(Flow::Paused),
                                    Input::Value(value) => {
                                        let label = if value.is_empty() { snapshot.name.clone() } else { value };
                                        if label.chars().count() > 80 { self.say("표시 이름은 80자 이내로 입력하세요.\n")?; continue; }
                                        break label;
                                    }
                                }
                            }
                        };
                        if label.is_empty() { continue; }
                        match self.confirm(&format!("{title}: 위치·식별 정보·공개 저장 단계를 내 목록에 저장할까요?\n필요하면 BXDL 관리 폴더와 목록만 만듭니다. 노드·설치 자료는 변경하지 않습니다."))? {
                            Confirmation::Paused => return Ok(Flow::Paused),
                            Confirmation::No => { self.say("목록을 변경하지 않았습니다.\n")?; continue; }
                            Confirmation::Yes => {}
                        }
                        match self.save(&snapshot, entry.as_ref(), &label, &mut catalog) {
                            Ok(saved) => { snapshot = saved.target.clone(); entry = Some(saved); self.say("목록에 저장했습니다. 실제 노드·설치 상태는 변경하지 않았습니다.\n")?; }
                            Err(error) => { self.problem(&error)?; self.say("목록 저장을 완료했다고 확인하지 못했습니다. 기존 노드·설치 자료는 그대로이며 홈에서 목록을 다시 읽거나 상세의 기존 명령을 사용하세요. 설치·등록을 반복하지 마세요.\n")?; }
                        }
                    }
                    "3" => {
                        self.say("\n명시 위치 (자동 실행하지 않음)\n")?;
                        self.say(&format!("{}\n", prompt::safe_default(&snapshot.path.to_string_lossy())))?;
                        if valid { self.say(&format!("기존 명령:\n{}\n", fallback(&snapshot)))?; }
                        else { self.say("대상이 바뀌었거나 검증되지 않았습니다. 이 경로로 명령을 실행하기 전에 실제 대상을 확인하세요.\n")?; }
                        self.say("완료된 설치 session·제품/native/chain·키 참조는 운용 자료이므로 보존하세요.\n")?;
                        if self.pause()? == Flow::Paused { return Ok(Flow::Paused); }
                    }
                    "4" if entry.is_some() => {
                        match self.confirm(&format!("{title}: 목록에서만 숨길까요? 노드를 중지하거나 설치·세션·데이터를 삭제하지 않습니다."))? {
                            Confirmation::Paused => return Ok(Flow::Paused),
                            Confirmation::No => continue,
                            Confirmation::Yes => {}
                        }
                        if let (Some(saved), Some(catalog)) = (&entry, &mut catalog) {
                            match catalog.remove(&saved.id) {
                                Ok(()) => { self.say("목록에서 숨겼습니다. 기존 자료와 실행 상태는 그대로입니다.\n")?; return Ok(Flow::Back); }
                                Err(error) => self.problem(&error)?,
                            }
                        }
                    }
                    _ => self.say("표시된 번호를 입력하세요.\n")?,
                },
            }
        }
    }
    fn save(
        &mut self,
        expected: &Snapshot,
        entry: Option<&Entry>,
        label: &str,
        catalog: &mut Option<Catalog>,
    ) -> Result<Entry> {
        let current = target::recheck(expected)?;
        catalog::check_target_location(&self.home, &current)?;
        if catalog.is_none() {
            *catalog = Some(match Catalog::open(&self.home)? {
                Some(value) => value,
                None => Catalog::create(&self.home)?,
            });
        }
        let catalog = catalog.as_mut().expect("catalog loaded above");
        let id = if let Some(entry) = entry {
            catalog.refresh(&entry.id, current)?;
            entry.id.clone()
        } else {
            catalog.add(label, current)?
        };
        catalog
            .entries()
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| BxdlError::new("CONSOLE_CATALOG_CHANGED", "목록을 다시 확인하세요."))
    }
}

fn fallback(snapshot: &Snapshot) -> String {
    // Display only. Shell quoting prevents a copied public root path from being code.
    let path = format!(
        "'{}'",
        snapshot.path.to_string_lossy().replace('\'', "'\\''")
    );
    match snapshot.kind {
        Kind::Instance => format!("bxdl instance show --instance {path}"),
        Kind::InstallSession => format!("bxdl setup --install --workspace {path} --resume"),
        Kind::DraftSession => format!("bxdl setup --workspace {path} --resume"),
        Kind::NetworkSession => format!("bxdl setup --network --workspace {path} --resume"),
        Kind::PreparationSession => preparation_command(&snapshot.path).unwrap_or_else(|| {
            "이름 있는 관리 위치에서 setup resume <이름> --home <관리-루트>를 사용하세요.".into()
        }),
    }
}

fn preparation_command(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let sessions = path.parent()?;
    if !super::install_flow::valid_name(name) || sessions.file_name()? != "sessions" {
        return None;
    }
    let home = sessions.parent()?;
    Some(format!(
        "bxdl setup resume {name} --home '{}'",
        home.to_string_lossy().replace('\'', "'\\''")
    ))
}
