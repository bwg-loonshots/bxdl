"""Read-only identities for already assembled development runtimes.

RC4 pins come from results/2026-11-15-rc4-runtime-assembly.json (RC4-03).
Profiles select exact archives/executors; an engine contract match is not a
DB migration or mixed-network compatibility claim. Import performs no I/O.
"""
import copy
import hashlib
import json
from pathlib import Path
import stat

from candidates import CANDIDATES, JAVA_SHA256
import support as h

_PROFILES = {
    "rc3": {
        "candidate": "rc3", "profile": "BXDL_RC3_EXECUTOR_V1",
        "capability": "executor.rc3-legacy-v1",
        "archiveSha256": "8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017",
        "publicKeySha256": "0f03b13417dbb9c362a8762afc7e45bedbf20a983024105b2dcbf55b4dc40f17",
        "engineLockSha256": "2d6ebff73f8f9c741922b3324fc1d5fc162be64e53ce830cb546ee6fb5d8c344",
        "manifestSha256": "2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123",
        "executorSha256": "b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef",
        "executorSizeBytes": 3024064,
        "payloadFiles": 270, "payloadBytes": 325299323, "jreFiles": 259,
    },
    "rc4": {
        "candidate": "rc4", "profile": "BXDL_RC4_EXECUTOR_V1",
        "capability": "executor.rc4-v1",
        "archiveSha256": "a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60",
        "archiveSizeBytes": 207705667,
        "publicKeySha256": "330c6c6e19d0e353b1001b9a4c84266765e8d4defcf04317771ab8977f72e1ee",
        "engineLockSha256": "d07b35ee5a630ef459bb26a492a1511547e47e483c40120b7474b67a90e1326d",
        "manifestSha256": "fe21e898f5ace8be57ad838620f14da1c3e07b3ffcfbcb969fd63a67854a142e",
        "executorSha256": "d2d5c586bf2ab59ac3198cbcc0f89101fc18d31e6f4ec7812b428e481a63f647",
        "executorSizeBytes": 4219840,
        "executorRevision": "d38027a474f1e8d4fb7b1d38ea17972ba88d93af",
        "executorTree": "cf97bc426e2ab997cb72b6e301aa8c057e0e214d",
        "payloadFiles": 269, "payloadBytes": 326373420, "jreFiles": 259,
        "inventorySha256": "620e2156b2d5f6437bd1756a23cd3938d07f4a8b197f8e7df324f731559253b8",
        "specTemplateSha256": "4288ffbb41bc8c19009c13dcb646a06cc8657523f35432f7e19c7d108a9b2b01",
        "resolvedSpecSha256": "a5f098381bb3fd1be9e129ca0361ac62485105468f1cdc0c5058b4af8e4a2058",
        "assemblyEvidence": "results/2026-11-15-rc4-runtime-assembly.json",
    },
}


RUNTIME_PROFILES = tuple(_PROFILES)

def runtime_profile(candidate="rc3"):
    h.require(candidate in _PROFILES, "RUNTIME_PROFILE_NOT_SUPPORTED")
    profile = copy.deepcopy(_PROFILES[candidate])
    profile.update(jarSha256=CANDIDATES[candidate]["jarSha256"],
                   jarSizeBytes=CANDIDATES[candidate]["jarSizeBytes"], javaSha256=JAVA_SHA256)
    profile["pins"] = {key: profile[field] for key, field in {
        "archive": "archiveSha256", "publicKey": "publicKeySha256",
        "engineLock": "engineLockSha256", "manifest": "manifestSha256",
        "executor": "executorSha256", "jar": "jarSha256", "java": "javaSha256",
    }.items()}
    return profile


def require_manager_capability(version_data, profile):
    h.require(isinstance(version_data, dict), "MANAGER_RUNTIME_CAPABILITY_REQUIRED")
    capabilities = version_data.get("capabilities")
    h.require(isinstance(capabilities, list)
              and all(isinstance(value, str) for value in capabilities)
              and profile["capability"] in capabilities, "MANAGER_RUNTIME_CAPABILITY_REQUIRED")
    return list(capabilities)


def verify_runtime_supply(archive, public_key, lock, candidate="rc3"):
    """Pin public supply files without executing CLI, JVM or signing tools."""
    profile = runtime_profile(candidate)
    for path, field in ((archive, "archiveSha256"), (public_key, "publicKeySha256"),
                        (lock, "engineLockSha256")):
        h.regular(Path(path))
        h.require(h.sha(Path(path)) == profile[field], "FIXED_RUNTIME_SUPPLY_PIN")
    if "archiveSizeBytes" in profile:
        h.require(Path(archive).stat().st_size == profile["archiveSizeBytes"], "RUNTIME_ARCHIVE_SIZE")
    return {key: profile[key] for key in ("archiveSha256", "publicKeySha256", "engineLockSha256")}


def verify_installed_payload(package, candidate="rc3"):
    """Verify the complete installed payload, excluding only install metadata."""
    package = Path(package)
    profile = runtime_profile(candidate)
    h.require(stat.S_ISDIR(package.lstat().st_mode), "RUNTIME_DIRECTORY")
    h.regular(package / "manifest.json")
    h.require(h.sha(package / "manifest.json") == profile["manifestSha256"], "RUNTIME_MANIFEST_PIN")
    manifest = h.document(package / "manifest.json")
    files = manifest["files"]
    h.require(len(files) == profile["payloadFiles"]
              and len({f["path"] for f in files}) == len(files), "RUNTIME_MEMBER_COUNT")
    if "inventorySha256" in profile:
        raw = (json.dumps(sorted(files, key=lambda item: item["path"]), indent=2, sort_keys=True) + "\n").encode()
        h.require(hashlib.sha256(raw).hexdigest() == profile["inventorySha256"], "RUNTIME_INVENTORY_PIN")
    paths = set()
    for path in package.rglob("*"):
        h.require(not path.is_symlink(), "RUNTIME_SYMLINK")
        if not path.is_dir():
            h.regular(path)
            paths.add(path.relative_to(package).as_posix())
    h.require(paths == {f["path"] for f in files}
              | {"manifest.json", "manifest.sig", ".bxdl-install.json"}, "RUNTIME_MEMBERSHIP")
    for item in files:
        relative = Path(item["path"])
        h.require(not relative.is_absolute() and ".." not in relative.parts, "RUNTIME_PATH")
        path = package / relative
        metadata = h.regular(path)
        h.require(metadata.st_size == item["size"]
                  and stat.S_IMODE(metadata.st_mode) == item["mode"]
                  and h.sha(path) == item["sha256"], "RUNTIME_FILE_CHANGED")
    h.require(h.sha(package / "bin/bxdl") == profile["executorSha256"]
              and (package / "bin/bxdl").stat().st_size == profile["executorSizeBytes"], "EXECUTOR_PIN")
    h.require(h.sha(package / "engine/nigo-node.jar") == profile["jarSha256"]
              and (package / "engine/nigo-node.jar").stat().st_size == profile["jarSizeBytes"], "ENGINE_PIN")
    h.require(h.sha(package / "runtime/bin/java") == profile["javaSha256"], "JAVA_PIN")
    runtime = [f for f in files if f["path"].startswith("runtime/")]
    h.require(len(runtime) == profile["jreFiles"]
              and sum(f["size"] for f in files) == profile["payloadBytes"], "RUNTIME_TOTALS")
    return {"files": len(files), "bytes": sum(f["size"] for f in files),
            "runtimeFiles": len(runtime), "manifestSha256": profile["manifestSha256"],
            "executorSha256": profile["executorSha256"], "engineJarSha256": profile["jarSha256"],
            "javaSha256": profile["javaSha256"]}
