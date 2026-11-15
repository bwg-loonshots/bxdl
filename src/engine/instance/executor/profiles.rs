//! Frozen runtime pins. The full received engine identity is compiled in;
//! no semver inference, mutable lock lookup, or version probe selects a profile.
use crate::{artifact, engine::Identity};

pub(super) struct Profile {
    pub archive: &'static str,
    pub manifest: &'static str,
    pub cli: &'static str,
    pub cli_size: u64,
    pub jar: &'static str,
    pub jar_size: u64,
    pub java: &'static str,
    pub product_revision: &'static str,
    pub files: usize,
    pub bytes: u64,
    pub expected_json: &'static str,
}

// RC.3 remains byte-for-byte the previously accepted runtime.
pub(super) static RC3: Profile = Profile {
    archive: "8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017",
    manifest: "2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123",
    cli: "b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef",
    cli_size: 3_024_064,
    jar: "05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f",
    jar_size: 164_088_530,
    java: "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c",
    product_revision: "53bb11b90d9d51aba5a7b509074046fd6ee78401",
    files: 270,
    bytes: 325_299_323,
    expected_json: include_str!(
        "../../../../contracts/nigo/engine-rc3-2026-10-22/consumer-evidence/engine-info.json"
    ),
};

// RC4-03 froze A before assembling R. This external manager may pin R without
// rebuilding or replacing the executor already inside that signed archive.
pub(super) static RC4: Profile = Profile {
    archive: "a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60",
    manifest: "fe21e898f5ace8be57ad838620f14da1c3e07b3ffcfbcb969fd63a67854a142e",
    cli: "d2d5c586bf2ab59ac3198cbcc0f89101fc18d31e6f4ec7812b428e481a63f647",
    cli_size: 4_219_840,
    jar: "56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a",
    jar_size: 164_133_636,
    java: "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c",
    product_revision: "d38027a474f1e8d4fb7b1d38ea17972ba88d93af",
    files: 269,
    bytes: 326_373_420,
    expected_json: include_str!(
        "../../../../contracts/nigo/engine-rc4-2026-11-15/consumer-evidence/engine-info.json"
    ),
};

pub(super) fn for_archive(archive: &str) -> Option<&'static Profile> {
    [&RC3, &RC4]
        .into_iter()
        .find(|profile| profile.archive == archive)
}

impl Profile {
    fn expected(&self) -> Option<Identity> {
        super::json::decode(self.expected_json.as_bytes()).ok()
    }

    pub fn matches_identity(&self, identity: &Identity) -> bool {
        self.expected()
            .is_some_and(|expected| expected == *identity)
    }

    /// The caller must first authenticate the archive and verify its complete
    /// inventory. These comparisons bind the verified result to one profile.
    pub fn matches_package(&self, report: &artifact::Report) -> bool {
        let Some(expected) = self.expected() else {
            return false;
        };
        let manifest = &report.manifest;
        report.authenticity == "verified-external-ed25519"
            && report.archive_sha256 == self.archive
            && report.manifest_sha256 == self.manifest
            && report.files_verified == self.files
            && report.bytes_verified == self.bytes
            && manifest.product.revision == self.product_revision
            && manifest.engine.revision == expected.source.commit
            && manifest.engine.contract_revision == expected.contract.fingerprint
            && manifest.engine.jar_sha256 == self.jar
            && manifest.runtime.java_sha256 == self.java
            && exact_file(manifest, "bin/bxdl", self.cli, self.cli_size, 0o755)
            && exact_file(
                manifest,
                "engine/nigo-node.jar",
                self.jar,
                self.jar_size,
                0o644,
            )
    }
}

fn exact_file(manifest: &artifact::Manifest, path: &str, sha: &str, size: u64, mode: u32) -> bool {
    let mut entries = manifest.files.iter().filter(|file| file.path == path);
    entries
        .next()
        .is_some_and(|file| file.sha256 == sha && file.size == size && file.mode == mode)
        && entries.next().is_none()
}
