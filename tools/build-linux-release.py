#!/usr/bin/env python3
"""Assemble signed, downloadable Linux CLI and NIGO runtime assets from pinned inputs.

Run on the Ubuntu builder using its locally built, revision-identified BXDL CLI.
The caller supplies approved product notices and SBOM. Packaged bytes are
checked here; decisions about redistribution are recorded by the release owner.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import linux_build_contract as contract


ROOT = Path(__file__).resolve().parents[1]
PACKAGING = ROOT / "packaging"
CLI = "bxdl-cli-linux-amd64.tar.gz"
RUNTIME = "bxdl-runtime-linux-amd64.tar.gz"
WORKER = "bxdl-linux-amd64"
SCRIPTS = ("install-bxdl.sh", "install-linux-helper.sh")
PUBLIC_KEY = "release-public.pem"
SIGNED_FILES = (CLI, RUNTIME, WORKER, *SCRIPTS, PUBLIC_KEY, "release.json")
CLI_FILES = ("share/zsh/site-functions/_bxdl", "docs/management-cli.md", "docs/completion.md")
MIN_GLIBC = "2.34"
OPENSSL = "/usr/bin/openssl"
COMMAND_ENV = {"LANG": "C", "LC_ALL": "C", "PATH": "/usr/bin:/bin"}


def load_sibling(filename):
    spec = importlib.util.spec_from_file_location(filename[:-3].replace("-", "_"), ROOT / "tools" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


native = load_sibling("build-linux-candidate.py")
jre = load_sibling("prepare-linux-runtime.py")
require = contract.require


def validate_tag(tag):
    require(isinstance(tag, str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,95}", tag)
            and ".." not in tag and not tag.endswith((".", ".lock")), "RELEASE_TAG_INVALID")


def revision():
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()

    require(not git("status", "--porcelain", "--untracked-files=all"), "COMMITTED_SOURCE_REQUIRED")
    commit = git("rev-parse", "HEAD")
    require(re.fullmatch(r"[a-f0-9]{40}", commit), "SOURCE_COMMIT_INVALID")
    return commit


def selected_supply(jar):
    candidate = native.read_json(PACKAGING / "linux-rc4-candidate.json")
    def locked(key):
        lock = candidate[key]
        path = ROOT / lock["path"]
        require(contract.digest(path) == lock["sha256"], "SUPPLY_LOCK_PIN_MISMATCH")
        return native.read_json(path)
    runtime, engine = locked("runtimeLock"), locked("engineLock")
    require(native.pin(jar) == {"size": engine["jarSizeBytes"], "sha256": engine["jarSha256"]}
            and candidate["engine"]["jarSha256"] == engine["jarSha256"]
            and runtime["javaSha256"] == engine["javaSha256"], "LINUX_RC4_SUPPLY_PIN_MISMATCH")
    return candidate, runtime, engine


def check_legal_inputs(notices, sbom):
    text = contract._read(notices, 32 << 20).decode()
    document = json.loads(contract._read(sbom, 64 << 20))
    require(text.strip() and "-----BEGIN PRIVATE KEY-----" not in text, "PRODUCT_NOTICES_REQUIRED")
    require(isinstance(document, dict) and
            ((document.get("bomFormat") == "CycloneDX" and document.get("components"))
             or (str(document.get("spdxVersion", "")).startswith("SPDX-") and document.get("packages"))),
            "PRODUCT_SBOM_REQUIRED")
    # Reject the repository's known runtime-only materials as substitutes for
    # the operator-approved notices and BOM of the complete release.
    for supplied, partial in ((notices, "runtime-notices.temurin-21-linux-amd64.txt"),
                              (sbom, "runtime-bom.temurin-21-linux-amd64.cdx.json")):
        require(contract.digest(supplied) != contract.digest(PACKAGING / partial), "PRODUCT_LEGAL_INPUT_REQUIRED")


def verified_assets(cli, command, archive, key, expected_metadata, stage):
    output, _ = native.run([cli, *command, "verify", archive, "--public-key", key, "--json"],
                           ROOT, COMMAND_ENV, "verify", timeout=600)
    report = native.envelope(output, " ".join([*command, "verify"]))
    require(report.get("authenticity") == "verified-external-ed25519", "RELEASE_SIGNATURE_INVALID")
    manifest = dict(report["manifest"])
    rows = manifest.pop("files")
    expected = native.tree(stage)
    require(len(rows) == len(expected) and {row["path"]: row for row in rows} ==
            {name: dict(entry, path=name) for name, entry in expected.items()}, "RELEASE_INVENTORY_MISMATCH")
    require(all(manifest.get(key) == value for key, value in expected_metadata.items()),
            "RELEASE_MANIFEST_MISMATCH")
    return report["manifestSha256"]


def build_archives(args, work, assets, commit, version, candidate, runtime, engine):
    runtime_stage = work / "runtime-stage"
    lock = candidate["runtimeLock"]
    jre.prepare(args.jre_archive, ROOT / lock["path"], lock["sha256"], runtime_stage)
    for name, src, mode in (("bin/bxdl", assets / WORKER, 0o755),
                            ("engine/nigo-node.jar", args.jar, 0o644),
                            ("licenses/THIRD_PARTY_NOTICES", args.notices, 0o644),
                            ("licenses/SBOM.json", args.sbom, 0o644)):
        native.copied(src, runtime_stage / name, mode)
    require(contract.digest(runtime_stage / "engine/nigo-node.jar") == engine["jarSha256"], "JAR_CHANGED")
    for row in runtime["files"]:
        file = runtime_stage / "runtime" / row["path"]
        with file.open("rb") as stream:
            if stream.read(4) == b"\x7fELF":
                native.elf_check(file, MIN_GLIBC)
    spec = {"schemaVersion": 1, "kind": "bxdl-package", "channel": "development",
            "product": {"name": "BXDL", "version": version, "revision": commit},
            "platform": {"os": "linux", "arch": "amd64", "libc": "glibc", "minGlibc": MIN_GLIBC,
                         "javaMajor": 21, "backend": "rocksdb"},
            "runtime": {key: runtime[key] for key in ("vendor", "version", "javaSha256")},
            "engine": {"revision": engine["expected"]["source"]["commit"], "jarSha256": engine["jarSha256"],
                       "contractStatus": "proposed", "contractRevision": engine["expected"]["contract"]["fingerprint"]}}
    spec_path = work / "runtime.json"
    native.document(spec_path, dict(spec, files=[]))
    cli = assets / WORKER
    def build(command, stage, name, extra, expected):
        native.run([cli, *command, "build", "--root", stage, "--output", assets / name, *extra,
                    "--signing-key", args.signing_key, "--json"], ROOT, COMMAND_ENV, "build", timeout=600)
        return verified_assets(cli, command, assets / name, assets / PUBLIC_KEY, expected, stage)
    runtime_manifest = build(["package"], runtime_stage, RUNTIME, ["--spec", spec_path], spec)
    cli_stage = work / "cli-stage"
    native.copied(cli, cli_stage / "bin/bxdl", 0o755)
    for name in CLI_FILES:
        source = ROOT / ("completions/_bxdl" if name == CLI_FILES[0] else name)
        native.copied(source, cli_stage / name, 0o644)
    cli_manifest = build(["cli", "bundle"], cli_stage, CLI, ["--version", version, "--revision", commit],
                         {"revision": commit, "version": version, "target": "linux-amd64"})
    return {"runtime": runtime_manifest, "management": cli_manifest}


def sign_checksums(assets, tag, key):
    validate_tag(tag)
    require({p.name for p in assets.iterdir()} == set(SIGNED_FILES), "UNEXPECTED_RELEASE_CONTENT")
    content = [f"# BXDL release {tag}"] + [f"{contract.digest(assets / name)}  {name}" for name in sorted(SIGNED_FILES)]
    manifest, signature = assets / "SHA256SUMS", assets / "SHA256SUMS.sig"
    native.write_bytes(manifest, ("\n".join(content) + "\n").encode("ascii"))
    for options in (["-sign", "-inkey", key, "-out", signature],
                    ["-verify", "-pubin", "-inkey", assets / PUBLIC_KEY, "-sigfile", signature]):
        subprocess.run([OPENSSL, "pkeyutl", *options, "-rawin", "-in", manifest],
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, check=True)
    require(signature.stat().st_size == 64, "ED25519_SIGNATURE_REQUIRED")


def assemble(args):
    native.native_host()
    validate_tag(args.tag)
    commit = revision()
    for name in ("cli", "signing_key", "jar", "jre_archive", "notices", "sbom", "output"):
        setattr(args, name, contract._absolute(getattr(args, name)))
    key = args.signing_key
    require(not key.is_relative_to(ROOT) and not contract.fingerprint(key)["mode"] & 0o077,
            "EXTERNAL_PRIVATE_SIGNING_KEY_REQUIRED")
    require(key not in (args.notices, args.sbom), "KEY_INPUT_OVERLAP")
    require(not os.path.lexists(args.output), "NEW_OUTPUT_REQUIRED")
    candidate, runtime, engine = selected_supply(args.jar)
    with tempfile.TemporaryDirectory(prefix=".bxdl-release-", dir=args.output.parent) as temporary:
        work = Path(temporary)
        assets = work / "assets"
        assets.mkdir(mode=0o700)
        worker = native.copied(args.cli, assets / WORKER, 0o755)
        native.elf_check(assets / WORKER, MIN_GLIBC)
        version = native.version(assets / WORKER, ROOT, commit, COMMAND_ENV, None)
        # Snapshot exactly the source material reviewed for incorporation.
        for name in ("notices", "sbom"):
            local = work / name
            native.copied(getattr(args, name), local, 0o600)
            setattr(args, name, local)
        check_legal_inputs(args.notices, args.sbom)
        subprocess.run([OPENSSL, "pkey", "-in", key, "-pubout", "-out", assets / PUBLIC_KEY],
                       stdin=subprocess.DEVNULL, check=True)
        manifests = build_archives(args, work, assets, commit, version, candidate, runtime, engine)
        for script in SCRIPTS:
            native.copied(ROOT / "scripts" / script, assets / script, 0o755)
        native.document(assets / "release.json", {
            "schemaVersion": 1, "tag": args.tag, "sourceCommit": commit, "managementVersion": version,
            "workerSha256": worker["sha256"], "platform": "linux-amd64", "engine": candidate["engine"],
            "runtime": {field: runtime[field] for field in ("vendor", "version", "javaSha256")},
            "runtimeLockSha256": candidate["runtimeLock"]["sha256"], "engineLockSha256": candidate["engineLock"]["sha256"],
            "manifestSha256": manifests, "noticesSha256": contract.digest(args.notices), "sbomSha256": contract.digest(args.sbom)})
        require(revision() == commit, "SOURCE_CHANGED")
        sign_checksums(assets, args.tag, key)
        for item in assets.iterdir():
            item.chmod(0o755 if item.name in (*SCRIPTS, WORKER) else 0o644)
        # Reserve the destination exclusively, even if another build raced us.
        # The workflow consumes it only after this command succeeds.
        args.output.mkdir(mode=0o700)
        for item in sorted(assets.iterdir()):
            item.rename(args.output / item.name)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    for option in ("jar", "jre-archive", "cli", "notices", "sbom", "signing-key", "output"):
        parser.add_argument("--" + option, required=True, type=Path)
    args = parser.parse_args(argv)
    os.umask(0o077)
    try:
        assemble(args)
    except (contract.Failure, jre.SupplyError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Linux release assembly failed: {error}", file=sys.stderr)
        return 1
    print(f"Signed Linux assets prepared for {args.tag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
