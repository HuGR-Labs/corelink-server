#!/usr/bin/env python3
"""Contract checks and evidence validation for issue #2050's hosted dry-run."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-2050-cli-release-dry-run.yml")
ALLOWED_FILES = {
    ".github/workflows/issue-2050-cli-release-dry-run.yml",
    ".github/workflows/issue-2152-cyclonedx-diagnostic.yml",
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
ZIGBUILD_VERSION_PROBE = '''test "$(cargo-zigbuild --version | awk '{print $2}')" = "0.19.8"'''
UNSUPPORTED_ZIGBUILD_VERSION_PROBES = (
    "cargo zigbuild --version",
    "cargo zigbuild -V",
)
MINGW_W64_FORMULA_URL = (
    "https://raw.githubusercontent.com/Homebrew/homebrew-core/"
    "00e77a1611f627f2ea8f876fad0468f2c4b3ed20/Formula/m/mingw-w64.rb"
)
MINGW_W64_FORMULA_SHA256 = "4b5f53d8ff341875f158092cbdcd5536ce3d6d5136de1672ecf687564dae07f9"
BINUTILS_RESOURCE_URL = '    url "https://ftpmirror.gnu.org/binutils/binutils-2.47.tar.bz2"'
BINUTILS_RESOURCE_MIRROR = '    mirror "https://ftp.gnu.org/gnu/binutils/binutils-2.47.tar.bz2"'
BINUTILS_RESOURCE_SHA256 = '    sha256 "3068128c75cda9f898ccb4211d360246e8e195ffcc9dfb655b23ae23a54800e8"'
BINUTILS_BUILD_DATE = "20260726"
PINNED_FORMULA_TRUST = 'brew trust --formula "${MINGW_W64_TAP}/mingw-w64"'
PINNED_FORMULA_INSTALL = 'brew install "${MINGW_W64_TAP}/mingw-w64"'
DLLTOOL_PATH = "$(brew --prefix mingw-w64)/bin/x86_64-w64-mingw32-dlltool"
DLLTOOL_GUARDS = (
    'if [ "${ACTUAL_MINGW_W64_VERSION}" != "${EXPECTED_MINGW_W64_VERSION}" ]; then',
    'if [ ! -x "${DLLTOOL_PATH}" ]; then',
    'if [ "${ACTUAL_DLLTOOL_VERSION}" != "${EXPECTED_DLLTOOL_VERSION}" ]; then',
)


def fail(message: str) -> None:
    raise SystemExit(f"contract failure: {message}")


def _verify_zigbuild_version_probe(text: str) -> None:
    if ZIGBUILD_VERSION_PROBE not in text:
        fail("cargo-zigbuild must be pinned by its direct --version probe")
    for unsupported in UNSUPPORTED_ZIGBUILD_VERSION_PROBES:
        if unsupported in text:
            fail(f"unsupported version probe must not return: {unsupported}")


def _zigbuild_version_probe_mutation_self_test(text: str) -> None:
    for unsupported in UNSUPPORTED_ZIGBUILD_VERSION_PROBES:
        mutated = text.replace(
            ZIGBUILD_VERSION_PROBE,
            f"{ZIGBUILD_VERSION_PROBE}\n          {unsupported}",
            1,
        )
        try:
            _verify_zigbuild_version_probe(mutated)
        except SystemExit as error:
            if unsupported not in str(error):
                raise
        else:
            fail(f"unsupported cargo-zigbuild mutation survived: {unsupported}")


def _verify_i2050_pr_slice(changed_files: set[str]) -> bool:
    """Enforce the isolated dry-run and SBOM diagnostic file allowlist."""
    if not changed_files.intersection(ALLOWED_FILES):
        return False
    outside_files = changed_files - ALLOWED_FILES
    if outside_files:
        fail(
            "PR changes outside this isolated slice: "
            f"{sorted(outside_files)}"
        )
    return True


def _slice_routing_mutation_self_test() -> None:
    for allowed_file in ALLOWED_FILES:
        if not _verify_i2050_pr_slice({allowed_file}):
            fail(f"the relevant-change case did not enforce the slice: {allowed_file}")

    if not _verify_i2050_pr_slice(ALLOWED_FILES.copy()):
        fail("the both-files case did not enforce the isolated CLI slice")

    try:
        _verify_i2050_pr_slice(ALLOWED_FILES | {"Cargo.lock"})
    except SystemExit:
        pass
    else:
        fail("an unrelated file mixed into the isolated CLI slice survived")

    if _verify_i2050_pr_slice({"Cargo.lock"}):
        fail("an unrelated-only change was incorrectly routed into the CLI slice")


def _verify_dlltool_contract(text: str) -> None:
    install_step = text.find("- name: Install and verify pinned MinGW-w64 dlltool")
    windows_target = text.find("if [ \"${target}\" = \"x86_64-pc-windows-gnu\" ]; then")
    windows_build = text.find('cargo zigbuild -p corelink-cli --release --locked --target "${target}"')
    if install_step < 0 or windows_target < 0 or windows_build < 0:
        fail("pinned dlltool installation and the Windows GNU build must be present")
    if install_step > windows_target or windows_target > windows_build:
        fail("Windows GNU build must be gated by its dlltool probe")
    for unsupported in (
        'brew tap-new --no-git "${MINGW_W64_TAP}"',
        'env -u HOMEBREW_FORBID_PACKAGES_FROM_PATHS brew install',
        'HOMEBREW_NO_REQUIRE_TAP_TRUST=1',
    ):
        if unsupported in text:
            fail(f"untrusted or unpinned tap installation is forbidden: {unsupported}")
    for token in (
        "Install and verify pinned MinGW-w64 dlltool",
        "EXPECTED_MINGW_W64_VERSION=14.0.0_3",
        "EXPECTED_BINUTILS_VERSION=2.47",
        f"EXPECTED_BINUTILS_BUILD_DATE={BINUTILS_BUILD_DATE}",
        f"EXPECTED_MINGW_W64_FORMULA_SHA256={MINGW_W64_FORMULA_SHA256}",
        MINGW_W64_FORMULA_URL,
        'MINGW_W64_TAP=corelink/homebrew-mingw-w64',
        'brew tap-new "${MINGW_W64_TAP}"',
        'test "$(git -C "${MINGW_W64_TAP_REPOSITORY}" rev-parse --is-inside-work-tree)" = true',
        'curl --fail --location --silent --show-error "${MINGW_W64_FORMULA_URL}" --output "${MINGW_W64_PINNED_FORMULA}"',
        "shasum -a 256 --check --status",
        'BINUTILS_RESOURCE="$(awk',
        BINUTILS_RESOURCE_URL,
        BINUTILS_RESOURCE_MIRROR,
        BINUTILS_RESOURCE_SHA256,
        'cp "${MINGW_W64_PINNED_FORMULA}" "${MINGW_W64_TAP_FORMULA}"',
        PINNED_FORMULA_TRUST,
        PINNED_FORMULA_INSTALL,
        'brew list --versions mingw-w64',
        DLLTOOL_PATH,
        'EXPECTED_DLLTOOL_VERSION="GNU ${DLLTOOL_PATH} (GNU Binutils) ${EXPECTED_BINUTILS_VERSION}.${EXPECTED_BINUTILS_BUILD_DATE}"',
        'printf \'DLLTOOL_PATH=%s\\nEXPECTED_DLLTOOL_VERSION=%s\\n\' "${DLLTOOL_PATH}" "${EXPECTED_DLLTOOL_VERSION}" >> "${GITHUB_ENV}"',
        '"${DLLTOOL_PATH}" --version | sed -n \'1p\'',
        'dirname "${DLLTOOL_PATH}" >> "${GITHUB_PATH}"',
        'test "$(command -v x86_64-w64-mingw32-dlltool)" = "${DLLTOOL_PATH}"',
        'test "$("${DLLTOOL_PATH}" --version | sed -n \'1p\')" = "${EXPECTED_DLLTOOL_VERSION}"',
    ):
        if token not in text:
            fail(f"pinned fail-closed Windows dlltool contract is missing: {token}")
    for guard in DLLTOOL_GUARDS:
        start = text.find(guard)
        end = text.find("\n          fi", start)
        if start < 0 or end < 0 or "exit 1" not in text[start:end]:
            fail(f"dlltool preflight must fail closed: {guard}")


def _dlltool_contract_mutation_self_test(text: str) -> None:
    for token, replacement in (
        (
            "EXPECTED_MINGW_W64_VERSION=14.0.0_3",
            "EXPECTED_MINGW_W64_VERSION=14.0.0",
        ),
        (
            "EXPECTED_MINGW_W64_VERSION=14.0.0_3",
            "EXPECTED_MINGW_W64_VERSION=14.0.0_2",
        ),
        (f"EXPECTED_BINUTILS_BUILD_DATE={BINUTILS_BUILD_DATE}", "EXPECTED_BINUTILS_BUILD_DATE=20260725"),
        (f"EXPECTED_BINUTILS_BUILD_DATE={BINUTILS_BUILD_DATE}", "EXPECTED_BINUTILS_BUILD_DATE=20260230"),
        ("EXPECTED_BINUTILS_VERSION=2.47", "EXPECTED_BINUTILS_VERSION=2.48"),
        (DLLTOOL_PATH, "$(brew --prefix mingw-w64)/lib/x86_64-w64-mingw32-dlltool"),
        (BINUTILS_RESOURCE_SHA256, '    sha256 "0000000000000000000000000000000000000000000000000000000000000000"'),
        (MINGW_W64_FORMULA_URL, "https://example.invalid/mingw-w64.rb"),
        (
            PINNED_FORMULA_INSTALL,
            'brew tap-new --no-git "${MINGW_W64_TAP}"\n          brew install "${MINGW_W64_TAP}/mingw-w64"',
        ),
        ('if [ ! -x "${DLLTOOL_PATH}" ]; then', "# dlltool executable check removed"),
        (
            'if [ "${ACTUAL_DLLTOOL_VERSION}" != "${EXPECTED_DLLTOOL_VERSION}" ]; then',
            "# dlltool version check removed",
        ),
        (
            'test "$(command -v x86_64-w64-mingw32-dlltool)" = "${DLLTOOL_PATH}"',
            "# Windows build-time dlltool path probe removed",
        ),
        (
            'EXPECTED_DLLTOOL_VERSION="GNU ${DLLTOOL_PATH} (GNU Binutils) ${EXPECTED_BINUTILS_VERSION}.${EXPECTED_BINUTILS_BUILD_DATE}"',
            'EXPECTED_DLLTOOL_VERSION="GNU ${DLLTOOL_PATH} (GNU Binutils) 2.47.20260725"',
        ),
        (
            'printf \'DLLTOOL_PATH=%s\\nEXPECTED_DLLTOOL_VERSION=%s\\n\' "${DLLTOOL_PATH}" "${EXPECTED_DLLTOOL_VERSION}" >> "${GITHUB_ENV}"',
            "# step-local dlltool variables are not exported",
        ),
        (
            '            exit 1\n          fi\n          DLLTOOL_PATH=',
            '            # missing dlltool no longer stops before compile\n          fi\n          DLLTOOL_PATH=',
        ),
    ):
        mutated = text.replace(token, replacement, 1)
        try:
            _verify_dlltool_contract(mutated)
        except SystemExit:
            continue
        fail(f"dlltool contract mutation survived: {token}")

    untrusted_tap = text.replace(
        'brew tap-new "${MINGW_W64_TAP}"',
        'brew tap-new --no-git "${MINGW_W64_TAP}"',
        1,
    )
    try:
        _verify_dlltool_contract(untrusted_tap)
    except SystemExit as error:
        if "untrusted or unpinned tap installation is forbidden" not in str(error):
            raise
    else:
        fail("untrusted no-git tap installation mutation survived")


def _verify_attest_verifier_checkout(text: str) -> None:
    job = re.search(r"(?ms)^  attest-and-verify:\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)", text)
    if job is None:
        fail("attest-and-verify job is missing")
    job_text = job.group(1)
    if not re.search(r"(?m)^      contents: read$", job_text):
        fail("attest-and-verify must retain read-only repository contents permission")
    checkout = re.search(
        r"(?ms)^      - name: Checkout repository verifier without persisted credentials\n"
        r"(?P<step>.*?)(?=^      - name:|\Z)",
        job_text,
    )
    if checkout is None:
        fail("attest-and-verify must checkout the repository verifier")
    step = checkout.group("step")
    if not re.search(
        r"(?m)^        uses: actions/checkout@[0-9a-f]{40}(?:\s+#.*)?$", step
    ):
        fail("attest-and-verify verifier checkout must use a full-SHA-pinned action")
    if not re.search(r"(?m)^          persist-credentials: false$", step):
        fail("attest-and-verify verifier checkout must not persist credentials")
    download = job_text.find("- name: Download ephemeral build inventory")
    verify = job_text.find("python3 scripts/verify_i2050_release_dryrun_contract.py provenance")
    if download < 0 or verify < 0 or checkout.start() > download or download > verify:
        fail("checkout must precede the downloaded inventory and provenance verifier")


def _attest_verifier_checkout_mutation_self_test(text: str) -> None:
    checkout_name = "      - name: Checkout repository verifier without persisted credentials\n"
    start = text.find(checkout_name)
    if start < 0:
        fail("attest-and-verify verifier checkout is missing")
    end = text.find("      - name:", start + len(checkout_name))
    if end < 0:
        fail("attest-and-verify verifier checkout step is not bounded")
    without_checkout = text[:start] + text[end:]
    try:
        _verify_attest_verifier_checkout(without_checkout)
    except SystemExit as error:
        if "must checkout the repository verifier" not in str(error):
            raise
    else:
        fail("missing attest-and-verify checkout mutation survived")

    checkout_start = text.find(checkout_name)
    checkout_end = text.find("      - name:", checkout_start + len(checkout_name))
    checkout_step = text[checkout_start:checkout_end]
    persisted_credentials = text[:checkout_start] + checkout_step.replace(
        "          persist-credentials: false", "          persist-credentials: true", 1
    ) + text[checkout_end:]
    try:
        _verify_attest_verifier_checkout(persisted_credentials)
    except SystemExit as error:
        if "must not persist credentials" not in str(error):
            raise
    else:
        fail("persisted-credential checkout mutation survived")


def contract() -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    changed = subprocess.run(
        ["git", "diff", "--name-only", "origin/main...HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    event_name = os.environ.get("GITHUB_EVENT_NAME", "pull_request")
    if event_name == "pull_request":
        _slice_routing_mutation_self_test()
        if _verify_i2050_pr_slice(set(changed)):
            print("PASS: relevant CLI dry-run change enforces the isolated workflow slice")
        else:
            print("SKIP: CLI dry-run slice isolation (neither slice file changed)")
    elif event_name == "workflow_dispatch":
        print("PASS: manual dispatch validates contracts without PR slice routing")
    else:
        fail(f"unsupported event for contract validation: {event_name}")
    hosted_contract = Path(".github/workflows/issue-1724-cli-provenance.yml").read_text(
        encoding="utf-8"
    )
    for token in (
        '".github/workflows/issue-2050-cli-release-dry-run.yml"',
        '"scripts/verify_i2050_release_dryrun_contract.py"',
        '"Cargo.lock"',
        '"rust-toolchain.toml"',
        "fetch-depth: 0",
        "persist-credentials: false",
        "workflow_dispatch:",
        "python3 -S scripts/verify_i2050_release_dryrun_contract.py contract",
    ):
        if token not in hosted_contract:
            fail(f"the existing read-only hosted PR lane does not run the contract: {token}")
    contract_job = re.search(
        r"(?ms)^  contract:\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)", hosted_contract
    )
    if contract_job is None or re.search(r"^\s+if:", contract_job.group(1), re.MULTILINE):
        fail("the CLI provenance contract job must remain enabled for PR and manual dispatch")
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
    _verify_attest_verifier_checkout(text)
    _attest_verifier_checkout_mutation_self_test(text)
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
    for token in (
        "if: github.event_name == 'pull_request'",
        "github.event_name == 'workflow_dispatch'",
    ):
        if token not in text:
            fail(f"manual dry-run dispatch semantics changed: {token}")
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
    _verify_zigbuild_version_probe(text)
    _zigbuild_version_probe_mutation_self_test(text)
    print("PASS: workflow-only, verifier-only, both, mixed, and unrelated-only routing cases")
    _verify_dlltool_contract(text)
    _dlltool_contract_mutation_self_test(text)
    print("PASS: pinned MinGW-w64 toolchain, dlltool path/version probes, and mutation checks")
    print("PASS: attestation verifier checkout is pinned, credentialless, and precedes artifact download")
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
