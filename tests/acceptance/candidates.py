"""Immutable engine identities for the explicitly supported test candidates.

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
}


def candidate_for(args):
    # The old, fixed RC1 probe entrypoints predate --candidate. Their allowlists
    # remain RC1-only. New preparation/acceptance CLIs require an explicit choice.
    return CANDIDATES[getattr(args, "candidate", "rc1")]
