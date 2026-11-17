//! Root-controlled, per-attempt system units. The only executable is the pinned
//! BXDL gate snapshot: direct `systemctl start` therefore runs the same durable
//! single-use gate. The gate must verify its root journal before dropping all
//! supplementary groups, GID and UID and execing Java in the same PID.
//!
//! `Type=exec` observes the gate executable, not Java readiness. Enqueued start
//! and stop requests are never proof of engine readiness or clean DB closure.
//! Reference: systemd.service(5), systemd.exec(5), systemd.kill(5), systemctl(1).
use super::{ExpectedUnit, UnitProperties, properties, run_read_only};
use crate::error::{BxdlError, Result};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, MetadataExt, OpenOptions, OpenOptionsExt, Permissions, PermissionsExt},
};
use rustix::fs::OFlags;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const SYSTEMCTL: &str = "/usr/bin/systemctl";
const UNIT_DIRECTORY: &str = "/etc/systemd/system";
const UNIT_MODE: u32 = 0o644;
const STOP_TIMEOUT_SECONDS: u32 = 60;
const UNIT_PROPERTIES: &[&str] = &[
    "Id",
    "LoadState",
    "ActiveState",
    "SubState",
    "MainPID",
    "InvocationID",
    "ControlGroup",
    "Result",
    "ExecMainCode",
    "ExecMainStatus",
];
const EXIT_HOOK_PROPERTIES: &[&str] = &[
    "Id",
    "ActiveState",
    "SubState",
    "ControlPID",
    "MainPID",
    "InvocationID",
    "ControlGroup",
];
const CONFIG_PROPERTIES: &[&str] = &[
    "FragmentPath",
    "DropInPaths",
    "NeedDaemonReload",
    "Transient",
];

#[derive(Clone, Debug)]
pub struct Unit {
    name: String,
    control: PathBuf,
    attempt: String,
    program: PathBuf,
}

impl Unit {
    /// Pure construction. Fixed Linux control paths intentionally admit no
    /// whitespace, specifiers, environment expansion or command-line quoting.
    pub fn new(control: &Path, attempt: &str, program: &Path) -> Result<Self> {
        path_text(control)?;
        path_text(program)?;
        if attempt.is_empty()
            || attempt.len() > 80
            || !attempt
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !attempt
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
            || program != control.join("attempts").join(attempt).join("bxdl-worker")
        {
            return Err(invalid());
        }
        let digest = hex::encode(Sha256::digest(path_text(control)?.as_bytes()));
        let name = format!("bxdl-{}-{attempt}.service", &digest[..24]);
        if !super::valid_unit(&name) {
            return Err(invalid());
        }
        Ok(Self {
            name,
            control: control.into(),
            attempt: attempt.into(),
            program: program.into(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn path(&self) -> PathBuf {
        Path::new(UNIT_DIRECTORY).join(&self.name)
    }

    /// No [Install] section, automatic restart, shell, ExecStartPre, or boot
    /// enablement. Root runs the gate and exit receipt hook; Java MUST drop
    /// privileges before executing.
    pub fn render(&self) -> Result<String> {
        Ok(format!(
            "[Unit]\nDescription=BXDL guarded engine attempt\n\
             [Service]\nType=exec\nUser=root\nGroup=root\n\
             ExecStart={} systemd-run --control {} --attempt {} --json\n\
             ExecStopPost={} systemd-exit --control {} --attempt {} --json\n\
             SuccessExitStatus=143\nWorkingDirectory={}\nRestart=no\nKillMode=control-group\n\
             KillSignal=SIGTERM\nSendSIGKILL=no\nTimeoutStopSec={}\n\
             UMask=0077\nNoNewPrivileges=yes\nStandardInput=null\n\
             StandardOutput=journal\nStandardError=journal\n\
             Environment=LANG=C LC_ALL=C PATH=\n",
            path_text(&self.program)?,
            path_text(&self.control)?,
            self.attempt,
            path_text(&self.program)?,
            path_text(&self.control)?,
            self.attempt,
            path_text(self.program.parent().ok_or_else(invalid)?)?,
            STOP_TIMEOUT_SECONDS,
        ))
    }

    /// Publish a new root-owned unit. Partial files remain as evidence; neither
    /// overwrite nor remove is a recovery mechanism. No systemctl is executed.
    pub fn prepare(&self) -> Result<()> {
        check_host_root()?;
        self.check_inputs()?;
        let directory = trusted_directory(Path::new(UNIT_DIRECTORY))?;
        let bytes = self.render()?;
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(UNIT_MODE)
            .custom_flags((OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32);
        let mut file = directory
            .open_with(&self.name, &options)
            .map_err(|_| files_error())?;
        file.set_permissions(Permissions::from_mode(UNIT_MODE))
            .map_err(|_| files_error())?;
        file.write_all(bytes.as_bytes())
            .map_err(|_| files_error())?;
        file.sync_all().map_err(|_| files_error())?;
        let final_meta = file.metadata().map_err(|_| files_error())?;
        let visible = directory
            .symlink_metadata(&self.name)
            .map_err(|_| files_error())?;
        if !same_inode(&final_meta, &visible) || !regular(&final_meta, UNIT_MODE) {
            return Err(files_error());
        }
        directory
            .try_clone()
            .map_err(|_| files_error())?
            .into_std_file()
            .sync_all()
            .map_err(|_| files_error())?;
        self.check_files()
    }

    /// Install only this new attempt and reload manager configuration. Failure
    /// after publication is UNKNOWN; a second invocation cannot overwrite it.
    pub fn install(&self, timeout: Duration) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        self.prepare()?;
        request(Action::Reload, deadline.remaining()?)?;
        self.check_configuration(deadline.remaining()?)
    }

    pub fn inspect(&self, timeout: Duration) -> Result<UnitProperties> {
        let deadline = Deadline::new(timeout)?;
        check_host_root()?;
        self.check_files()?;
        self.check_configuration(deadline.remaining()?)?;
        let output = query(&self.name, UNIT_PROPERTIES, deadline.remaining()?)?;
        let observed = super::parse_unit(&output).map_err(|_| unknown())?;
        if observed.unit != self.name {
            return Err(unknown());
        }
        Ok(observed)
    }

    /// Authenticate the root post-exit hook before it writes a durable receipt.
    /// systemd may garbage-collect successful inactive units; this observation
    /// preserves the exact invocation while the hook is still its ControlPID.
    /// The caller also verifies /proc/self/cgroup and its root consumed attempt.
    pub fn inspect_exit_hook(
        &self,
        expected_invocation_id: &str,
        current_control_pid: u32,
        timeout: Duration,
    ) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        check_host_root()?;
        if current_control_pid != std::process::id() {
            return Err(invalid());
        }
        self.check_files()?;
        self.check_configuration(deadline.remaining()?)?;
        let bytes = query(&self.name, EXIT_HOOK_PROPERTIES, deadline.remaining()?)?;
        verify_exit_hook(
            &bytes,
            &self.name,
            expected_invocation_id,
            current_control_pid,
        )
    }

    /// Only the initial never-run unit may be queued here. The root gate also
    /// refuses replay independently of this check, including direct systemctl.
    pub fn start(&self, timeout: Duration) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        let observed = self.inspect(deadline.remaining()?)?;
        if !initial_unit(&observed) {
            return Err(unknown());
        }
        request(Action::Start(&self.name), deadline.remaining()?)
    }

    /// The root controller must audit its pinned attempt, process start ticks,
    /// UID/GID, executable, argv, cgroup and authenticated engine identity before
    /// constructing authorization. A fresh systemd observation is checked again
    /// here. Queuing stop is not proof of STOPPED or of an empty cgroup.
    pub(crate) fn stop(&self, authorization: &StopAuthorization, timeout: Duration) -> Result<()> {
        let deadline = Deadline::new(timeout)?;
        if authorization.expected.unit != self.name {
            return Err(invalid());
        }
        let observed = self.inspect(deadline.remaining()?)?;
        if !observed.matches_running_metadata(&authorization.expected) {
            return Err(unknown());
        }
        request(Action::Stop(&self.name), deadline.remaining()?)
    }

    fn check_inputs(&self) -> Result<()> {
        let control = trusted_directory(&self.control)?;
        if control.dir_metadata().map_err(|_| files_error())?.mode() & 0o7777 != 0o755 {
            return Err(files_error());
        }
        let parent = trusted_directory(self.program.parent().ok_or_else(invalid)?)?;
        if parent.dir_metadata().map_err(|_| files_error())?.mode() & 0o7777 != 0o755 {
            return Err(files_error());
        }
        let file = open_regular(&parent, Path::new("bxdl-worker"), 0o755)?;
        if file.metadata().map_err(|_| files_error())?.len() == 0 {
            return Err(files_error());
        }
        Ok(())
    }

    fn check_files(&self) -> Result<()> {
        self.check_inputs()?;
        let directory = trusted_directory(Path::new(UNIT_DIRECTORY))?;
        let mut file = open_regular(&directory, Path::new(&self.name), UNIT_MODE)?;
        let before = file.metadata().map_err(|_| files_error())?;
        let expected = self.render()?;
        if before.len() != expected.len() as u64 {
            return Err(files_error());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| files_error())?;
        let after = file.metadata().map_err(|_| files_error())?;
        let visible = directory
            .symlink_metadata(&self.name)
            .map_err(|_| files_error())?;
        if bytes != expected.as_bytes()
            || !same_file(&before, &after)
            || !same_inode(&after, &visible)
        {
            return Err(files_error());
        }
        Ok(())
    }

    fn check_configuration(&self, timeout: Duration) -> Result<()> {
        let bytes = query(&self.name, CONFIG_PROPERTIES, timeout)?;
        verify_configuration(&bytes, &self.path())
    }
}

/// An internal control-layer capability. There is deliberately no public proof
/// constructor and no CLI flag that accepts a caller's claim of process identity.
#[derive(Debug)]
pub(crate) struct StopAuthorization {
    expected: ExpectedUnit,
}
impl StopAuthorization {
    /// Call only after the root controller has independently verified the
    /// pinned engine process and authenticated report for this exact attempt.
    pub(crate) fn after_process_audit(expected: ExpectedUnit) -> Result<Self> {
        if !super::valid_unit(&expected.unit)
            || !super::valid_invocation(&expected.invocation_id)
            || expected.main_pid <= 1
            || expected.control_group != format!("/system.slice/{}", expected.unit)
        {
            return Err(invalid());
        }
        Ok(Self { expected })
    }
}

fn verify_exit_hook(bytes: &[u8], unit: &str, invocation: &str, pid: u32) -> Result<()> {
    if !super::valid_unit(unit) || !super::valid_invocation(invocation) || pid <= 1 {
        return Err(invalid());
    }
    let values = properties(bytes, EXIT_HOOK_PROPERTIES).map_err(|_| unknown())?;
    if values["Id"] != unit
        || values["ActiveState"] != "deactivating"
        || values["SubState"] != "stop-post"
        || values["InvocationID"] != invocation
        || super::decimal(values["ControlPID"]).map_err(|_| unknown())? != pid
        || super::decimal(values["MainPID"]).map_err(|_| unknown())? != 0
        || values["ControlGroup"] != format!("/system.slice/{unit}")
    {
        return Err(unknown());
    }
    Ok(())
}

fn verify_configuration(bytes: &[u8], expected_path: &Path) -> Result<()> {
    let values = properties(bytes, CONFIG_PROPERTIES).map_err(|_| unknown())?;
    if values["FragmentPath"] != path_text(expected_path)?
        || !values["DropInPaths"].is_empty()
        || values["NeedDaemonReload"] != "no"
        || values["Transient"] != "no"
    {
        return Err(unknown());
    }
    Ok(())
}

fn initial_unit(observed: &UnitProperties) -> bool {
    observed.load_state == "loaded"
        && observed.active_state == "inactive"
        && observed.sub_state == "dead"
        && observed.main_pid == 0
        && observed.invocation_id.is_empty()
        && observed.control_group.is_empty()
}

#[derive(Clone, Copy)]
enum Action<'a> {
    Reload,
    Start(&'a str),
    Stop(&'a str),
}
fn mutation_arguments(action: Action<'_>) -> Result<Vec<String>> {
    let mut args: Vec<String> = ["--system", "--no-pager", "--no-ask-password"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    match action {
        Action::Reload => args.push("daemon-reload".into()),
        Action::Start(unit) | Action::Stop(unit) => {
            if !super::valid_unit(unit) {
                return Err(invalid());
            }
            args.extend([
                "--no-block".into(),
                if matches!(action, Action::Start(_)) {
                    "start"
                } else {
                    "stop"
                }
                .into(),
                "--".into(),
                unit.into(),
            ]);
        }
    }
    Ok(args)
}
fn query_arguments(unit: &str, names: &[&str]) -> Result<Vec<String>> {
    if !super::valid_unit(unit)
        || (names != UNIT_PROPERTIES && names != CONFIG_PROPERTIES && names != EXIT_HOOK_PROPERTIES)
    {
        return Err(invalid());
    }
    let mut args: Vec<String> = [
        "--system",
        "--no-pager",
        "--no-ask-password",
        "show",
        "--all",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.extend(names.iter().map(|name| format!("--property={name}")));
    args.extend(["--".into(), unit.into()]);
    Ok(args)
}
fn query(unit: &str, names: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    check_systemctl()?;
    let mut command = Command::new(SYSTEMCTL);
    command.args(query_arguments(unit, names)?);
    let output = run_read_only(command, timeout).map_err(|_| unknown())?;
    if output.exit != 0 || !output.stderr.is_empty() {
        return Err(unknown());
    }
    Ok(output.stdout)
}
fn request(action: Action<'_>, timeout: Duration) -> Result<()> {
    check_host_root()?;
    check_systemctl()?;
    let mut command = Command::new(SYSTEMCTL);
    command.args(mutation_arguments(action)?);
    let output = run_read_only(command, timeout).map_err(|_| mutation_unknown())?;
    if output.exit != 0 || !output.stdout.is_empty() || !output.stderr.is_empty() {
        return Err(mutation_unknown());
    }
    Ok(())
}
fn check_systemctl() -> Result<()> {
    let directory = trusted_directory(Path::new("/usr/bin"))?;
    let meta = directory
        .symlink_metadata("systemctl")
        .map_err(|_| files_error())?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.uid() != 0
        || meta.mode() & 0o7022 != 0
        || meta.mode() & 0o111 == 0
        || meta.len() == 0
    {
        return Err(files_error());
    }
    Ok(())
}
fn check_host_root() -> Result<()> {
    if !cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        return Err(BxdlError::new(
            "SERVICE_PLATFORM_UNSUPPORTED",
            "systemd 서비스 제어는 Linux GNU x86_64 호스트에서만 지원합니다.",
        ));
    }
    if rustix::process::getuid().as_raw() != 0 || rustix::process::geteuid().as_raw() != 0 {
        return Err(BxdlError::new(
            "SERVICE_ROOT_REQUIRED",
            "Linux 시스템 서비스 제어는 root 권한이 필요합니다.",
        ));
    }
    Ok(())
}
fn path_text(path: &Path) -> Result<&str> {
    let text = path.to_str().ok_or_else(invalid)?;
    if !path.is_absolute()
        || text.len() > 4096
        || text == "/"
        || !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
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
    if reconstructed.as_os_str() != path.as_os_str() {
        return Err(invalid());
    }
    Ok(text)
}
fn trusted_directory(path: &Path) -> Result<Dir> {
    path_text(path)?;
    let mut directory =
        Dir::open_ambient_dir("/", ambient_authority()).map_err(|_| files_error())?;
    trusted_directory_metadata(&directory.dir_metadata().map_err(|_| files_error())?)?;
    for component in path.components().skip(1) {
        let Component::Normal(name) = component else {
            return Err(files_error());
        };
        let before = directory
            .symlink_metadata(name)
            .map_err(|_| files_error())?;
        trusted_directory_metadata(&before)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags((OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32);
        let file = directory
            .open_with(name, &options)
            .map_err(|_| files_error())?;
        let opened = file.metadata().map_err(|_| files_error())?;
        let visible = directory
            .symlink_metadata(name)
            .map_err(|_| files_error())?;
        if !same_inode(&before, &opened) || !same_inode(&opened, &visible) {
            return Err(files_error());
        }
        directory = Dir::from_std_file(file.into_std());
    }
    Ok(directory)
}
fn trusted_directory_metadata(metadata: &Metadata) -> Result<()> {
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != 0
        || metadata.mode() & 0o7022 != 0
    {
        return Err(files_error());
    }
    Ok(())
}
fn regular(metadata: &Metadata, mode: u32) -> bool {
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == 0
        && metadata.nlink() == 1
        && metadata.mode() & 0o7777 == mode
}
fn open_regular(directory: &Dir, name: &Path, mode: u32) -> Result<cap_std::fs::File> {
    let before = directory
        .symlink_metadata(name)
        .map_err(|_| files_error())?;
    if !regular(&before, mode) {
        return Err(files_error());
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32);
    let file = directory
        .open_with(name, &options)
        .map_err(|_| files_error())?;
    let opened = file.metadata().map_err(|_| files_error())?;
    let visible = directory
        .symlink_metadata(name)
        .map_err(|_| files_error())?;
    if !regular(&opened, mode) || !same_inode(&before, &opened) || !same_inode(&opened, &visible) {
        return Err(files_error());
    }
    Ok(file)
}
fn same_inode(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}
fn same_file(left: &Metadata, right: &Metadata) -> bool {
    same_inode(left, right)
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.nlink() == right.nlink()
}
struct Deadline {
    started: Instant,
    timeout: Duration,
}
impl Deadline {
    fn new(timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(invalid());
        }
        Ok(Self {
            started: Instant::now(),
            timeout,
        })
    }
    fn remaining(&self) -> Result<Duration> {
        self.timeout
            .checked_sub(self.started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(mutation_unknown)
    }
}
fn invalid() -> BxdlError {
    BxdlError::new(
        "SERVICE_SYSTEMD_OPTIONS_INVALID",
        "systemd 시도 이름·경로 또는 제한 시간이 올바르지 않습니다.",
    )
}
fn files_error() -> BxdlError {
    BxdlError::new(
        "SERVICE_SYSTEMD_FILES_UNSAFE",
        "root 소유 서비스 파일·경로가 없거나 변경되었거나 안전하지 않습니다.",
    )
}
fn unknown() -> BxdlError {
    BxdlError::new(
        "SERVICE_SYSTEMD_OBSERVATION_UNKNOWN",
        "systemd 서비스 관측이 불명확합니다. 실행·종료 완료를 확정하지 않습니다.",
    )
}
fn mutation_unknown() -> BxdlError {
    BxdlError::new(
        "SERVICE_SYSTEMD_REQUEST_UNKNOWN",
        "systemd 요청 결과가 불명확합니다. 요청이 이미 전달되었을 수 있으므로 재시작하지 말고 상태를 확인하세요.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit() -> Unit {
        Unit::new(
            Path::new("/var/lib/bxdl/control/node1"),
            "attempt-123",
            Path::new("/var/lib/bxdl/control/node1/attempts/attempt-123/bxdl-worker"),
        )
        .unwrap()
    }
    fn expected() -> ExpectedUnit {
        let unit = unit();
        ExpectedUnit {
            unit: unit.name().into(),
            invocation_id: "1234567890abcdef1234567890abcdef".into(),
            main_pid: 123,
            control_group: format!("/system.slice/{}", unit.name()),
        }
    }
    fn inactive() -> UnitProperties {
        UnitProperties {
            unit: unit().name().into(),
            load_state: "loaded".into(),
            active_state: "inactive".into(),
            sub_state: "dead".into(),
            main_pid: 0,
            invocation_id: String::new(),
            control_group: String::new(),
            result: "success".into(),
            exec_main_code: 0,
            exec_main_status: 0,
        }
    }
    #[test]
    fn unit_binds_control_attempt_and_gate_snapshot() {
        let unit = unit();
        assert_eq!(unit.name(), super::tests::unit().name());
        assert!(unit.name().starts_with("bxdl-"));
        assert!(unit.name().ends_with("-attempt-123.service"));
        assert_eq!(unit.path().parent(), Some(Path::new(UNIT_DIRECTORY)));
        let different = Unit::new(
            Path::new("/var/lib/bxdl/control/node2"),
            "attempt-123",
            Path::new("/var/lib/bxdl/control/node2/attempts/attempt-123/bxdl-worker"),
        )
        .unwrap();
        assert_ne!(unit.name(), different.name());
        for wrong in [
            "/var/lib/bxdl/control/node1/bxdl-worker",
            "/var/lib/bxdl/control/node1/attempts/attempt-123/java",
            "/var/lib/bxdl/control/node1/attempts/attempt-124/bxdl-worker",
        ] {
            assert!(Unit::new(&unit.control, &unit.attempt, Path::new(wrong)).is_err());
        }
    }
    #[test]
    fn unit_rejects_attempt_and_systemd_expansion_injection() {
        for attempt in [
            "",
            "--evil",
            "a.service\nExecStart=/bin/sh",
            "%i",
            "${USER}",
            "a/b",
            "UPPER",
            "a.b",
        ] {
            let program = Path::new("/var/lib/bxdl/control/node1")
                .join("attempts")
                .join(attempt)
                .join("bxdl-worker");
            let result = Unit::new(Path::new("/var/lib/bxdl/control/node1"), attempt, &program);
            assert!(result.is_err(), "{attempt}");
        }
        for path in [
            "relative",
            "/",
            "/var/lib//bxdl",
            "/var/lib/./bxdl",
            "/var/lib/../bxdl",
            "/var/lib/bxdl/",
            "/var/lib/$USER",
            "/var/lib/%i",
            "/var/lib/a b",
            "/var/lib/a\nb",
            "/var/lib/a;id",
            "/var/lib/a\\b",
        ] {
            assert!(path_text(Path::new(path)).is_err(), "{path}");
        }
    }
    #[test]
    fn render_uses_one_root_gate_without_restart_or_boot_enablement() {
        let rendered = unit().render().unwrap();
        assert_eq!(
            rendered
                .lines()
                .filter(|line| line.starts_with("ExecStart="))
                .count(),
            1
        );
        assert!(rendered.contains("ExecStart=/var/lib/bxdl/control/node1/attempts/attempt-123/bxdl-worker systemd-run --control /var/lib/bxdl/control/node1 --attempt attempt-123 --json\n"));
        for required in [
            "Type=exec\n",
            "SuccessExitStatus=143\n",
            "User=root\n",
            "Group=root\n",
            "Restart=no\n",
            "KillMode=control-group\n",
            "KillSignal=SIGTERM\n",
            "SendSIGKILL=no\n",
            "TimeoutStopSec=60\n",
            "NoNewPrivileges=yes\n",
            "UMask=0077\n",
            "Environment=LANG=C LC_ALL=C PATH=\n",
        ] {
            assert!(rendered.contains(required), "{required}");
        }
        assert!(rendered.contains("ExecStopPost=/var/lib/bxdl/control/node1/attempts/attempt-123/bxdl-worker systemd-exit --control /var/lib/bxdl/control/node1 --attempt attempt-123 --json\n"));
        for forbidden in [
            "ExecStartPre=",
            "ExecStop=",
            "[Install]",
            "WantedBy=",
            "Restart=always",
            "User=bxdl",
            "/bin/sh",
            "-jar",
            "EnvironmentFile=",
            "KillMode=process",
        ] {
            assert!(!rendered.contains(forbidden), "{forbidden}");
        }
    }
    #[test]
    fn fixed_mutations_are_only_reload_or_exact_asynchronous_unit() {
        assert_eq!(
            mutation_arguments(Action::Reload).unwrap(),
            [
                "--system",
                "--no-pager",
                "--no-ask-password",
                "daemon-reload"
            ]
        );
        let unit = unit();
        for (action, verb) in [
            (Action::Start(unit.name()), "start"),
            (Action::Stop(unit.name()), "stop"),
        ] {
            assert_eq!(
                mutation_arguments(action).unwrap(),
                [
                    "--system",
                    "--no-pager",
                    "--no-ask-password",
                    "--no-block",
                    verb,
                    "--",
                    unit.name()
                ]
            );
        }
        for name in [
            "sshd.service",
            "bxdl-*.service",
            "--all",
            "bxdl-a.service\n",
        ] {
            assert!(mutation_arguments(Action::Stop(name)).is_err());
        }
    }
    #[test]
    fn query_accepts_only_fixed_property_sets() {
        let unit = unit();
        let args = query_arguments(unit.name(), UNIT_PROPERTIES).unwrap();
        assert_eq!(args.last().unwrap(), unit.name());
        assert!(args.contains(&"--property=InvocationID".into()));
        assert!(args.contains(&"--property=ControlGroup".into()));
        assert!(!args.contains(&"--user".into()));
        assert!(query_arguments(unit.name(), &["Environment"]).is_err());
        assert!(query_arguments("sshd.service", UNIT_PROPERTIES).is_err());
    }
    #[test]
    fn loaded_config_refuses_dropins_shadowing_or_stale_manager() {
        let unit = unit();
        let raw = format!(
            "FragmentPath={}\nDropInPaths=\nNeedDaemonReload=no\nTransient=no\n",
            unit.path().display()
        );
        verify_configuration(raw.as_bytes(), &unit.path()).unwrap();
        for changed in [
            raw.replace(
                "DropInPaths=\n",
                "DropInPaths=/etc/systemd/system/override.conf\n",
            ),
            raw.replace("NeedDaemonReload=no", "NeedDaemonReload=yes"),
            raw.replace("Transient=no", "Transient=yes"),
            raw.replace("/etc/systemd/system/", "/run/systemd/system/"),
            format!("{raw}Transient=no\n"),
            raw.replace("Transient=no\n", ""),
        ] {
            assert!(verify_configuration(changed.as_bytes(), &unit.path()).is_err());
        }
    }
    #[test]
    fn exit_hook_requires_its_current_control_pid_before_gc() {
        let expected = expected();
        let raw = format!(
            "Id={}\nActiveState=deactivating\nSubState=stop-post\nControlPID=456\nMainPID=0\nInvocationID={}\nControlGroup={}\n",
            expected.unit, expected.invocation_id, expected.control_group
        );
        verify_exit_hook(raw.as_bytes(), &expected.unit, &expected.invocation_id, 456).unwrap();
        for changed in [
            raw.replace("ControlPID=456", "ControlPID=457"),
            raw.replace("MainPID=0", "MainPID=123"),
            raw.replace("ActiveState=deactivating", "ActiveState=inactive"),
            raw.replace("SubState=stop-post", "SubState=dead"),
            raw.replace(&expected.invocation_id, "abcdef1234567890abcdef1234567890"),
            raw.replace(&expected.control_group, "/system.slice/bxdl-other.service"),
            format!("{raw}MainPID=0\n"),
        ] {
            assert!(
                verify_exit_hook(
                    changed.as_bytes(),
                    &expected.unit,
                    &expected.invocation_id,
                    456
                )
                .is_err()
            );
        }
        assert!(
            verify_exit_hook(raw.as_bytes(), &expected.unit, &expected.invocation_id, 0).is_err()
        );
    }

    #[test]
    fn start_refuses_any_previous_invocation_or_remaining_cgroup() {
        let value = inactive();
        assert!(initial_unit(&value));
        let mut previous = value.clone();
        previous.invocation_id = expected().invocation_id;
        assert!(!initial_unit(&previous));
        let mut previous = value.clone();
        previous.control_group = expected().control_group;
        assert!(!initial_unit(&previous));
        let mut previous = value.clone();
        previous.main_pid = 123;
        assert!(!initial_unit(&previous));
        let mut previous = value.clone();
        previous.active_state = "failed".into();
        assert!(!initial_unit(&previous));
        let mut previous = value;
        previous.load_state = "not-found".into();
        assert!(!initial_unit(&previous));
    }
    #[test]
    fn internal_stop_capability_requires_exact_unit_cgroup_identity() {
        StopAuthorization::after_process_audit(expected()).unwrap();
        let mut value = expected();
        value.main_pid = 1;
        assert!(StopAuthorization::after_process_audit(value).is_err());
        let mut value = expected();
        value.invocation_id = "0".repeat(32);
        assert!(StopAuthorization::after_process_audit(value).is_err());
        let mut value = expected();
        value.control_group = "/system.slice/bxdl-other.service".into();
        assert!(StopAuthorization::after_process_audit(value).is_err());
    }
    #[test]
    fn deadlines_are_bounded_and_zero_is_never_accepted() {
        assert!(Deadline::new(Duration::ZERO).is_err());
        assert!(Deadline::new(Duration::from_secs(31)).is_err());
        assert!(
            Deadline::new(Duration::from_secs(1))
                .unwrap()
                .remaining()
                .unwrap()
                <= Duration::from_secs(1)
        );
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")))]
    #[test]
    fn unsupported_hosts_cannot_prepare_or_invoke_a_manager() {
        let unit = unit();
        let timeout = Duration::from_secs(1);
        for result in [
            unit.prepare(),
            unit.install(timeout),
            unit.start(timeout),
            unit.inspect(timeout).map(|_| ()),
            unit.inspect_exit_hook(&expected().invocation_id, std::process::id(), timeout),
            unit.stop(
                &StopAuthorization::after_process_audit(expected()).unwrap(),
                timeout,
            ),
        ] {
            assert_eq!(result.unwrap_err().code, "SERVICE_PLATFORM_UNSUPPORTED");
        }
    }
}
