//! User-scoped management CLI installation; independent of node packages/data.
pub mod bundle;
mod fs;
mod probe;
mod store;

use crate::error::{BxdlError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Locations {
    pub root: PathBuf,
    pub bin_dir: PathBuf,
}
#[derive(Clone, Debug)]
pub struct InstallOptions {
    pub locations: Locations,
    pub from: Option<PathBuf>,
    pub trust: bundle::VerifyOptions,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseInfo {
    pub release_id: String,
    pub version: String,
    pub revision: String,
    pub target: String,
    pub manifest_sha256: String,
    pub authenticity: String,
}
impl ReleaseInfo {
    fn from_bundle(b: &bundle::Bundle) -> Self {
        Self {
            release_id: b.release_id(),
            version: b.manifest.version.clone(),
            revision: b.manifest.revision.clone(),
            target: b.manifest.target.clone(),
            manifest_sha256: b.manifest_sha256.clone(),
            authenticity: b.authenticity.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub action: String,
    pub status: String,
    pub changed: bool,
    pub root: PathBuf,
    pub bin_dir: PathBuf,
    pub executable: PathBuf,
    pub current: Option<ReleaseInfo>,
    pub previous: Option<ReleaseInfo>,
    pub invoked_executable: Option<PathBuf>,
    pub path_status: String,
    pub shell_setup: Vec<String>,
    pub activation: Option<String>,
}

pub fn default_locations(root: Option<&Path>, bin_dir: Option<&Path>) -> Result<Locations> {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| {
                error(
                    "CLI_LOCATION_REQUIRED",
                    "사용자 홈을 확인할 수 없습니다. --root와 --bin-dir을 지정하세요.",
                )
            })
    };
    let root = match root {
        Some(p) => crate::setup::absolute(p)?,
        None => home()?.join("Library/Application Support/BXDL-CLI"),
    };
    let bin_dir = match bin_dir {
        Some(p) => crate::setup::absolute(p)?,
        None => match store::recorded_bin_dir(&root)? {
            Some(p) => p,
            None => home()?.join(".local/bin"),
        },
    };
    Ok(Locations { root, bin_dir })
}
pub fn install(o: &InstallOptions) -> Result<Report> {
    platform()?;
    store::install(o, false, probe::check)
}
pub fn update(o: &InstallOptions) -> Result<Report> {
    platform()?;
    if o.from.is_none() {
        return Err(error(
            "INVALID_ARGUMENTS",
            "update에는 --from 배포본을 지정하세요.",
        ));
    }
    store::install(o, true, probe::check)
}
pub fn info(l: &Locations) -> Result<Report> {
    store::info(l)
}
pub fn rollback(l: &Locations) -> Result<Report> {
    platform()?;
    store::rollback(l, probe::check)
}
fn platform() -> Result<()> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok(())
    } else {
        Err(error(
            "CLI_PLATFORM_UNSUPPORTED",
            "관리 CLI 설치·업데이트·복귀는 macOS arm64에서 지원합니다.",
        ))
    }
}
fn error(code: &str, message: &str) -> BxdlError {
    BxdlError::new(code, message)
}
fn report(
    l: &Locations,
    action: &str,
    status: &str,
    changed: bool,
    current: Option<ReleaseInfo>,
    previous: Option<ReleaseInfo>,
    activation: Option<String>,
) -> Report {
    let executable = l.bin_dir.join("bxdl");
    let selected = std::env::var_os("PATH")
        .filter(|s| s.len() <= 65536)
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .take(256)
                .map(|p| p.join("bxdl"))
                .find(|p| p.is_file())
        });
    let path_status = match selected {
        None => "NOT_ON_PATH",
        Some(p)
            if std::fs::canonicalize(&p).ok() == std::fs::canonicalize(&executable).ok()
                && executable.is_file() =>
        {
            "SELECTED"
        }
        Some(_) => "SHADOWED",
    };
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"));
    let shell_setup = vec![
        format!("export PATH={}:\"$PATH\"", quote(&l.bin_dir)),
        format!(
            "fpath=({} $fpath)",
            quote(&l.root.join("current/share/zsh/site-functions"))
        ),
        "autoload -Uz compinit".into(),
        "compinit".into(),
        "rehash".into(),
        "command -v bxdl".into(),
        "bxdl version --json".into(),
        "source <(bxdl completion zsh)".into(),
    ];
    Report {
        action: action.into(),
        status: status.into(),
        changed,
        root: l.root.clone(),
        bin_dir: l.bin_dir.clone(),
        executable,
        current,
        previous,
        invoked_executable: std::env::current_exe().ok(),
        path_status: path_status.into(),
        shell_setup,
        activation,
    }
}
