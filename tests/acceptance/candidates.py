"""Immutable engine identities for explicit preparation and acceptance.

RC1 constants remain available in the legacy diagnostic modules. A moving
release branch, a rebuilt JAR, or another Java launcher is never an alias.
"""

JAVA_SHA256 = "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c"
CANDIDATES = {
    "rc1": {
        "source": "540a89f0814c01d2a4ec67da1087af35195882ca",
        "jarSha256": "f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45",
        "jarSizeBytes": 164083923,
        "transactionCount": 8,
    },
    "rc2": {
        "source": "67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b",
        "jarSha256": "b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10",
        "jarSizeBytes": 164085159,
        "transactionCount": 12,
    },
    "rc3": {
        "source": "7ec5abd8c98e1cf070763a16f815e9281b74a5c1",
        "jarSha256": "05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f",
        "jarSizeBytes": 164088530,
        "transactionCount": 12,
    },
    "rc4": {
        "source": "037c98c315d5322fa7da266ca0254d61f3863622",
        "jarSha256": "56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a",
        "jarSizeBytes": 164133636,
        "transactionCount": 12,
    },
}

# Keep the preparation API independent of runtime-specific supply checks.
PREPARATION_CANDIDATES = dict(CANDIDATES)

RC4_PREPARATION_RESOURCES = {
    "generator": {
        "class": "BOOT-INF/classes/org/nigo/node/devnet/qbft/QbftDevnetGenerator.class",
        "sha256": "c02940619cc4d51eb611fd912dbdcf619511177337905476c1a39330351a3de6",
    },
    "launcher": {
        "class": "org/springframework/boot/loader/launch/PropertiesLauncher.class",
        "sha256": "4116ec36a9dc21b8d360d6fad6d10eb176cb50e62825fe9d944865392e0270ac",
    },
    "nativeCatalog": {
        "container": "BOOT-INF/lib/nigo-vm-0.0.1-SNAPSHOT.jar",
        "lockPath": "native/standard-product-candidate/lock.json",
        "lockSha256": "8aa6912d714cc829f6a6421ff3ee032e2de544c53a349801dcaae6812cb9d25b",
        "lockSizeBytes": 1000681,
        "schema": "nigo-standard-product-candidate-v1",
        "chainId": 11578,
        "acceptance": "LOCAL_DEFAULT_SUPPLY",
        "defaultActivated": True,
        "resources": {
            "fileCount": 378, "fileBytes": 9017808,
            "stockRootCount": 21, "stockDependencyCount": 26,
            "nigoRootCount": 1, "nigoDependencyCount": 1,
            "totalArtifactCount": 49,
        },
        "verification": "LOCK_BYTES_AND_SUMMARY_ONLY",
    },
}


def candidate_for(args):
    # The old, fixed RC1 probe entrypoints predate --candidate. Their allowlists
    # remain RC1-only. New preparation/acceptance CLIs require an explicit choice.
    return CANDIDATES[getattr(args, "candidate", "rc1")]


def preparation_candidate_for(args):
    return PREPARATION_CANDIDATES[getattr(args, "candidate", "rc1")]
