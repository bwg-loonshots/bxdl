//! Dedicated non-login accounts, with durable intent before fixed OS tools.
use super::*;
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum Phase {
    #[default]
    New,
    GroupIntent,
    GroupReady,
    UserIntent,
    Ready,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct State {
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
}
#[derive(Debug, PartialEq, Eq)]
enum Found {
    Absent,
    Group(u32),
    Ready(u32, u32),
}
fn conflict() -> BxdlError {
    install_error("LINUX_INSTALL_ACCOUNT_CONFLICT")
}
fn number(raw: &str) -> Result<u32> {
    let n = raw.parse::<u32>().map_err(|_| conflict())?;
    if matches!(n, 0 | u32::MAX) {
        return Err(conflict());
    }
    Ok(n)
}
fn parse(name: &str, passwd: &str, groups: &str, shadow: &str) -> Result<Found> {
    let users = passwd
        .lines()
        .map(|l| l.split(':').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let groups = groups
        .lines()
        .map(|l| l.split(':').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if users.iter().any(|u| u.len() != 7) || groups.iter().any(|g| g.len() != 4) {
        return Err(conflict());
    }
    let matching_users = users.iter().filter(|u| u[0] == name).collect::<Vec<_>>();
    let matching_groups = groups.iter().filter(|g| g[0] == name).collect::<Vec<_>>();
    if matching_users.is_empty() && matching_groups.is_empty() {
        return Ok(Found::Absent);
    }
    if matching_users.len() > 1 || matching_groups.len() != 1 {
        return Err(conflict());
    }
    let group = matching_groups[0];
    let gid = number(group[2])?;
    if !group[3].is_empty()
        || groups
            .iter()
            .filter(|g| g[2].parse::<u32>().ok() == Some(gid))
            .count()
            != 1
        || users
            .iter()
            .any(|u| u[0] != name && u[3].parse::<u32>().ok() == Some(gid))
    {
        return Err(conflict());
    }
    if matching_users.is_empty() {
        return Ok(Found::Group(gid));
    }
    let user = matching_users[0];
    let uid = number(user[2])?;
    if number(user[3])? != gid
        || user[5] != "/nonexistent"
        || !matches!(user[6], "/usr/sbin/nologin" | "/sbin/nologin")
        || users
            .iter()
            .filter(|u| u[2].parse::<u32>().ok() == Some(uid))
            .count()
            != 1
    {
        return Err(conflict());
    }
    let secrets = shadow
        .lines()
        .map(|l| l.split(':').collect::<Vec<_>>())
        .filter(|p| p.first() == Some(&name))
        .collect::<Vec<_>>();
    if secrets.len() != 1 || secrets[0].len() != 9 || !secrets[0][1].starts_with(['!', '*']) {
        return Err(conflict());
    }
    // The account must not be supplementary member of another local group.
    if groups
        .iter()
        .any(|g| g[3].split(',').any(|member| member == name))
    {
        return Err(conflict());
    }
    Ok(Found::Ready(uid, gid))
}
fn inspect(name: &str) -> Result<Found> {
    let read = |path| {
        String::from_utf8(boundary::root_read(Path::new(path), 1024 * 1024)?)
            .map_err(|_| conflict())
    };
    parse(
        name,
        &read("/etc/passwd")?,
        &read("/etc/group")?,
        &read("/etc/shadow")?,
    )
}
fn command(name: &str, user: bool) -> Command {
    let mut c = Command::new(if user {
        "/usr/sbin/useradd"
    } else {
        "/usr/sbin/groupadd"
    });
    c.env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/sbin:/usr/bin")
        .current_dir("/");
    if user {
        c.args([
            "--system",
            "--gid",
            name,
            "--no-user-group",
            "--home-dir",
            "/nonexistent",
            "--no-create-home",
            "--shell",
            "/usr/sbin/nologin",
            "--no-log-init",
            "--password",
            "!",
            "--",
            name,
        ]);
    } else {
        c.args(["--system", "--", name]);
    }
    c
}
fn run(name: &str, user: bool, guard: &File, timeout: Duration) -> Result<()> {
    let mut cmd = command(name, user);
    super::trusted_executable(Path::new(cmd.get_program()))?;
    cmd.stdin(Stdio::from(guard.try_clone().map_err(|_| conflict())?));
    let (exit, _) = super::capture_command(cmd, timeout)?;
    if exit != 0 {
        return Err(install_error("LINUX_INSTALL_UNKNOWN"));
    }
    Ok(())
}
fn own_ready(state: &State, uid: u32, gid: u32) -> Result<()> {
    let permitted = match state.phase {
        Phase::UserIntent => state.uid.is_none() && state.gid == Some(gid),
        Phase::Ready => state.uid == Some(uid) && state.gid == Some(gid),
        _ => false,
    };
    if !permitted {
        return Err(conflict());
    }
    Ok(())
}
pub(super) fn provision(
    control: &Path,
    state: &mut State,
    guard: &File,
    timeout: Duration,
    mut save: impl FnMut(&State) -> Result<()>,
) -> Result<(u32, u32)> {
    boundary::root()?;
    let name = service_account(control)?;
    let found = inspect(&name)?;
    if let Found::Ready(uid, gid) = found {
        own_ready(state, uid, gid)?;
        state.phase = Phase::Ready;
        state.uid = Some(uid);
        state.gid = Some(gid);
        save(state)?;
        boundary::account(control, uid, gid)?;
        return Ok((uid, gid));
    }
    if state.phase == Phase::Ready {
        return Err(conflict());
    }
    let gid = match found {
        Found::Absent if state.phase == Phase::New => {
            state.phase = Phase::GroupIntent;
            save(state)?;
            run(&name, false, guard, timeout)?;
            match inspect(&name)? {
                Found::Group(gid) => gid,
                _ => return Err(conflict()),
            }
        }
        Found::Group(gid)
            if matches!(
                state.phase,
                Phase::GroupIntent | Phase::GroupReady | Phase::UserIntent
            ) =>
        {
            gid
        }
        _ => return Err(install_error("LINUX_INSTALL_PARTIAL")),
    };
    if state.gid.is_some_and(|n| n != gid) {
        return Err(conflict());
    }
    // An unresolved useradd intent is never automatically repeated.
    if state.phase == Phase::UserIntent {
        return Err(install_error("LINUX_INSTALL_PARTIAL"));
    }
    state.gid = Some(gid);
    state.phase = Phase::GroupReady;
    save(state)?;
    state.phase = Phase::UserIntent;
    save(state)?;
    run(&name, true, guard, timeout)?;
    let (uid, actual_gid) = match inspect(&name)? {
        Found::Ready(uid, gid) => (uid, gid),
        _ => return Err(conflict()),
    };
    if actual_gid != gid {
        return Err(conflict());
    }
    boundary::account(control, uid, gid)?;
    state.uid = Some(uid);
    state.phase = Phase::Ready;
    save(state)?;
    Ok((uid, gid))
}
#[cfg(test)]
mod tests {
    use super::*;
    const P: &str =
        "root:x:0:0:root:/root:/bin/bash\nbxdl-a:x:998:997::/nonexistent:/usr/sbin/nologin\n";
    const G: &str = "root:x:0:\nbxdl-a:x:997:\n";
    const S: &str = "bxdl-a:!:20000:0:99999:7:::\n";
    #[test]
    fn exact_dedicated_locked_account_only() {
        assert_eq!(parse("bxdl-a", P, G, S).unwrap(), Found::Ready(998, 997));
        for changed in [
            P.replace("/nonexistent", "/home/operator"),
            P.replace("/usr/sbin/nologin", "/bin/bash"),
            format!("{P}other:x:998:999::/none:/bin/false\n"),
            format!("{P}other:x:999:997::/none:/bin/false\n"),
        ] {
            assert!(parse("bxdl-a", &changed, G, S).is_err());
        }
        assert!(parse("bxdl-a", P, G, &S.replace("!", "hash")).is_err());
        assert!(parse("bxdl-a", P, &format!("{G}sudo:x:27:bxdl-a\n"), S).is_err());
    }
    #[test]
    fn fixed_account_tool_arguments() {
        let c = command("bxdl-a", true);
        assert_eq!(c.get_program(), "/usr/sbin/useradd");
        assert!(c.get_args().any(|v| v == "--no-create-home"));
        assert!(c.get_args().any(|v| v == "--no-log-init"));
        assert!(!c.get_args().any(|v| v == "--uid"));
    }
    #[test]
    fn pending_records_round_trip_without_null() {
        for phase in [
            Phase::New,
            Phase::GroupIntent,
            Phase::GroupReady,
            Phase::UserIntent,
            Phase::Ready,
        ] {
            let state = State {
                phase,
                uid: None,
                gid: None,
            };
            let bytes = encoded(&state).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("null"));
            let _: State = json::decode(&bytes).unwrap();
        }
    }
    #[test]
    fn complete_account_requires_this_transactions_prior_user_intent() {
        for phase in [Phase::New, Phase::GroupIntent, Phase::GroupReady] {
            assert!(
                own_ready(
                    &State {
                        phase,
                        uid: None,
                        gid: Some(997)
                    },
                    998,
                    997
                )
                .is_err()
            );
        }
        let pending = State {
            phase: Phase::UserIntent,
            uid: None,
            gid: Some(997),
        };
        own_ready(&pending, 998, 997).unwrap();
        assert!(own_ready(&pending, 998, 996).is_err());
        assert!(
            own_ready(
                &State {
                    phase: Phase::UserIntent,
                    uid: None,
                    gid: None
                },
                998,
                997
            )
            .is_err()
        );
        let complete = State {
            phase: Phase::Ready,
            uid: Some(998),
            gid: Some(997),
        };
        own_ready(&complete, 998, 997).unwrap();
        assert!(own_ready(&complete, 999, 997).is_err());
    }
}
