//! User GUI LaunchAgent adapter. No login-directory installation or automatic
//! restart. `launchctl print` is an OS-specific observation, not a stable API:
//! an unrecognised or ambiguous representation fails closed as UNKNOWN.
//!
//! launchd may force termination on logout/shutdown after ExitTimeOut. A launchd
//! exit observation is never proof of NIGO storage/consensus closure. The gate
//! must clear its inherited environment before execing the pinned Java process.
use crate::{
    error::{BxdlError, Result},
    setup::store,
};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, MetadataExt, OpenOptions, OpenOptionsExt},
};
use rustix::fs::OFlags;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{ErrorKind, Read},
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 131_072;
pub(super) const EXIT_TIMEOUT_SECONDS: u32 = 60;

#[derive(Clone, Debug)]
pub(super) struct Job {
    pub uid: u32,
    pub label: String,
    pub service_target: String,
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
    pub plist: PathBuf,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Observation {
    GuiUnavailable,
    NotLoaded,
    Loaded(JobState),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct JobState {
    pub pid: Option<u32>,
    pub state: String,
    pub last_exit_code: Option<i32>,
    pub last_terminating_signal: Option<u32>,
}

impl JobState {
    pub fn is_stopped(&self) -> bool {
        self.pid.is_none()
            && matches!(self.state.as_str(), "not running" | "exited")
            && (self.last_exit_code.is_some() || self.last_terminating_signal.is_some())
    }
}

pub(super) fn current_uid() -> Result<u32> {
    let uid = rustix::process::getuid().as_raw();
    if uid == 0 || rustix::process::geteuid().as_raw() != uid {
        return Err(error(
            "SERVICE_USER_INVALID",
            "일반 사용자 GUI 세션이 필요합니다.",
        ));
    }
    Ok(uid)
}

impl Job {
    /// Pure specification construction; service operations enforce the host/UID.
    /// The snapshot must be an immediate child of the private attempt directory.
    pub fn new(
        uid: u32,
        instance: &Path,
        attempt: &str,
        program: &Path,
        working_directory: &Path,
    ) -> Result<Self> {
        if uid == 0
            || attempt.is_empty()
            || attempt.len() > 80
            || !attempt
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(invalid());
        }
        let instance = path_text(instance)?;
        let program_text = path_text(program)?;
        path_text(working_directory)?;
        if program.parent() != Some(working_directory) {
            return Err(invalid());
        }
        let digest = hex::encode(Sha256::digest(instance.as_bytes()));
        let label = format!("com.bxdl.instance.{}.{}", &digest[..24], attempt);
        let arguments = vec![
            program_text.into(),
            "service-run".into(),
            "--instance".into(),
            instance.into(),
            "--attempt".into(),
            attempt.into(),
            "--json".into(),
        ];
        let job = Self {
            uid,
            service_target: format!("gui/{uid}/{label}"),
            plist: working_directory.join(format!("{label}.plist")),
            stdout: working_directory.join("service.stdout.json"),
            stderr: working_directory.join("service.stderr.private"),
            label,
            program: program.into(),
            arguments,
            working_directory: working_directory.into(),
        };
        if job.program == job.stdout || job.program == job.stderr || job.program == job.plist {
            return Err(invalid());
        }
        Ok(job)
    }

    /// Prepare only attempt-owned files. Partial preparation is preserved and
    /// cannot be retried by overwriting files. No launchd command is invoked.
    pub fn prepare(&self) -> Result<()> {
        let root = open_private_directory(&self.working_directory, self.uid)?;
        self.check_program(&root)?;
        // Refuse all pre-existing outputs before publishing the first file.
        for path in [&self.stdout, &self.stderr, &self.plist] {
            match root.symlink_metadata(path.file_name().ok_or_else(invalid)?) {
                Err(e) if e.kind() == ErrorKind::NotFound => (),
                _ => {
                    return Err(error(
                        "SERVICE_FILE_EXISTS",
                        "서비스 시도 파일이 이미 있거나 안전하지 않습니다.",
                    ));
                }
            }
        }
        store::write_new(&self.stdout, b"").map_err(|_| unsafe_files())?;
        store::write_new(&self.stderr, b"").map_err(|_| unsafe_files())?;
        store::write_new(&self.plist, self.plist_bytes()?.as_bytes())
            .map_err(|_| unsafe_files())?;
        self.recheck_files()
    }

    pub fn inspect(&self, timeout: Duration) -> Result<Observation> {
        self.check_host()?;
        let output = run_launchctl(&["print", &self.service_target], timeout)?;
        parse_observation(self, &output)
    }

    /// Success means bootstrap was accepted, not that the engine is ready.
    pub fn bootstrap(&self, timeout: Duration) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        match deadline.call(|remaining| self.inspect(remaining))? {
            Observation::NotLoaded => (),
            Observation::GuiUnavailable => return Err(gui_unavailable()),
            Observation::Loaded(_) => {
                return Err(error(
                    "SERVICE_ALREADY_LOADED",
                    "서비스 시도가 이미 등록되어 있습니다.",
                ));
            }
        }
        self.recheck_files()?;
        let domain = format!("gui/{}", self.uid);
        let output = deadline.call(|remaining| {
            run_launchctl(&["bootstrap", &domain, path_text(&self.plist)?], remaining)
        })?;
        successful_mutation(output)
    }

    /// Signal only the exact service label after a fresh expected-PID binding.
    /// The caller's single-use durable attempt gate prevents a replacement run.
    pub fn terminate(&self, expected_pid: u32, timeout: Duration) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        if expected_pid == 0 {
            return Err(invalid());
        }
        match deadline.call(|remaining| self.inspect(remaining))? {
            Observation::Loaded(state)
                if state.pid == Some(expected_pid) && state.state == "running" => {}
            Observation::GuiUnavailable => return Err(gui_unavailable()),
            _ => return Err(unknown()),
        }
        successful_mutation(deadline.call(|remaining| {
            run_launchctl(&["kill", "SIGTERM", &self.service_target], remaining)
        })?)
    }

    /// Never use bootout as a stop operation: it can cause OS escalation.
    pub fn bootout_stopped(&self, timeout: Duration) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        match deadline.call(|remaining| self.inspect(remaining))? {
            Observation::NotLoaded => return Ok(()),
            Observation::Loaded(state) if state.is_stopped() => (),
            Observation::GuiUnavailable => return Err(gui_unavailable()),
            _ => {
                return Err(error(
                    "SERVICE_NOT_STOPPED",
                    "정지한 서비스만 등록 해제할 수 있습니다.",
                ));
            }
        }
        successful_mutation(
            deadline
                .call(|remaining| run_launchctl(&["bootout", &self.service_target], remaining))?,
        )?;
        match deadline.call(|remaining| self.inspect(remaining))? {
            Observation::NotLoaded => Ok(()),
            _ => Err(unknown()),
        }
    }

    fn check_host(&self) -> Result<()> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(error(
                "SERVICE_PLATFORM_UNSUPPORTED",
                "서비스 제어는 macOS arm64 사용자 GUI 세션에서만 지원합니다.",
            ));
        }
        if current_uid()? != self.uid {
            return Err(error(
                "SERVICE_USER_INVALID",
                "서비스 사용자와 현재 사용자가 다릅니다.",
            ));
        }
        Ok(())
    }

    fn check_program(&self, root: &Dir) -> Result<()> {
        let file = open_regular(root, &self.program, self.uid, &[0o500, 0o700])?;
        if file.metadata().map_err(|_| unsafe_files())?.len() == 0 {
            return Err(unsafe_files());
        }
        Ok(())
    }

    fn recheck_files(&self) -> Result<()> {
        let root = open_private_directory(&self.working_directory, self.uid)?;
        self.check_program(&root)?;
        for path in [&self.stdout, &self.stderr] {
            open_regular(&root, path, self.uid, &[0o600])?;
        }
        let mut file = open_regular(&root, &self.plist, self.uid, &[0o600])?;
        let before = file.metadata().map_err(|_| unsafe_files())?;
        let expected = self.plist_bytes()?;
        if before.len() != expected.len() as u64 {
            return Err(unsafe_files());
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unsafe_files())?;
        let after = file.metadata().map_err(|_| unsafe_files())?;
        if bytes != expected.as_bytes() || !same_file(&before, &after) {
            return Err(unsafe_files());
        }
        let visible = open_private_directory(&self.working_directory, self.uid)?;
        if !same_inode(
            &root.dir_metadata().map_err(|_| unsafe_files())?,
            &visible.dir_metadata().map_err(|_| unsafe_files())?,
        ) {
            return Err(unsafe_files());
        }
        Ok(())
    }

    fn plist_bytes(&self) -> Result<String> {
        let args = self
            .arguments
            .iter()
            .map(|s| format!("<string>{}</string>", xml_escape(s)))
            .collect::<String>();
        Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n\
             <key>Label</key><string>{}</string>\n\
             <key>Program</key><string>{}</string>\n\
             <key>ProgramArguments</key><array>{args}</array>\n\
             <key>WorkingDirectory</key><string>{}</string>\n\
             <key>StandardOutPath</key><string>{}</string>\n\
             <key>StandardErrorPath</key><string>{}</string>\n\
             <key>RunAtLoad</key><true/>\n<key>KeepAlive</key><false/>\n\
             <key>EnableTransactions</key><false/>\n<key>EnablePressuredExit</key><false/>\n\
             <key>ProcessType</key><string>Standard</string>\n<key>Umask</key><integer>63</integer>\n\
             <key>ExitTimeOut</key><integer>{EXIT_TIMEOUT_SECONDS}</integer>\n\
             <key>EnvironmentVariables</key><dict><key>LANG</key><string>C</string><key>LC_ALL</key><string>C</string><key>PATH</key><string></string></dict>\n\
             </dict></plist>\n",
            xml_escape(&self.label),
            xml_escape(path_text(&self.program)?),
            xml_escape(path_text(&self.working_directory)?),
            xml_escape(path_text(&self.stdout)?),
            xml_escape(path_text(&self.stderr)?),
        ))
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn path_text(path: &Path) -> Result<&str> {
    let text = path.to_str().ok_or_else(invalid)?;
    if !path.is_absolute()
        || text.len() > 4096
        || text.trim() != text
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}'))
    {
        return Err(invalid());
    }
    let mut reconstructed = PathBuf::from("/");
    for component in path.components().skip(1) {
        match component {
            Component::Normal(value) => reconstructed.push(value),
            _ => return Err(invalid()),
        }
    }
    if reconstructed.as_os_str() != path.as_os_str() || path == Path::new("/") {
        return Err(invalid());
    }
    Ok(text)
}

fn open_private_directory(path: &Path, uid: u32) -> Result<Dir> {
    path_text(path)?;
    let mut dir = Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| unsafe_files())?;
    for component in path.components().skip(1) {
        let Component::Normal(name) = component else {
            return Err(unsafe_files());
        };
        let before = dir.symlink_metadata(name).map_err(|_| unsafe_files())?;
        if !before.is_dir() || before.file_type().is_symlink() {
            return Err(unsafe_files());
        }
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags((OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32);
        let file = dir.open_with(name, &options).map_err(|_| unsafe_files())?;
        let opened = file.metadata().map_err(|_| unsafe_files())?;
        let visible = dir.symlink_metadata(name).map_err(|_| unsafe_files())?;
        if !same_inode(&before, &opened) || !same_inode(&opened, &visible) {
            return Err(unsafe_files());
        }
        dir = Dir::from_std_file(file.into_std());
    }
    let meta = dir.dir_metadata().map_err(|_| unsafe_files())?;
    if meta.uid() != uid || meta.mode() & 0o7777 != 0o700 {
        return Err(unsafe_files());
    }
    Ok(dir)
}

fn open_regular(root: &Dir, path: &Path, uid: u32, modes: &[u32]) -> Result<cap_std::fs::File> {
    let name = path.file_name().ok_or_else(unsafe_files)?;
    let before = root.symlink_metadata(name).map_err(|_| unsafe_files())?;
    let safe = |m: &Metadata| {
        m.is_file() && m.nlink() == 1 && m.uid() == uid && modes.contains(&(m.mode() & 0o7777))
    };
    if !safe(&before) {
        return Err(unsafe_files());
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC).bits() as i32);
    let file = root.open_with(name, &options).map_err(|_| unsafe_files())?;
    let opened = file.metadata().map_err(|_| unsafe_files())?;
    let visible = root.symlink_metadata(name).map_err(|_| unsafe_files())?;
    if !safe(&opened)
        || !safe(&visible)
        || !same_file(&before, &opened)
        || !same_file(&opened, &visible)
    {
        return Err(unsafe_files());
    }
    Ok(file)
}

fn same_inode(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    same_inode(a, b)
        && a.len() == b.len()
        && a.mode() == b.mode()
        && a.nlink() == b.nlink()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}

#[derive(Debug)]
struct Output {
    exit: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// A mutation's inspections and launchctl commands share one monotonic budget.
/// Expiry after a command also stays UNKNOWN: it may already have taken effect.
struct Deadline(Instant);
impl Deadline {
    fn new(timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(invalid());
        }
        Ok(Self(
            Instant::now().checked_add(timeout).ok_or_else(invalid)?,
        ))
    }

    fn remaining(&self) -> Result<Duration> {
        let remaining = self.0.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            Err(unknown())
        } else {
            Ok(remaining)
        }
    }

    fn call<T>(&self, operation: impl FnOnce(Duration) -> Result<T>) -> Result<T> {
        let result = operation(self.remaining()?);
        self.remaining()?;
        result
    }
}

fn run_launchctl(arguments: &[&str], timeout: Duration) -> Result<Output> {
    if timeout.is_zero() || timeout > Duration::from_secs(30) {
        return Err(invalid());
    }
    let mut command = Command::new("/bin/launchctl");
    command
        .args(arguments)
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .current_dir("/");
    run_bounded(command, timeout)
}

fn capture(
    mut stream: UnixStream,
    done: Arc<AtomicBool>,
    exceeded: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut finishing = None;
    loop {
        if done.load(Ordering::Acquire)
            && finishing.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(150)
        {
            return Err(ErrorKind::TimedOut.into());
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(n) if bytes.len() + n <= OUTPUT_LIMIT && !exceeded.load(Ordering::Acquire) => {
                bytes.extend_from_slice(&buffer[..n])
            }
            Ok(_) => {
                exceeded.store(true, Ordering::Release);
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => (),
            Err(e)
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                    && !done.load(Ordering::Acquire) => {}
            Err(e) => return Err(e),
        }
    }
}

fn run_bounded(mut command: Command, timeout: Duration) -> Result<Output> {
    let streams = || -> std::io::Result<(UnixStream, Stdio)> {
        let (read, write) = UnixStream::pair()?;
        read.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok((read, Stdio::from(OwnedFd::from(write))))
    };
    let (stdout, stdout_child) = streams().map_err(|_| unknown())?;
    let (stderr, stderr_child) = streams().map_err(|_| unknown())?;
    command
        .stdin(Stdio::null())
        .stdout(stdout_child)
        .stderr(stderr_child);
    let mut child = command.spawn().map_err(|_| unknown())?;
    drop(command);
    let done = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = [stdout, stderr]
        .into_iter()
        .map(|stream| {
            let done = done.clone();
            let exceeded = exceeded.clone();
            thread::spawn(move || capture(stream, done, exceeded))
        })
        .collect();
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout && !exceeded.load(Ordering::Acquire) => {
                thread::sleep(Duration::from_millis(5))
            }
            _ => {
                // Only our launchctl helper is killed, never the engine PID or
                // a service process. Its mutation may already have succeeded.
                let _ = child.kill();
                thread::spawn(move || {
                    let _ = child.wait();
                });
                break None;
            }
        }
    };
    done.store(true, Ordering::Release);
    let mut raw = Vec::new();
    for reader in readers {
        raw.push(
            reader
                .join()
                .map_err(|_| unknown())?
                .map_err(|_| unknown())?,
        );
    }
    let Some(status) = status else {
        return Err(unknown());
    };
    if exceeded.load(Ordering::Acquire) {
        return Err(unknown());
    }
    let stderr = raw.pop().ok_or_else(unknown)?;
    let stdout = raw.pop().ok_or_else(unknown)?;
    Ok(Output {
        exit: status.code().ok_or_else(unknown)?,
        stdout,
        stderr,
    })
}

fn successful_mutation(output: Output) -> Result<()> {
    if output.exit == 0 && output.stdout.is_empty() && output.stderr.is_empty() {
        Ok(())
    } else {
        Err(unknown())
    }
}

fn parse_observation(job: &Job, output: &Output) -> Result<Observation> {
    if output.stdout.len() > OUTPUT_LIMIT || output.stderr.len() > OUTPUT_LIMIT {
        return Err(unknown());
    }
    if output.exit != 0 {
        if !output.stdout.is_empty() {
            return Err(unknown());
        }
        let stderr = std::str::from_utf8(&output.stderr).map_err(|_| unknown())?;
        let missing = format!(
            "Bad request.\nCould not find service \"{}\" in domain for user gui: {}\n",
            job.label, job.uid
        );
        let no_gui = format!(
            "Bad request.\nCould not find domain for user gui: {}\n",
            job.uid
        );
        return match (output.exit, stderr) {
            (113, text) if text == missing => Ok(Observation::NotLoaded),
            (112, text) if text == no_gui => Ok(Observation::GuiUnavailable),
            _ => Err(unknown()),
        };
    }
    if !output.stderr.is_empty() {
        return Err(unknown());
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| unknown())?;
    if text.contains('\r') || text.contains('\0') {
        return Err(unknown());
    }
    let mut lines = text.lines();
    if lines.next() != Some(format!("{} = {{", job.service_target).as_str()) {
        return Err(unknown());
    }
    let mut depth: usize = 1;
    let mut fields = BTreeMap::new();
    let mut arguments = None;
    let mut in_arguments = false;
    let mut args = Vec::new();
    let mut ended = false;
    for line in lines {
        if ended {
            if !line.trim().is_empty() {
                return Err(unknown());
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if in_arguments {
            if line == "\t}" {
                arguments = Some(std::mem::take(&mut args));
                in_arguments = false;
                continue;
            }
            let value = line.strip_prefix("\t\t").ok_or_else(unknown)?;
            if value.starts_with('\t') || value.is_empty() || args.len() >= 16 {
                return Err(unknown());
            }
            args.push(value.to_owned());
            continue;
        }
        if line == "}" {
            if depth != 1 {
                return Err(unknown());
            }
            ended = true;
            depth = 0;
            continue;
        }
        if depth == 1 {
            let value = line.strip_prefix('\t').ok_or_else(unknown)?;
            if value.starts_with('\t') {
                return Err(unknown());
            }
            if value == "arguments = {" {
                if arguments.is_some() {
                    return Err(unknown());
                }
                in_arguments = true;
                continue;
            }
            let (key, val) = value.split_once(" = ").ok_or_else(unknown)?;
            if fields.insert(key.to_owned(), val.to_owned()).is_some() {
                return Err(unknown());
            }
            if val == "{" {
                depth += 1;
            }
        } else {
            // Nested environment/endpoints are not identity evidence. Require
            // exact structural indentation so nested spoofed fields stay nested.
            if line == format!("{}}}", "\t".repeat(depth - 1)) {
                depth -= 1;
            } else {
                let value = line.strip_prefix(&"\t".repeat(depth)).ok_or_else(unknown)?;
                if value.starts_with('\t') {
                    return Err(unknown());
                }
                if value.ends_with(" = {") || value.ends_with(" => {") {
                    depth += 1;
                }
            }
        }
        if depth > 16 {
            return Err(unknown());
        }
    }
    if !ended || depth != 0 || in_arguments || arguments.as_ref() != Some(&job.arguments) {
        return Err(unknown());
    }
    for (field, expected) in [
        ("path", path_text(&job.plist)?),
        ("type", "LaunchAgent"),
        ("program", path_text(&job.program)?),
        ("working directory", path_text(&job.working_directory)?),
        ("stdout path", path_text(&job.stdout)?),
        ("stderr path", path_text(&job.stderr)?),
    ] {
        if fields.get(field).map(String::as_str) != Some(expected) {
            return Err(unknown());
        }
    }
    let state = fields.get("state").ok_or_else(unknown)?.clone();
    if !matches!(
        state.as_str(),
        "running" | "not running" | "exited" | "waiting" | "spawn scheduled" | "spawn failed"
    ) {
        return Err(unknown());
    }
    let pid = fields.get("pid").map(|s| positive_integer(s)).transpose()?;
    if (state == "running" && pid.is_none())
        || (matches!(state.as_str(), "not running" | "exited") && pid.is_some())
    {
        return Err(unknown());
    }
    let last_exit_code = match fields.get("last exit code").map(String::as_str) {
        None | Some("(never exited)") => None,
        Some(text) => Some(exit_code(text)?),
    };
    let last_terminating_signal = fields
        .get("last terminating signal")
        .map(|text| positive_integer(text))
        .transpose()?;
    Ok(Observation::Loaded(JobState {
        pid,
        state,
        last_exit_code,
        last_terminating_signal,
    }))
}

fn exit_code(text: &str) -> Result<i32> {
    let (number, symbol) = match text.split_once(": ") {
        Some((number, symbol)) => (number, Some(symbol)),
        None => (text, None),
    };
    let code = number.parse::<i32>().map_err(|_| unknown())?;
    if code.to_string() != number {
        return Err(unknown());
    }
    if let Some(symbol) = symbol {
        // macOS launchctl annotates sysexits.h codes, for example the observed
        // pre-exec failure "78: EX_CONFIG". Do not accept arbitrary suffixes or
        // a valid symbolic name paired with the wrong integer.
        let expected = match code {
            0 => "EX_OK",
            64 => "EX_USAGE",
            65 => "EX_DATAERR",
            66 => "EX_NOINPUT",
            67 => "EX_NOUSER",
            68 => "EX_NOHOST",
            69 => "EX_UNAVAILABLE",
            70 => "EX_SOFTWARE",
            71 => "EX_OSERR",
            72 => "EX_OSFILE",
            73 => "EX_CANTCREAT",
            74 => "EX_IOERR",
            75 => "EX_TEMPFAIL",
            76 => "EX_PROTOCOL",
            77 => "EX_NOPERM",
            78 => "EX_CONFIG",
            _ => return Err(unknown()),
        };
        if symbol != expected {
            return Err(unknown());
        }
    }
    Ok(code)
}

fn positive_integer(text: &str) -> Result<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(unknown());
    }
    let number = text.parse().map_err(|_| unknown())?;
    if number == 0 {
        Err(unknown())
    } else {
        Ok(number)
    }
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn invalid() -> BxdlError {
    error(
        "SERVICE_OPTIONS_INVALID",
        "서비스 시도와 절대 경로를 확인하세요.",
    )
}
fn unsafe_files() -> BxdlError {
    error(
        "SERVICE_FILES_UNSAFE",
        "서비스 시도 파일의 경로·권한·동일성을 확인할 수 없습니다.",
    )
}
fn gui_unavailable() -> BxdlError {
    error(
        "SERVICE_GUI_UNAVAILABLE",
        "현재 사용자의 GUI 로그인 세션을 확인할 수 없습니다.",
    )
}
fn unknown() -> BxdlError {
    error(
        "SERVICE_OBSERVATION_UNKNOWN",
        "서비스 작업 또는 관측 결과를 확정할 수 없습니다. 시도 자료를 보존했습니다.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    fn spec() -> Job {
        Job::new(
            501,
            Path::new("/private/instance one"),
            "bxdl-123-a0",
            Path::new("/private/attempt/bxdl"),
            Path::new("/private/attempt"),
        )
        .unwrap()
    }

    fn print_fixture(job: &Job, state: &str, pid: Option<u32>, exit: &str) -> String {
        let pid = pid
            .map(|pid| format!("\tpid = {pid}\n"))
            .unwrap_or_default();
        let args = job
            .arguments
            .iter()
            .map(|a| format!("\t\t{a}\n"))
            .collect::<String>();
        format!(
            "{} = {{\n\tactive count = 1\n\tpath = {}\n\ttype = LaunchAgent\n\tstate = {state}\n\tprogram = {}\n\targuments = {{\n{args}\t}}\n\tworking directory = {}\n\tstdout path = {}\n\tstderr path = {}\n\tdefault environment = {{\n\t\tPATH => /usr/bin:/bin\n\t}}\n\tenvironment = {{\n\t\tXPC_SERVICE_NAME => {}\n\t}}\n\truns = 1\n{pid}\tlast exit code = {exit}\n\tendpoints = {{\n\t\t\"example\" = {{\n\t\t\tport = 0x0\n\t\t}}\n\t}}\n}}\n",
            job.service_target,
            job.plist.display(),
            job.program.display(),
            job.working_directory.display(),
            job.stdout.display(),
            job.stderr.display(),
            job.label
        )
    }

    fn observed(job: &Job, text: &str) -> Result<Observation> {
        parse_observation(
            job,
            &Output {
                exit: 0,
                stdout: text.as_bytes().to_vec(),
                stderr: vec![],
            },
        )
    }

    #[test]
    fn plist_escapes_xml_and_keeps_explicit_single_load_policy() {
        let job = Job::new(
            501,
            Path::new("/private/one & <two> \"three\" 'four'"),
            "a",
            Path::new("/private/attempt/bxdl"),
            Path::new("/private/attempt"),
        )
        .unwrap();
        let xml = job.plist_bytes().unwrap();
        assert!(xml.contains("one &amp; &lt;two&gt; &quot;three&quot; &apos;four&apos;"));
        assert!(xml.contains("<key>RunAtLoad</key><true/>"));
        assert!(xml.contains("<key>KeepAlive</key><false/>"));
        assert!(xml.contains("<key>ExitTimeOut</key><integer>60</integer>"));
        assert!(xml.contains("<key>Umask</key><integer>63</integer>"));
        assert_eq!(job.arguments[1], "service-run");
        assert_eq!(job.arguments.len(), 7);
        assert_eq!(job.plist.parent(), Some(job.working_directory.as_path()));
        assert!(!xml.contains("/bin/sh"));
        assert!(!xml.contains("LaunchAgents"));
    }

    #[test]
    fn specification_rejects_unsafe_inputs_and_separates_attempt_labels() {
        for path in [
            "relative",
            "/private/../attempt",
            "/private/./attempt",
            "/private//attempt",
            "/private/attempt/",
            "/private/line\nnext",
            "/private/tab\t",
            "/",
        ] {
            assert!(path_text(Path::new(path)).is_err(), "{path:?}");
        }
        for attempt in ["", "../x", "--x/y", "a b", "é", "\n"] {
            assert!(
                Job::new(
                    501,
                    Path::new("/a"),
                    attempt,
                    Path::new("/w/bxdl"),
                    Path::new("/w")
                )
                .is_err()
            );
        }
        assert!(
            Job::new(
                0,
                Path::new("/a"),
                "a",
                Path::new("/w/bxdl"),
                Path::new("/w")
            )
            .is_err()
        );
        assert!(
            Job::new(
                501,
                Path::new("/a"),
                "a",
                Path::new("/else/bxdl"),
                Path::new("/w")
            )
            .is_err()
        );
        let a = spec();
        let b = Job::new(
            501,
            Path::new("/another"),
            "bxdl-123-a0",
            &a.program,
            &a.working_directory,
        )
        .unwrap();
        let c = Job::new(
            501,
            Path::new("/private/instance one"),
            "bxdl-123-a1",
            &a.program,
            &a.working_directory,
        )
        .unwrap();
        assert_ne!(a.label, b.label);
        assert_ne!(a.label, c.label);
        assert!(
            a.label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        );
    }

    #[test]
    fn print_observation_requires_binding_and_tracks_running_and_stopped() {
        let job = spec();
        let Observation::Loaded(running) = observed(
            &job,
            &print_fixture(&job, "running", Some(321), "(never exited)"),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(running.pid, Some(321));
        assert!(!running.is_stopped());
        let Observation::Loaded(stopped) =
            observed(&job, &print_fixture(&job, "not running", None, "0")).unwrap()
        else {
            panic!()
        };
        assert!(stopped.is_stopped());
        assert_eq!(stopped.last_exit_code, Some(0));
        let Observation::Loaded(pending) = observed(
            &job,
            &print_fixture(&job, "not running", None, "(never exited)"),
        )
        .unwrap() else {
            panic!()
        };
        assert!(!pending.is_stopped());
    }

    #[test]
    fn actual_launchctl_sysexits_annotation_preserves_code_and_rejects_ambiguity() {
        let job = spec();
        let raw = print_fixture(&job, "not running", None, "78: EX_CONFIG");
        let Observation::Loaded(state) = observed(&job, &raw).unwrap() else {
            panic!()
        };
        assert_eq!(state.last_exit_code, Some(78));
        assert!(state.is_stopped());
        assert_eq!(exit_code("0").unwrap(), 0);
        assert_eq!(exit_code("0: EX_OK").unwrap(), 0);
        assert_eq!(exit_code("64: EX_USAGE").unwrap(), 64);
        assert_eq!(exit_code("77: EX_NOPERM").unwrap(), 77);
        for value in [
            "77: EX_CONFIG",
            "78: EX_FUTURE",
            "78: EX_CONFIG extra",
            "78: EX_CONFIG: EX_CONFIG",
            "78:EX_CONFIG",
            "078: EX_CONFIG",
            "+78",
            "78 ",
            "1: EX_CONFIG",
        ] {
            assert!(exit_code(value).is_err(), "{value}");
        }
        let duplicate = raw.replace(
            "\tlast exit code = 78: EX_CONFIG\n",
            "\tlast exit code = 78: EX_CONFIG\n\tlast exit code = 0\n",
        );
        assert!(observed(&job, &duplicate).is_err());
    }

    #[test]
    fn observation_rejects_each_wrong_binding_duplicate_and_truncated_field() {
        let job = spec();
        let raw = print_fixture(&job, "running", Some(321), "(never exited)");
        for (from, to) in [
            (job.service_target.as_str(), "gui/501/foreign"),
            (
                &format!("path = {}", job.plist.display()),
                "path = /foreign",
            ),
            ("type = LaunchAgent", "type = LaunchDaemon"),
            (
                &format!("program = {}", job.program.display()),
                "program = /foreign",
            ),
            ("\t\tservice-run\n", "\t\tinit\n"),
            (
                &format!("working directory = {}", job.working_directory.display()),
                "working directory = /foreign",
            ),
            (
                &format!("stdout path = {}", job.stdout.display()),
                "stdout path = /foreign",
            ),
            (
                &format!("stderr path = {}", job.stderr.display()),
                "stderr path = /foreign",
            ),
            ("\tpid = 321\n", "\tpid = 321\n\tpid = 321\n"),
            ("\tpid = 321\n", ""),
            ("\tpid = 321\n", "\tpid = 0\n"),
            ("state = running", "state = unknown future format"),
        ] {
            let changed = raw.replacen(from, to, 1);
            assert_ne!(changed, raw);
            assert!(observed(&job, &changed).is_err(), "mutation: {from}");
        }
        assert!(observed(&job, raw.trim_end_matches("}\n")).is_err());
        assert!(observed(&job, &format!("{raw}foreign output\n")).is_err());
        assert!(observed(&job, &print_fixture(&job, "not running", Some(321), "0")).is_err());
        // A nested field cannot substitute for a required top-level binding.
        let nested = raw
            .replace(&format!("\tprogram = {}\n", job.program.display()), "")
            .replace(
                "\t\tPATH => /usr/bin:/bin\n",
                &format!("\t\tprogram = {}\n", job.program.display()),
            );
        assert!(observed(&job, &nested).is_err());
    }

    #[test]
    fn absent_job_and_absent_gui_require_exact_error_and_exit_pair() {
        let job = spec();
        let missing = format!(
            "Bad request.\nCould not find service \"{}\" in domain for user gui: 501\n",
            job.label
        );
        let no_gui = "Bad request.\nCould not find domain for user gui: 501\n";
        for (exit, stderr, expected) in [
            (113, missing.as_str(), Observation::NotLoaded),
            (112, no_gui, Observation::GuiUnavailable),
        ] {
            let out = Output {
                exit,
                stdout: vec![],
                stderr: stderr.as_bytes().to_vec(),
            };
            assert_eq!(parse_observation(&job, &out).unwrap(), expected);
            assert!(parse_observation(&job, &Output { exit: 5, ..out }).is_err());
        }
        for stderr in [
            missing.replace("501", "502"),
            format!("{missing}other error\n"),
            "Permission denied\n".into(),
        ] {
            assert!(
                parse_observation(
                    &job,
                    &Output {
                        exit: 113,
                        stdout: vec![],
                        stderr: stderr.into_bytes()
                    }
                )
                .is_err()
            );
        }
    }

    fn owned_fixture() -> (tempfile::TempDir, Job) {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let program = root.join("bxdl");
        fs::write(&program, b"test-owned placeholder, never executed").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let job = Job::new(
            rustix::process::getuid().as_raw(),
            &root.join("instance"),
            "owned-test",
            &program,
            &root,
        )
        .unwrap();
        (temp, job)
    }

    #[test]
    fn preparation_publishes_private_files_and_never_overwrites() {
        let (_temp, job) = owned_fixture();
        job.prepare().unwrap();
        for path in [&job.plist, &job.stdout, &job.stderr] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o7777,
                0o600
            );
        }
        assert_eq!(
            fs::read_to_string(&job.plist).unwrap(),
            job.plist_bytes().unwrap()
        );
        fs::write(&job.stderr, b"preserved test marker").unwrap();
        assert_eq!(job.prepare().unwrap_err().code, "SERVICE_FILE_EXISTS");
        assert_eq!(fs::read(&job.stderr).unwrap(), b"preserved test marker");
    }

    #[test]
    fn preparation_refuses_existing_output_before_any_publication() {
        let (_temp, job) = owned_fixture();
        fs::write(&job.stderr, b"existing").unwrap();
        assert!(job.prepare().is_err());
        assert!(!job.stdout.exists());
        assert!(!job.plist.exists());
    }

    #[test]
    fn preparation_and_recheck_reject_links_permissions_and_plist_changes() {
        let (_temp, job) = owned_fixture();
        fs::set_permissions(&job.working_directory, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(job.prepare().is_err());
        fs::set_permissions(&job.working_directory, fs::Permissions::from_mode(0o700)).unwrap();
        fs::hard_link(&job.program, job.working_directory.join("alias")).unwrap();
        assert!(job.prepare().is_err());
        fs::remove_file(job.working_directory.join("alias")).unwrap();
        let target = job.working_directory.join("real-program");
        fs::rename(&job.program, &target).unwrap();
        symlink(&target, &job.program).unwrap();
        assert!(job.prepare().is_err());
        fs::remove_file(&job.program).unwrap();
        fs::rename(&target, &job.program).unwrap();
        job.prepare().unwrap();
        fs::write(&job.plist, b"untrusted changed plist").unwrap();
        assert!(job.recheck_files().is_err());
    }

    #[test]
    fn symlink_ancestor_is_rejected_even_when_target_is_private() {
        let (_temp, job) = owned_fixture();
        let link = job.working_directory.join("alias-dir");
        symlink(&job.working_directory, &link).unwrap();
        assert!(open_private_directory(&link, job.uid).is_err());
    }

    #[test]
    fn bounded_helper_captures_output_and_does_not_expose_errors() {
        let mut command = Command::new("/usr/bin/printf");
        command.arg("bounded-output").env_clear();
        let output = run_bounded(command, Duration::from_secs(2)).unwrap();
        assert_eq!(output.stdout, b"bounded-output");
        assert!(output.stderr.is_empty());
        assert_eq!(output.exit, 0);
        assert!(successful_mutation(output).is_err());
        let err = run_bounded(
            Command::new("/not/a/real/SECRET-program"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(!err.message.contains("SECRET"));
        assert_eq!(err.code, "SERVICE_OBSERVATION_UNKNOWN");
    }

    #[test]
    fn bounded_helper_timeout_and_output_limit_fail_closed() {
        let mut sleep = Command::new("/bin/sleep");
        sleep.arg("2").env_clear();
        let start = Instant::now();
        assert!(run_bounded(sleep, Duration::from_millis(20)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        let mut output = Command::new("/usr/bin/yes");
        output.arg("owned-helper-output").env_clear();
        assert!(run_bounded(output, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn shared_deadline_decreases_and_refuses_a_later_call_after_expiry() {
        assert!(Deadline::new(Duration::ZERO).is_err());
        assert!(Deadline::new(Duration::from_secs(31)).is_err());
        let deadline = Deadline::new(Duration::from_secs(1)).unwrap();
        let first = deadline.call(Ok).unwrap();
        thread::sleep(Duration::from_millis(20));
        let second = deadline.call(Ok).unwrap();
        assert!(second < first);
        assert!(first - second >= Duration::from_millis(20));

        // A completed earlier command must not grant the next command a fresh
        // full timeout, including when expiry occurs between the two calls.
        let expired = Deadline(Instant::now());
        let mut called = false;
        let error = expired
            .call(|_| {
                called = true;
                Ok(())
            })
            .unwrap_err();
        assert!(!called);
        assert_eq!(error.code, "SERVICE_OBSERVATION_UNKNOWN");
    }

    #[test]
    fn deadline_expired_after_a_mutation_cannot_report_success() {
        let deadline = Deadline::new(Duration::from_millis(10)).unwrap();
        let mut performed = false;
        let result = deadline.call(|remaining| {
            performed = true;
            // This represents a helper which completed its side effect but
            // only delivered the result after the overall method deadline.
            thread::sleep(remaining + Duration::from_millis(5));
            Ok(())
        });
        assert!(performed);
        assert_eq!(result.unwrap_err().code, "SERVICE_OBSERVATION_UNKNOWN");
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn unsupported_host_refuses_service_calls_before_launchctl() {
        assert_eq!(
            spec().inspect(Duration::from_secs(1)).unwrap_err().code,
            "SERVICE_PLATFORM_UNSUPPORTED"
        );
    }
}
