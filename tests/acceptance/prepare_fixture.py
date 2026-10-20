#!/usr/bin/env python3
"""Prepare fresh test-only material from the pinned RC1 bytes; never run a node.

Preparation is separate from BXDL installation/acceptance. This script needs no
NIGO checkout, Gradle, network access, or customer credentials. Its Java helper is
the QbftDevnetGenerator already present in the unchanged RC1 distribution.
All output stays in a new private directory, including child diagnostics.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
from urllib.parse import unquote, urlparse


ENGINE_SOURCE = "540a89f0814c01d2a4ec67da1087af35195882ca"
JAR_SHA256 = "f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45"
JAVA_SHA256 = "9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c"
KEYTOOL_SHA256 = "17b50cf49c688935080bb39b3196494827577735d9240ef9178598df6dbe0911"
CHAIN_ID = 11578
PROFILE = "DEV_QBFT"
SECRET_NAMES = (
    "validator.keystore",
    "validator.password",
    "peer-identity.p12",
    "peer-identity.password",
    "peer-trust.p12",
    "peer-trust.password",
)

WALLET_SCRIPT = r"""
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

// No provider is constructed: signing is entirely offline.
try {
  const [moduleUrl, output] = process.argv.slice(2);
  const { Wallet, Transaction, keccak256, version } = await import(moduleUrl);
  if (version !== '6.17.0') throw new Error('unsupported ethers');
  const wallet = Wallet.createRandom();
  function save(name, value) {
    writeFileSync(join(output, name), JSON.stringify(value, null, 2) + '\n',
      { encoding: 'utf8', mode: 0o600, flag: 'wx' });
  }
  save('wallet.json', {
    purpose: 'EPHEMERAL_LOCAL_ACCEPTANCE_ONLY',
    address: wallet.address.toLowerCase(),
    privateKey: wallet.privateKey
  });
  const transactions = [];
  for (let nonce = 0; nonce < 8; nonce++) {
    const request = {
      type: 2, chainId: 11578, nonce, gasLimit: 21000n,
      maxFeePerGas: 0n, maxPriorityFeePerGas: 0n,
      to: '0x0000000000000000000000000000000000000042',
      value: 1n, data: '0x', accessList: []
    };
    const rawTransaction = await wallet.signTransaction(request);
    const hash = keccak256(rawTransaction);
    const decoded = Transaction.from(rawTransaction);
    if (decoded.hash !== hash || decoded.nonce !== nonce
        || decoded.from?.toLowerCase() !== wallet.address.toLowerCase()
        || decoded.chainId !== 11578n || decoded.type !== 2) {
      throw new Error('transaction identity mismatch');
    }
    save(`transactions/tx-${nonce}.json`, {
      schemaVersion: 1, nonce, hash, rawTransaction,
      chainId: 11578, type: 2,
      from: wallet.address.toLowerCase(), to: request.to, value: '1'
    });
    transactions.push({ nonce, hash, file: `transactions/tx-${nonce}.json` });
  }
  save('wallet-public.json', {
    schemaVersion: 1, ethersVersion: version,
    address: wallet.address.toLowerCase(), transactions
  });
} catch {
  // Never interpolate exceptions: they may include a key or signed payload.
  process.stderr.write('Offline transaction fixture preparation failed.\n');
  process.exitCode = 1;
}
"""


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def regular(path):
    resolved = Path(path).expanduser().resolve(strict=True)
    if not resolved.is_file():
        raise ValueError("regular file required")
    return resolved


def pinned_file(path, digest):
    resolved = regular(path)
    if sha256(resolved) != digest:
        raise ValueError("pinned artifact mismatch")
    return resolved


def write_new(path, content):
    if not isinstance(content, bytes):
        content = content.encode("utf-8")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as destination:
        destination.write(content)
        destination.flush()
        os.fsync(destination.fileno())


def write_json(path, value):
    write_new(path, json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def read_public_json(path):
    if path.stat().st_size > 1024 * 1024:
        raise ValueError("public fixture too large")
    return json.loads(path.read_text("utf-8"))


def child_environment():
    # Do not inherit Java/Node loader injection or ambient Spring configuration.
    keep = ("PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "SystemRoot")
    return {name: os.environ[name] for name in keep if name in os.environ}


def run_private(argv, root, label, timeout=120):
    stdout = root / "evidence" / (label + ".stdout.txt")
    stderr = root / "evidence" / (label + ".stderr.txt")
    out_fd = os.open(stdout, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    err_fd = os.open(stderr, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(out_fd, "wb") as out, os.fdopen(err_fd, "wb") as err:
        result = subprocess.run(
            [str(item) for item in argv],
            cwd=root / "tool-cwd",
            env=child_environment(),
            stdin=subprocess.DEVNULL,
            stdout=out,
            stderr=err,
            timeout=timeout,
            check=False,
        )
    if result.returncode != 0:
        raise RuntimeError("fixture child failed")


def module_path(value):
    if value.startswith("file:"):
        parsed = urlparse(value)
        if parsed.netloc not in ("", "localhost") or parsed.query or parsed.fragment:
            raise ValueError("local module URL required")
        value = unquote(parsed.path)
    return regular(value)


def port_base(value):
    port = int(value)
    if not 1024 <= port <= 65532:
        raise argparse.ArgumentTypeError("base port must allow four unprivileged ports")
    return port


def checked_manifest(path, generated, http_base, p2p_base):
    manifest = read_public_json(path)
    if (
        manifest.get("chainId") != CHAIN_ID
        or manifest.get("profileId") != PROFILE
        or manifest.get("transportSecurityScheme") != "MTLS"
    ):
        raise ValueError("generator chain identity mismatch")
    nodes = manifest.get("nodes", [])
    if len(nodes) != 4:
        raise ValueError("four nodes required")
    seen = {"nodeId": set(), "validatorId": set(), "credentialFingerprint": set()}
    for index, node in enumerate(nodes, 1):
        directory = generated / ("validator-" + str(index))
        if (
            node.get("index") != index
            or node.get("role") != "VALIDATOR"
            or node.get("directory") != str(directory)
            or node.get("config") != str(directory / "application.yml")
            or node.get("httpPort") != http_base + index - 1
            or node.get("p2pPort") != p2p_base + index - 1
        ):
            raise ValueError("generator node metadata mismatch")
        for field, width in (("nodeId", 64), ("validatorId", 40), ("credentialFingerprint", 64)):
            value = node.get(field, "")
            if not isinstance(value, str) or not re.fullmatch("0x[0-9a-f]{" + str(width) + "}", value):
                raise ValueError("generator public identity invalid")
            if value in seen[field]:
                raise ValueError("duplicate generated identity")
            seen[field].add(value)
        for name in SECRET_NAMES:
            metadata = (directory / name).lstat()
            if not stat.S_ISREG(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o600:
                raise ValueError("generated secret permissions invalid")
    return nodes


def chain_properties(nodes, funded_address):
    # Explicit canonical RC1 whitelist. QbftDevnetGenerator's generated YAML
    # remains evidence; it is not an ambient configuration source.
    chain = {
        "nigo.protocol.chain-id": str(CHAIN_ID),
        "nigo.protocol.consensus.profile-id": PROFILE,
        "nigo.protocol.consensus.protocol": "QBFT",
        "nigo.protocol.consensus.max-transactions-per-block": "100",
        "nigo.protocol.consensus.qbft.block-period-ms": "1000",
        "nigo.protocol.consensus.qbft.empty-block-period-ms": "0",
        "nigo.protocol.consensus.qbft.request-timeout-ms": "5000",
        "nigo.protocol.consensus.qbft.max-request-timeout-ms": "20000",
        "nigo.protocol.consensus.qbft.max-future-block-time-ms": "1000",
        "nigo.protocol.native-assets.registration-mode": "ALLOWLISTED",
        "nigo.protocol.devnet.genesis-funding[0].address": funded_address,
        "nigo.protocol.devnet.genesis-funding[0].amount": "1000000000000000000000",
        "nigo.protocol.fee-profile.revisions[0].activation-height": "0",
        "nigo.protocol.fee-profile.revisions[0].mode": "ZERO",
        "nigo.protocol.fee-profile.revisions[0].policy-id": "ZERO",
        "nigo.protocol.fee-profile.revisions[0].policy-revision": "1",
        "nigo.protocol.fee-profile.revisions[0].gas-price": "0",
        "nigo.protocol.fee-profile.revisions[0].settlement-mode": "NONE",
    }
    for index, validator_id in enumerate(sorted(node["validatorId"] for node in nodes)):
        chain[f"nigo.protocol.consensus.qbft.validators[{index}]"] = validator_id
    return chain


def prepare(args):
    java = pinned_file(args.java, JAVA_SHA256)
    jar = pinned_file(args.jar, JAR_SHA256)
    node_executable = regular(args.node)
    ethers = module_path(args.ethers_module)
    if not os.access(java, os.X_OK) or not os.access(node_executable, os.X_OK):
        raise ValueError("executable required")
    if set(range(args.http_base_port, args.http_base_port + 4)) & set(
        range(args.p2p_base_port, args.p2p_base_port + 4)
    ):
        raise ValueError("HTTP and P2P ranges overlap")
    requested = Path(args.output).expanduser().absolute()
    if requested.name in ("", ".", ".."):
        raise ValueError("new output path required")
    root = requested.parent.resolve(strict=True) / requested.name
    os.mkdir(root, 0o700)  # Refuses existing files, folders and dangling symlinks.
    for name in ("products", "public", "transactions", "evidence", "tooling", "tool-cwd", "data"):
        os.mkdir(root / name, 0o700)

    generated = root / "generated"
    run_private(
        [
            java,
            "-Dloader.home=" + str(root / "tool-cwd"),
            "-Dloader.main=org.nigo.node.devnet.qbft.QbftDevnetGenerator",
            "-cp",
            jar,
            "org.springframework.boot.loader.launch.PropertiesLauncher",
            "--output=" + str(generated),
            "--validators=4",
            "--observers=0",
            "--transport-security=MTLS",
            "--chain-id=" + str(CHAIN_ID),
            "--profile-id=" + PROFILE,
            "--http-base-port=" + str(args.http_base_port),
            "--p2p-base-port=" + str(args.p2p_base_port),
            "--request-timeout-ms=5000",
            "--max-request-timeout-ms=20000",
        ],
        root,
        "generate",
        timeout=180,
    )
    nodes = checked_manifest(
        generated / "devnet.json", generated, args.http_base_port, args.p2p_base_port
    )
    write_new(root / "tooling" / "make-transactions.mjs", WALLET_SCRIPT)
    run_private(
        [node_executable, root / "tooling" / "make-transactions.mjs", ethers.as_uri(), root],
        root,
        "sign-offline",
    )
    wallet_public = read_public_json(root / "wallet-public.json")
    address = wallet_public.get("address", "")
    if not re.fullmatch(r"0x[0-9a-f]{40}", address):
        raise ValueError("funded address invalid")
    write_json(root / "chain.json", chain_properties(nodes, address))
    network_nodes = []
    for index, metadata in enumerate(nodes, 1):
        directory = generated / ("validator-" + str(index))
        network_nodes.append(
            {
                "nodeId": metadata["nodeId"],
                "validatorId": metadata["validatorId"],
                "host": "127.0.0.1",
                "port": metadata["p2pPort"],
                "transactionPermission": "INGRESS",
                "credentialFingerprints": [metadata["credentialFingerprint"]],
                "revokedCredentialFingerprints": [],
            }
        )
        write_json(
            root / "products" / f"n{index}.json",
            {
                "schemaVersion": 1,
                "instanceId": f"qbft-acceptance-n{index}",
                "role": "validator",
                "nodeId": metadata["nodeId"],
                "chainDescription": str(root / "chain.json"),
                "storage": {"backend": "rocksdb", "dataDirectory": str(root / "data" / f"n{index}")},
                "secrets": {
                    "validatorKeystore": str(directory / "validator.keystore"),
                    "validatorPasswordFile": str(directory / "validator.password"),
                    "tlsKeyStore": str(directory / "peer-identity.p12"),
                    "tlsKeyPasswordFile": str(directory / "peer-identity.password"),
                    "tlsTrustStore": str(directory / "peer-trust.p12"),
                    "tlsTrustPasswordFile": str(directory / "peer-trust.password"),
                },
                "http": {"address": "127.0.0.1", "port": metadata["httpPort"]},
                "p2p": {"address": "127.0.0.1", "port": metadata["p2pPort"]},
            },
        )
    write_json(root / "network.json", {"schemaVersion": 1, "kind": "BXDL_QBFT_NETWORK", "nodes": network_nodes})

    keytool = java.parent / "keytool"
    certificate_export = "NOT_AVAILABLE"
    if keytool.is_file():
        keytool = pinned_file(keytool, KEYTOOL_SHA256)
        for index, metadata in enumerate(nodes, 1):
            directory = generated / ("validator-" + str(index))
            leaf = root / "public" / f"leaf-n{index}.der"
            run_private(
                [
                    keytool, "-exportcert", "-alias", "peer", "-storetype", "PKCS12",
                    "-keystore", directory / "peer-identity.p12",
                    "-storepass:file", directory / "peer-identity.password",
                    "-file", leaf,
                ],
                root,
                f"export-leaf-n{index}",
            )
            leaf.chmod(0o600)
            der = leaf.read_bytes()
            expected = "0x" + hashlib.sha256(b"NIGO_TRANSPORT_CREDENTIAL/v1\x00MTLS\x00" + der).hexdigest()
            if expected != metadata["credentialFingerprint"]:
                raise ValueError("exported certificate pin mismatch")
        certificate_export = "EXPORTED_PIN_MATCHED"
    if sha256(jar) != JAR_SHA256 or sha256(java) != JAVA_SHA256:
        raise ValueError("artifact changed during preparation")
    if any((root / "data" / f"n{i}").exists() for i in range(1, 5)):
        raise ValueError("unexpected node data creation")
    summary = {
        "schemaVersion": 1,
        "status": "PREPARED_NOT_EXECUTED",
        "purpose": "EPHEMERAL_LOCAL_BXDL_PACKAGE_ACCEPTANCE_ONLY",
        "engineSource": ENGINE_SOURCE,
        "engineJarSha256": JAR_SHA256,
        "javaSha256": JAVA_SHA256,
        "ethersVersion": wallet_public["ethersVersion"],
        "ethersEntrySha256": sha256(ethers),
        "chainSha256": sha256(root / "chain.json"),
        "networkSha256": sha256(root / "network.json"),
        "fundedAddress": address,
        "transactions": wallet_public["transactions"],
        "nodes": [
            {
                "index": item["index"], "nodeId": item["nodeId"], "validatorId": item["validatorId"],
                "httpPort": item["httpPort"], "p2pPort": item["p2pPort"],
                "credentialFingerprint": item["credentialFingerprint"],
                "product": f"products/n{item['index']}.json",
            }
            for item in nodes
        ],
        "certificateExport": certificate_export,
        "recommendedRuntimeOptions": {
            "mode": "join-network", "consensusAutoStart": True, "archiveEnabled": True,
            "keyStoreType": "PKCS12", "trustStoreType": "PKCS12",
        },
        "submissionPolicy": "NODE_1_ONLY_ONCE_PER_PRECOMPUTED_HASH",
        "nodeDataCreated": False,
        "nodeStarted": False,
        "networkTraffic": "NOT_PERFORMED",
        "runtimeAcceptance": "NOT_CHECKED",
    }
    write_json(root / "fixture-summary.json", summary)
    return {"status": "PREPARED_NOT_EXECUTED", "validators": 4, "transactions": 8, "certificateExport": certificate_export}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java", required=True)
    parser.add_argument("--jar", required=True)
    parser.add_argument("--ethers-module", required=True)
    parser.add_argument("--node", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--http-base-port", type=port_base, required=True)
    parser.add_argument("--p2p-base-port", type=port_base, required=True)
    args = parser.parse_args()
    old_mask = os.umask(0o077)
    try:
        result = prepare(args)
    except Exception:
        # Preserve private evidence and partial fixture; never overwrite/retry.
        print("Fixture preparation failed; inspect the private evidence directory.", file=sys.stderr)
        return 1
    finally:
        os.umask(old_mask)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
