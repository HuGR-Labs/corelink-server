#!/usr/bin/env python3
"""Contract checks and evidence validation for issue #2050's hosted dry-run."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-2050-cli-release-dry-run.yml")
ALLOWED_FILES = {
    ".github/workflows/issue-2050-cli-release-dry-run.yml",
    ".github/workflows/issue-1724-cli-provenance.yml",
    "scripts/verify_i2050_release_dryrun_contract.py",
}
TARGETS = {
    "corelink-linux-x86_64.tar.gz",
    "corelink-linux-aarch64.tar.gz",
    "corelink-darwin-x86_64.tar.gz",
    "corelink-darwin-arm64.tar.gz",
    "corelink-windows-x86_64.zip",
}
PINNED_ACTION = re.compile(r"^\s*uses:\s+[^\s@]+@[0-9a-f]{40}(?:\s+#.*)?$", re.MULTILINE)


def fail(message: str) -> None:
    raise SystemExit(f"contract failure: {message}")


def contract() -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    changed = subprocess.run(
        ["git", "diff", "--name-only", "origin/main...HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    if set(changed) != ALLOWED_FILES:
        fail(f"PR changes outside this isolated slice: {sorted(set(changed) ^ ALLOWED_FILES)}")
    hosted_contract = Path(".github/workflows/issue-1724-cli-provenance.yml").read_text(
        encoding="utf-8"
    )
    for token in (
        '".github/workflows/issue-2050-cli-release-dry-run.yml"',
        '"scripts/verify_i2050_release_dryrun_contract.py"',
        "fetch-depth: 0",
        "persist-credentials: false",
        "python3 -S scripts/verify_i2050_release_dryrun_contract.py contract",
    ):
        if token not in hosted_contract:
            fail(f"the existing read-only hosted PR lane does not run the contract: {token}")
    if "id-token: write" in hosted_contract or "contents: write" in hosted_contract:
        fail("the PR contract lane has signing or write permission")
    if not re.search(r'^"on":\s*$', text, re.MULTILINE):
        fail('event map must use the quoted "on" key')
    for token in ("pull_request:", "workflow_dispatch:", "refs/heads/main", "github.ref_protected"):
        if token not in text:
            fail(f"required event or protected-ref guard is missing: {token}")
    if re.search(r"^\s*(?:push|schedule|create|release):", text, re.MULTILINE):
        fail("unexpected automatic publication-capable trigger")
    if not PINNED_ACTION.findall(text):
        fail("no pinned actions found")
    for line in re.findall(r"^\s*uses:.*$", text, re.MULTILINE):
        if not re.fullmatch(r"\s*uses:\s+[^\s@]+@[0-9a-f]{40}(?:\s+#.*)?", line):
            fail(f"action is not pinned to a full SHA: {line.strip()}")
    forbidden = (
        "contents: write",
        "CORELINK_CLI_RELEASE_TOKEN",
        "GH_TOKEN",
        "gh release",
        "gh api",
        "git tag",
        "refs/tags/",
        "gh auth",
        "repository_dispatch",
        "workflow_call:",
        "secrets.GITHUB_TOKEN",
    )
    for token in forbidden:
        if token.lower() in text.lower():
            fail(f"forbidden publication capability appears: {token}")
    if "secrets.GPG_KEY_FINGERPRINT" not in text:
        fail("optional fingerprint-only GPG check was removed")
    hosted_workflow = Path(".github/workflows/issue-1724-cli-provenance.yml").read_text(
        encoding="utf-8"
    )
    for token in (
        "actionlint_1.7.12_linux_amd64.tar.gz",
        "8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8",
        "Validate the dry-run trigger and workflow schema",
        "-color .github/workflows/issue-2050-cli-release-dry-run.yml",
    ):
        if token not in hosted_workflow:
            fail(f"hosted actionlint protection is missing or unpinned: {token}")
    if "persist-credentials: false" not in text:
        fail("checkout credentials are persisted")
    if not re.search(r"retention-days:\s*1\b", text):
        fail("artifacts must expire after one day")
    for artifact in TARGETS:
        if artifact.removesuffix(".tar.gz").removesuffix(".zip") not in text:
            fail(f"release-shaped target is missing: {artifact}")
    for token in (
        "cargo zigbuild",
        "cargo-zigbuild@0.19.8",
        "version: 0.16.0",
        "CycloneDX",
        "cosign sign-blob",
        "cosign verify-blob",
        "rekor",
        "actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8",
        "subject-path:",
        "if-no-files-found: error",
    ):
        if token.lower() not in text.lower():
            fail(f"required release proof is missing: {token}")
    print("PASS: only the new dry-run workflow, its checker, and read-only PR test wiring changed")
    print("PASS: manual protected-main lane, no release/tag/cross-repo write path")
    print("PASS: five targets, pinned cross-toolchain, CycloneDX, Rekor, and SLSA provenance")


def _manifest(directory: Path, source_sha: str) -> None:
    archives = sorted(path for path in directory.iterdir() if path.name.endswith((".tar.gz", ".zip")))
    if {path.name for path in archives} != TARGETS:
        fail("the package set does not contain exactly the five required targets")
    entries = []
    for path in archives:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sidecar = Path(f"{path}.sha256")
        if not sidecar.is_file() or sidecar.read_text(encoding="utf-8").split()[0] != digest:
            fail(f"checksum mismatch for {path.name}")
        entries.append({"name": path.name, "sha256": digest})
    expected_checksums = "".join(
        (directory / f"{path.name}.sha256").read_text(encoding="utf-8")
        for path in archives
    )
    actual_checksums = (directory / "checksums.txt").read_text(encoding="utf-8")
    if actual_checksums != "".join(sorted(expected_checksums.splitlines(keepends=True))):
        fail("combined checksum inventory does not match the five archive sidecars")
    bom = json.loads((directory / "corelink-cli.cdx.json").read_text(encoding="utf-8"))
    if bom.get("bomFormat") != "CycloneDX" or bom.get("specVersion") != "1.5":
        fail("CLI SBOM is not CycloneDX 1.5")
    if not isinstance(bom.get("components"), list) or not bom["components"]:
        fail("CLI SBOM contains no components")
    value = {
        "schema": "https://corelink.dev/schemas/cli-release-dry-run/v1",
        "dry_run": True,
        "source_sha": source_sha,
        "artifacts": entries,
        "sbom": {
            "name": "corelink-cli.cdx.json",
            "sha256": hashlib.sha256((directory / "corelink-cli.cdx.json").read_bytes()).hexdigest(),
        },
    }
    (directory / "dry-run-manifest.json").write_text(
        json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )
    print("PASS: five release-shaped archives and CycloneDX manifest verified")


def provenance(directory: Path, bundle_path: Path) -> None:
    try:
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
        payload = base64.b64decode(bundle["dsseEnvelope"]["payload"], validate=True)
        statement = json.loads(payload)
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        fail(f"invalid GitHub provenance bundle: {error}")
    if statement.get("predicateType") != "https://slsa.dev/provenance/v1":
        fail("attestation predicate is not SLSA v1")
    actual = {}
    for subject in statement.get("subject", []):
        name = subject.get("name")
        digest = subject.get("digest", {}).get("sha256")
        if not isinstance(name, str) or not isinstance(digest, str):
            fail("malformed in-toto subject")
        actual[name.rsplit("/", 1)[-1]] = digest.lower()
    expected_paths = [
        *directory.glob("*.tar.gz"),
        *directory.glob("*.zip"),
        *directory.glob("*.sha256"),
        *directory.glob("*.cdx.json"),
        directory / "dry-run-manifest.json",
        directory / "checksums.txt",
    ]
    expected = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in expected_paths}
    if actual != expected:
        fail(f"SLSA subject inventory differs: expected {sorted(expected)}, got {sorted(actual)}")
    (directory / "provenance.intoto.jsonl").write_bytes(payload + b"\n")
    print(f"PASS: SLSA v1 provenance binds exactly {len(expected)} dry-run inventory subjects")


def summary(directory: Path, run_url: str) -> None:
    manifest = json.loads((directory / "dry-run-manifest.json").read_text(encoding="utf-8"))
    print("## CLI release dry-run (nonpublishing)")
    print()
    print(f"- Run: {run_url}")
    print(f"- Source: `{manifest['source_sha']}`")
    print(f"- Archives: {len(manifest['artifacts'])} / 5; all SHA-256 sidecars verified")
    print(f"- SBOM: CycloneDX 1.5 (`{manifest['sbom']['sha256']}`)")
    print("- Cosign keyless signatures: verified against GitHub OIDC identity and Rekor bundles")
    print("- Provenance: SLSA v1 in-toto statement verified against the exact artifact inventory")
    print("- Publication: none; ephemeral artifacts expire after one day")


def main() -> None:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("contract")
    manifest_cmd = sub.add_parser("manifest")
    manifest_cmd.add_argument("--directory", type=Path, required=True)
    manifest_cmd.add_argument("--source-sha", required=True)
    provenance_cmd = sub.add_parser("provenance")
    provenance_cmd.add_argument("--directory", type=Path, required=True)
    provenance_cmd.add_argument("--bundle", type=Path, required=True)
    summary_cmd = sub.add_parser("summary")
    summary_cmd.add_argument("--directory", type=Path, required=True)
    summary_cmd.add_argument("--run-url", required=True)
    args = parser.parse_args()
    if args.command == "contract":
        contract()
    elif args.command == "manifest":
        _manifest(args.directory, args.source_sha)
    elif args.command == "provenance":
        provenance(args.directory, args.bundle)
    elif args.command == "summary":
        summary(args.directory, args.run_url)


if __name__ == "__main__":
    main()
