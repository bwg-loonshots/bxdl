//! Immutable releases and activation records. A single symlink rename selects
//! both executable and previous-version metadata. No node storage is opened.
use super::{
    InstallOptions, Locations, ReleaseInfo, Report, Result,
    bundle::{self, Bundle, VerifyOptions},
    error,
    fs::{self, Directory, Identity},
    report,
};
use cap_std::fs::{Dir, File, MetadataExt};
use ed25519_dalek::VerifyingKey;
use rustix::fs::{FlockOperation, flock};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MARKER: &str = ".bxdl-cli.json";
const LOCK: &str = ".lock";
const LIMIT: usize = 4096;
static COUNTER: AtomicU64 = AtomicU64::new(0);
type Probe = fn(&Path, &bundle::Manifest) -> Result<()>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Trust {
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key_hex: Option<String>,
}
impl Trust {
    fn initial(o: &VerifyOptions) -> Result<Self> {
        if o.public_key_path.is_some() == o.allow_unsigned_development {
            return Err(trust_error());
        }
        if let Some(p) = &o.public_key_path {
            let k = crate::artifact::load_public_key(p)?;
            Ok(Self {
                mode: "ed25519".into(),
                public_key_hex: Some(hex::encode(k.as_bytes())),
            })
        } else {
            Ok(Self {
                mode: "unsigned-development".into(),
                public_key_hex: None,
            })
        }
    }
    fn key(&self) -> Result<Option<VerifyingKey>> {
        match (self.mode.as_str(), self.public_key_hex.as_deref()) {
            ("unsigned-development", None) => Ok(None),
            ("ed25519", Some(text)) => {
                let bytes: [u8; 32] = hex::decode(text)
                    .map_err(|_| fs::unsafe_path())?
                    .try_into()
                    .map_err(|_| fs::unsafe_path())?;
                Ok(Some(
                    VerifyingKey::from_bytes(&bytes).map_err(|_| fs::unsafe_path())?,
                ))
            }
            _ => Err(fs::unsafe_path()),
        }
    }
    fn accept_options(&self, o: &VerifyOptions) -> Result<()> {
        if self.mode == "unsigned-development" {
            if !o.allow_unsigned_development || o.public_key_path.is_some() {
                return Err(trust_error());
            }
        } else {
            if o.allow_unsigned_development {
                return Err(trust_error());
            }
            if let Some(p) = &o.public_key_path {
                if Some(crate::artifact::load_public_key(p)?) != self.key()? {
                    return Err(trust_error());
                }
            }
        }
        Ok(())
    }
    fn verify(&self, p: &Path) -> Result<Bundle> {
        bundle::verify_with_key(p, self.key()?.as_ref(), self.mode == "unsigned-development")
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    kind: String,
    root: Identity,
    lock: Identity,
    releases: Identity,
    activations: Identity,
    staging: Identity,
    bin_dir: PathBuf,
    bin_identity: Identity,
    trust: Trust,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Admission {
    schema_version: u32,
    kind: String,
    release: ReleaseInfo,
    directory: Identity,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_sha256: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selection {
    schema_version: u32,
    kind: String,
    current: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<String>,
}
struct Root {
    home: Directory,
    bin: Directory,
    record: Record,
    releases: Dir,
    activations: Dir,
    staging: Dir,
}
struct Lock(File);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = flock(&self.0, FlockOperation::Unlock);
    }
}

fn json<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|_| fs::io())
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    crate::artifact::decode_strict_json(bytes).map_err(|_| fs::unsafe_path())
}
fn not_found(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
}
fn exists(d: &Dir, n: &str) -> Result<bool> {
    match d.symlink_metadata(n) {
        Ok(_) => Ok(true),
        Err(e) if not_found(&e) => Ok(false),
        Err(_) => Err(fs::unsafe_path()),
    }
}
fn identity(d: &Dir) -> Result<Identity> {
    Ok(Identity::of(&d.dir_metadata().map_err(|_| fs::io())?))
}
fn owned_bin(d: &Directory) -> Result<()> {
    let m = d.dir.dir_metadata().map_err(|_| fs::io())?;
    if m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o022 != 0 {
        return Err(fs::unsafe_path());
    }
    Ok(())
}
fn validate_locations(l: &Locations) -> Result<()> {
    if crate::setup::paths::overlaps(&l.root, &l.bin_dir)? {
        return Err(error(
            "CLI_LOCATION_CONFLICT",
            "CLI 설치 위치와 실행 링크 폴더는 서로 분리해야 합니다.",
        ));
    }
    if let Some(parent) = l.root.parent() {
        fs::check_destination(parent)?;
    }
    fs::check_destination(&l.bin_dir)?;
    Ok(())
}
pub(super) fn recorded_bin_dir(path: &Path) -> Result<Option<PathBuf>> {
    let Some(home) = fs::open(path, false, true)? else {
        return Ok(None);
    };
    let r: Record = decode(&fs::read(&home.dir, MARKER)?)?;
    if r.schema_version != 1 || r.kind != "BXDL_CLI_INSTALLATION" || r.root != home.id {
        return Err(fs::unsafe_path());
    }
    r.trust.key()?;
    home.recheck()?;
    Ok(Some(r.bin_dir))
}
impl Root {
    fn open(l: &Locations) -> Result<Option<Self>> {
        let Some(home) = fs::open(&l.root, false, true)? else {
            return Ok(None);
        };
        let r: Record = decode(&fs::read(&home.dir, MARKER)?)?;
        if r.schema_version != 1
            || r.kind != "BXDL_CLI_INSTALLATION"
            || r.root != home.id
            || r.bin_dir != l.bin_dir
        {
            return Err(fs::unsafe_path());
        }
        r.trust.key()?;
        let bin = fs::open(&r.bin_dir, false, false)?.ok_or_else(fs::unsafe_path)?;
        owned_bin(&bin)?;
        if bin.id != r.bin_identity {
            return Err(fs::unsafe_path());
        }
        let releases = fs::child(&home.dir, "releases")?;
        let activations = fs::child(&home.dir, "activations")?;
        let staging = fs::child(&home.dir, "staging")?;
        let root = Self {
            home,
            bin,
            record: r,
            releases,
            activations,
            staging,
        };
        root.recheck()?;
        Ok(Some(root))
    }
    fn create(l: &Locations, trust: Trust) -> Result<Self> {
        // Prepare metadata privately, then publish the entire root atomically.
        // A killed initializer can leave only an unselected temporary sibling.
        if std::fs::symlink_metadata(&l.root).is_ok() {
            return Err(fs::unsafe_path());
        }
        let parent = fs::open(l.root.parent().ok_or_else(fs::unsafe_path)?, true, false)?
            .ok_or_else(fs::io)?;
        fs::reject_product_root(&parent.dir)?;
        let name = l
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(fs::unsafe_path)?;
        let bin = fs::open(&l.bin_dir, true, false)?.ok_or_else(fs::io)?;
        owned_bin(&bin)?;
        if exists(&bin.dir, "bxdl")? {
            return Err(entry_conflict());
        }
        let temp = unique(".bxdl-cli-install");
        let dir = fs::mkdir(&parent.dir, &temp)?;
        let id = identity(&dir)?;
        let outcome = (|| {
            let releases = fs::mkdir(&dir, "releases")?;
            let activations = fs::mkdir(&dir, "activations")?;
            let staging = fs::mkdir(&dir, "staging")?;
            fs::write_new(&dir, LOCK, b"", 0o600)?;
            let r = Record {
                schema_version: 1,
                kind: "BXDL_CLI_INSTALLATION".into(),
                root: id,
                lock: Identity::of(&dir.symlink_metadata(LOCK).map_err(|_| fs::io())?),
                releases: identity(&releases)?,
                activations: identity(&activations)?,
                staging: identity(&staging)?,
                bin_dir: l.bin_dir.clone(),
                bin_identity: bin.id,
                trust,
            };
            fs::write_new(&dir, MARKER, &json(&r)?, 0o600)?;
            parent.recheck()?;
            bin.recheck()?;
            fs::reject_product_root(&parent.dir)?;
            fs::publish(&parent.dir, &temp, &parent.dir, name)?;
            fs::sync(&parent.dir)?;
            Self::open(l)?.ok_or_else(fs::io)
        })();
        if parent
            .dir
            .symlink_metadata(&temp)
            .is_ok_and(|m| Identity::of(&m) == id)
        {
            let _ = parent.dir.remove_dir_all(&temp);
            let _ = fs::sync(&parent.dir);
        }
        outcome
    }
    fn recheck(&self) -> Result<()> {
        self.home.recheck()?;
        self.bin.recheck()?;
        owned_bin(&self.bin)?;
        let r: Record = decode(&fs::read(&self.home.dir, MARKER)?)?;
        if r != self.record {
            return Err(fs::unsafe_path());
        }
        let lock = self
            .home
            .dir
            .symlink_metadata(LOCK)
            .map_err(|_| fs::unsafe_path())?;
        fs::private_file(&lock)?;
        if lock.len() != 0 || Identity::of(&lock) != r.lock {
            return Err(fs::unsafe_path());
        }
        for (name, opened, expected) in [
            ("releases", &self.releases, r.releases),
            ("activations", &self.activations, r.activations),
            ("staging", &self.staging, r.staging),
        ] {
            let visible = fs::child(&self.home.dir, name)?;
            if identity(&visible)? != expected || identity(opened)? != expected {
                return Err(fs::unsafe_path());
            }
        }
        for (index, entry) in self.home.dir.entries().map_err(|_| fs::io())?.enumerate() {
            if index >= 64 {
                return Err(limit());
            }
            let name = entry.map_err(|_| fs::io())?.file_name();
            let text = name.to_str().ok_or_else(fs::unsafe_path)?;
            if ![
                MARKER,
                LOCK,
                "releases",
                "activations",
                "staging",
                "current",
            ]
            .contains(&text)
                && !text.starts_with(".current-")
            {
                return Err(fs::unsafe_path());
            }
        }
        Ok(())
    }
    fn lock(&self) -> Result<Lock> {
        self.recheck()?;
        let f = fs::lock_file(&self.home.dir, LOCK)?;
        if Identity::of(&f.metadata().map_err(|_| fs::io())?) != self.record.lock {
            return Err(fs::unsafe_path());
        }
        flock(&f, FlockOperation::NonBlockingLockExclusive).map_err(|_| {
            error(
                "CLI_BUSY",
                "다른 관리 CLI 설치·업데이트가 진행 중입니다. 종료 뒤 다시 확인하세요.",
            )
        })?;
        self.recheck()?;
        Ok(Lock(f))
    }
    fn entry(&self, allow_missing: bool) -> Result<()> {
        match self.bin.dir.symlink_metadata("bxdl") {
            Err(e) if not_found(&e) && allow_missing => Ok(()),
            Ok(m)
                if m.is_symlink()
                    && m.uid() == rustix::process::geteuid().as_raw()
                    && self.bin.dir.read_link_contents("bxdl").ok().as_ref()
                        == Some(&self.home.path.join("current/bin/bxdl")) =>
            {
                Ok(())
            }
            _ => Err(entry_conflict()),
        }
    }
    fn ensure_entry(&self) -> Result<()> {
        self.entry(true)?;
        if !exists(&self.bin.dir, "bxdl")? {
            self.bin
                .dir
                .symlink_contents(self.home.path.join("current/bin/bxdl"), "bxdl")
                .map_err(|_| entry_conflict())?;
            fs::sync(&self.bin.dir)?;
        }
        self.entry(false)
    }
    fn selection(&self) -> Result<Option<(String, Selection)>> {
        let m = match self.home.dir.symlink_metadata("current") {
            Err(e) if not_found(&e) => return Ok(None),
            Err(_) => return Err(fs::unsafe_path()),
            Ok(m) => m,
        };
        if !m.is_symlink() || m.uid() != rustix::process::geteuid().as_raw() {
            return Err(fs::unsafe_path());
        }
        let link = self
            .home
            .dir
            .read_link_contents("current")
            .map_err(|_| fs::unsafe_path())?;
        let text = link.to_str().ok_or_else(fs::unsafe_path)?;
        let id = text
            .strip_prefix("activations/")
            .filter(|s| {
                s.len() == 16
                    && s.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
            .ok_or_else(fs::unsafe_path)?;
        let dir = fs::child(&self.activations, id)?;
        let s: Selection = decode(&fs::read(&dir, "selection.json")?)?;
        if s.schema_version != 1
            || s.kind != "BXDL_CLI_ACTIVATION"
            || !release_id_valid(&s.current)
            || s.previous
                .as_ref()
                .is_some_and(|p| !release_id_valid(p) || p == &s.current)
        {
            return Err(fs::unsafe_path());
        }
        for name in ["bin", "share"] {
            let m = dir.symlink_metadata(name).map_err(|_| fs::unsafe_path())?;
            if !m.is_symlink()
                || m.uid() != rustix::process::geteuid().as_raw()
                || dir.read_link_contents(name).ok()
                    != Some(PathBuf::from(format!(
                        "../../releases/{}/payload/{name}",
                        s.current
                    )))
            {
                return Err(fs::unsafe_path());
            }
        }
        let mut count = 0;
        for item in dir.entries().map_err(|_| fs::io())? {
            let n = item.map_err(|_| fs::io())?.file_name();
            if !["selection.json", "bin", "share"].iter().any(|p| n == *p) {
                return Err(fs::unsafe_path());
            }
            count += 1;
        }
        if count != 3 || self.home.dir.read_link_contents("current").ok() != Some(link.clone()) {
            return Err(fs::unsafe_path());
        }
        self.recheck()?;
        Ok(Some((id.into(), s)))
    }
    fn admission(&self, id: &str) -> Result<Admission> {
        if !release_id_valid(id) {
            return Err(fs::unsafe_path());
        }
        let directory = fs::child(&self.releases, id)?;
        let a: Admission = decode(&fs::read(&directory, "admission.json")?)?;
        if a.directory != identity(&directory)? {
            return Err(changed());
        }
        for (i, item) in directory.entries().map_err(|_| fs::io())?.enumerate() {
            let name = item.map_err(|_| fs::io())?.file_name();
            if i >= 2 || (name != "payload" && name != "admission.json") {
                return Err(changed());
            }
        }
        if a.schema_version != 1
            || a.kind != "BXDL_CLI_ADMISSION"
            || a.release.release_id != id
            || id != format!("{}-{}", a.release.version, a.release.revision)
        {
            return Err(fs::unsafe_path());
        }
        Ok(a)
    }
    fn admitted(&self, id: &str) -> Result<(Admission, Bundle)> {
        let a = self.admission(id)?;
        let directory = fs::child(&self.releases, id)?;
        if identity(&directory)? != a.directory {
            return Err(changed());
        }
        let b = bundle::verify_integrity(&self.home.path.join("releases").join(id).join("payload"))
            .map_err(|_| changed())?;
        let mut expected = ReleaseInfo::from_bundle(&b);
        expected.authenticity = a.release.authenticity.clone();
        if a.release != expected || a.signature_sha256 != signature_hash(&b) {
            return Err(changed());
        }
        self.recheck()?;
        Ok((a, b))
    }
    fn stage(&self, b: &Bundle, probe: Probe) -> Result<Admission> {
        let id = b.release_id();
        if exists(&self.releases, &id)? {
            let (a, _) = self.admitted(&id)?;
            if a.release.manifest_sha256 != b.manifest_sha256
                || a.signature_sha256 != signature_hash(b)
            {
                return Err(error(
                    "CLI_RELEASE_CONFLICT",
                    "같은 버전·revision의 배포 내용이 다릅니다. 새로운 release identity를 사용하세요.",
                ));
            }
            probe(
                &self
                    .home
                    .path
                    .join("releases")
                    .join(&id)
                    .join("payload/bin/bxdl"),
                &b.manifest,
            )?;
            self.admitted(&id)?;
            return Ok(a);
        }
        bounded_count(&self.releases, 256)?;
        bounded_count(&self.staging, 64)?;
        let stage = unique("stage");
        let dir = fs::mkdir(&self.staging, &stage)?;
        let payload = fs::mkdir(&dir, "payload")?;
        let stage_path = self.home.path.join("staging").join(&stage).join("payload");
        let outcome = (|| {
            let bin = fs::mkdir(&payload, "bin")?;
            let docs = fs::mkdir(&payload, "docs")?;
            let share = fs::mkdir(&payload, "share")?;
            let zsh = fs::mkdir(&share, "zsh")?;
            let completion = fs::mkdir(&zsh, "site-functions")?;
            for (path, bytes) in &b.files {
                let (parent, name, mode) = match path.as_str() {
                    "bin/bxdl" => (&bin, "bxdl", 0o755),
                    "docs/management-cli.md" => (&docs, "management-cli.md", 0o644),
                    "docs/completion.md" => (&docs, "completion.md", 0o644),
                    "share/zsh/site-functions/_bxdl" => (&completion, "_bxdl", 0o644),
                    _ => return Err(fs::unsafe_path()),
                };
                fs::write_new(parent, name, bytes, mode)?;
            }
            fs::write_new(&payload, "manifest.json", &b.manifest_bytes, 0o644)?;
            if let Some(sig) = &b.signature {
                fs::write_new(&payload, "manifest.sig", sig, 0o644)?;
            }
            let check = bundle::verify_integrity(&stage_path)?;
            if check.manifest_sha256 != b.manifest_sha256
                || signature_hash(&check) != signature_hash(b)
            {
                return Err(changed());
            }
            probe(&stage_path.join("bin/bxdl"), &b.manifest)?;
            let check = bundle::verify_integrity(&stage_path)?;
            if check.manifest_sha256 != b.manifest_sha256
                || signature_hash(&check) != signature_hash(b)
            {
                return Err(changed());
            }
            self.recheck()?;
            if exists(&self.releases, &id)? {
                return Err(error(
                    "CLI_RELEASE_CONFLICT",
                    "설치 중 같은 release 경로가 나타났습니다. 기존 파일을 보존했습니다.",
                ));
            }
            let a = Admission {
                schema_version: 1,
                kind: "BXDL_CLI_ADMISSION".into(),
                release: ReleaseInfo::from_bundle(b),
                directory: identity(&dir)?,
                signature_sha256: signature_hash(b),
            };
            fs::write_new(&dir, "admission.json", &json(&a)?, 0o600)?;
            fs::publish(&self.staging, &stage, &self.releases, &id)?;
            fs::sync(&self.releases)?;
            fs::sync(&self.staging)?;
            self.admitted(&id)?;
            Ok(a)
        })();
        // Only the private staging directory created by this call is removed.
        // A published release is never erased or overwritten on failure.
        if self.staging.symlink_metadata(&stage).is_ok_and(|m| {
            Identity::of(&m)
                == identity(&dir).unwrap_or(Identity {
                    device: 0,
                    inode: 0,
                })
        }) {
            let _ = self.staging.remove_dir_all(&stage);
            let _ = fs::sync(&self.staging);
        }
        outcome
    }
    fn activate(
        &self,
        current: &str,
        previous: Option<String>,
        expected: &Option<(String, Selection)>,
    ) -> Result<String> {
        self.recheck()?;
        self.entry(true)?;
        if self.selection()? != *expected {
            return Err(error(
                "CLI_SELECTION_CHANGED",
                "다른 CLI 선택을 확인했습니다. 다시 cli info로 확인하세요.",
            ));
        }
        let sequence = next_activation(&self.activations)?;
        let name = format!("{sequence:016x}");
        let dir = fs::mkdir(&self.activations, &name)?;
        let selection = Selection {
            schema_version: 1,
            kind: "BXDL_CLI_ACTIVATION".into(),
            current: current.into(),
            previous,
        };
        fs::write_new(&dir, "selection.json", &json(&selection)?, 0o600)?;
        for component in ["bin", "share"] {
            dir.symlink_contents(
                format!("../../releases/{current}/payload/{component}"),
                component,
            )
            .map_err(|_| fs::io())?;
        }
        fs::sync(&dir)?;
        fs::sync(&self.activations)?;
        self.admitted(current)?;
        self.ensure_entry()?;
        self.recheck()?;
        if self.selection()? != *expected {
            return Err(error(
                "CLI_SELECTION_CHANGED",
                "CLI 선택이 변경되어 전환을 중단했습니다.",
            ));
        }
        let temp = unique(".current");
        self.home
            .dir
            .symlink_contents(format!("activations/{name}"), &temp)
            .map_err(|_| fs::io())?;
        fs::sync(&self.home.dir)?;
        if let Err(_e) = self.home.dir.rename(&temp, &self.home.dir, "current") {
            let _ = self.home.dir.remove_file(&temp);
            return Err(fs::io());
        }
        // From this point, report uncertainty instead of claiming no change.
        if fs::sync(&self.home.dir).is_err()
            || self.selection().ok().flatten() != Some((name.clone(), selection))
            || self.entry(false).is_err()
        {
            return Err(error(
                "CLI_ACTIVATION_UNKNOWN",
                "CLI 선택 전환 뒤 최종 확인을 마치지 못했습니다. 다시 업데이트하지 말고 cli info로 현재 선택을 확인하세요.",
            ));
        }
        Ok(name)
    }
}
fn unique(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
fn signature_hash(b: &Bundle) -> Option<String> {
    b.signature.as_ref().map(|s| hex::encode(Sha256::digest(s)))
}
fn release_id_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 180
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b))
        && !s.starts_with('.')
        && !s.contains("..")
}
fn bounded_count(d: &Dir, max: usize) -> Result<()> {
    for (i, e) in d.entries().map_err(|_| fs::io())?.enumerate() {
        e.map_err(|_| fs::io())?;
        if i >= max {
            return Err(limit());
        }
    }
    Ok(())
}
fn next_activation(d: &Dir) -> Result<u64> {
    let mut next = 1;
    for (i, e) in d.entries().map_err(|_| fs::io())?.enumerate() {
        if i >= LIMIT {
            return Err(limit());
        }
        let n = e.map_err(|_| fs::io())?.file_name();
        let s = n.to_str().ok_or_else(fs::unsafe_path)?;
        if s.len() != 16 {
            return Err(fs::unsafe_path());
        }
        let seq = u64::from_str_radix(s, 16).map_err(|_| fs::unsafe_path())?;
        next = next.max(seq.checked_add(1).ok_or_else(limit)?);
    }
    Ok(next)
}
fn source(o: &InstallOptions) -> Result<PathBuf> {
    if let Some(p) = &o.from {
        return crate::setup::absolute(p);
    }
    let exe = std::env::current_exe()
        .map_err(|_| fs::io())?
        .canonicalize()
        .map_err(|_| fs::io())?;
    if exe.file_name().and_then(|s| s.to_str()) != Some("bxdl")
        || exe
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            != Some("bin")
    {
        return Err(error(
            "CLI_BUNDLE_REQUIRED",
            "배포본의 bin/bxdl에서 실행하거나 --from으로 배포본을 지정하세요.",
        ));
    }
    let root = exe
        .parent()
        .and_then(Path::parent)
        .ok_or_else(fs::unsafe_path)?;
    if !root.join("manifest.json").is_file() {
        return Err(error(
            "CLI_BUNDLE_REQUIRED",
            "배포본의 bin/bxdl에서 실행하거나 --from으로 배포본을 지정하세요.",
        ));
    }
    Ok(root.into())
}
pub(super) fn install(o: &InstallOptions, update: bool, probe: Probe) -> Result<Report> {
    let l = &o.locations;
    validate_locations(l)?;
    let existing = Root::open(l)?;
    if update && existing.is_none() {
        return Err(not_installed());
    }
    if let Some(root) = &existing {
        root.entry(!update)?;
    } else if let Some(bin) = fs::open(&l.bin_dir, false, false)? {
        owned_bin(&bin)?;
        if exists(&bin.dir, "bxdl")? {
            return Err(entry_conflict());
        }
    }
    let trust = match &existing {
        Some(r) => {
            r.record.trust.accept_options(&o.trust)?;
            r.record.trust.clone()
        }
        None => Trust::initial(&o.trust)?,
    };
    let supply = source(o)?;
    if crate::setup::paths::overlaps(&l.root, &supply)? {
        return Err(error(
            "CLI_LOCATION_CONFLICT",
            "CLI 설치 위치와 새 배포 원본은 분리해야 합니다. 외부 배포본을 --from으로 지정하세요.",
        ));
    }
    let bundle = trust.verify(&supply)?;
    let root = match existing {
        Some(r) => r,
        None => Root::create(l, trust)?,
    };
    let _lock = root.lock()?;
    let old = root.selection()?;
    if update && old.is_none() {
        return Err(not_installed());
    }
    if let Some((activation, s)) = &old {
        if s.current == bundle.release_id() {
            let (a, _) = root.admitted(&s.current)?;
            if a.release.manifest_sha256 != bundle.manifest_sha256
                || a.signature_sha256 != signature_hash(&bundle)
            {
                return Err(error(
                    "CLI_RELEASE_CONFLICT",
                    "같은 버전·revision의 배포 내용이 다릅니다. 새 identity를 사용하세요.",
                ));
            }
            let missing = !exists(&root.bin.dir, "bxdl")?;
            root.ensure_entry()?;
            let previous = s
                .previous
                .as_ref()
                .map(|p| root.admission(p).map(|a| a.release))
                .transpose()?;
            return Ok(report(
                l,
                if update { "update" } else { "install" },
                "UNCHANGED",
                missing,
                Some(a.release),
                previous,
                Some(activation.clone()),
            ));
        }
        if !update {
            return Err(error(
                "CLI_ALREADY_INSTALLED",
                "다른 관리 CLI가 설치되어 있습니다. cli update --from으로 버전을 변경하세요.",
            ));
        }
    }
    let admission = root.stage(&bundle, probe)?;
    let previous = old.as_ref().map(|(_, s)| s.current.clone());
    let activation = root.activate(&admission.release.release_id, previous.clone(), &old)?;
    let previous = previous
        .map(|p| root.admission(&p).map(|a| a.release))
        .transpose()
        .map_err(|_| {
            error(
                "CLI_ACTIVATION_UNKNOWN",
                "CLI 전환 뒤 이전 버전 정보를 확인하지 못했습니다. cli info로 확인하세요.",
            )
        })?;
    Ok(report(
        l,
        if update { "update" } else { "install" },
        if update { "UPDATED" } else { "INSTALLED" },
        true,
        Some(admission.release),
        previous,
        Some(activation),
    ))
}
pub(super) fn info(l: &Locations) -> Result<Report> {
    let Some(root) = Root::open(l)? else {
        return Ok(report(l, "info", "NOT_INSTALLED", false, None, None, None));
    };
    let selection = root.selection()?;
    let Some((activation, s)) = selection else {
        return Ok(report(l, "info", "NOT_INSTALLED", false, None, None, None));
    };
    root.entry(false)?;
    let (current, _) = root.admitted(&s.current)?;
    let previous = s
        .previous
        .as_ref()
        .map(|p| root.admission(p).map(|a| a.release))
        .transpose()?;
    if root.selection()? != Some((activation.clone(), s)) {
        return Err(error(
            "CLI_SELECTION_CHANGED",
            "조회 중 CLI 선택이 바뀌었습니다. 다시 조회하세요.",
        ));
    }
    Ok(report(
        l,
        "info",
        "READY",
        false,
        Some(current.release),
        previous,
        Some(activation),
    ))
}
pub(super) fn rollback(l: &Locations, probe: Probe) -> Result<Report> {
    validate_locations(l)?;
    let root = Root::open(l)?.ok_or_else(not_installed)?;
    let _lock = root.lock()?;
    root.entry(false)?;
    let old = root.selection()?.ok_or_else(not_installed)?;
    let previous = old
        .1
        .previous
        .clone()
        .ok_or_else(|| error("CLI_PREVIOUS_MISSING", "복귀할 이전 관리 CLI가 없습니다."))?;
    let (candidate, b) = root.admitted(&previous)?;
    probe(
        &l.root
            .join("releases")
            .join(&previous)
            .join("payload/bin/bxdl"),
        &b.manifest,
    )?;
    let current = root.admission(&old.1.current)?.release;
    let activation = root.activate(&previous, Some(old.1.current.clone()), &Some(old))?;
    Ok(report(
        l,
        "rollback",
        "ROLLED_BACK",
        true,
        Some(candidate.release),
        Some(current),
        Some(activation),
    ))
}
fn trust_error() -> crate::error::BxdlError {
    error(
        "CLI_TRUST_POLICY",
        "최초 설치의 외부 공개키 또는 명시 개발 정책을 확인하세요. 신뢰키 변경은 허용하지 않으며 무서명 개발 설치·업데이트에는 매번 --allow-unsigned-development가 필요합니다.",
    )
}
fn entry_conflict() -> crate::error::BxdlError {
    error(
        "CLI_ENTRY_CONFLICT",
        "실행 경로의 bxdl이 이 설치의 링크가 아니거나 변경됐습니다. 기존 파일은 덮어쓰지 않았습니다.",
    )
}
fn not_installed() -> crate::error::BxdlError {
    error(
        "CLI_NOT_INSTALLED",
        "설치된 관리 CLI가 없습니다. cli install로 먼저 설치하세요.",
    )
}
fn changed() -> crate::error::BxdlError {
    error(
        "CLI_RELEASE_CHANGED",
        "설치된 CLI 파일이 검증 기록과 다릅니다. 기존 자료를 보존하고 신뢰한 배포본을 확인하세요.",
    )
}
fn limit() -> crate::error::BxdlError {
    error(
        "CLI_STORAGE_LIMIT",
        "관리 CLI 보관 이력 한도를 초과했습니다. 기존 자료를 자동 삭제하지 않았습니다.",
    )
}

#[cfg(test)]
mod tests;
