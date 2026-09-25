#!/usr/bin/env python3
"""Fail-closed static contract for the credentialless Windows readiness probe."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROBE_WORKFLOW = ROOT / ".github/workflows/issue-2586-windows-readiness.yml"
CI_WORKFLOW = ROOT / ".github/workflows/issue-2586-windows-contract.yml"
SIGN_WORKFLOW = ROOT / ".github/workflows/sign-windows.yml"
SCRIPT = ROOT / "scripts/windows_signing_readiness.ps1"
EXPECTED_SECRETS = [
    "WINDOWS_CODE_SIGNING_CERT",
    "WINDOWS_CODE_SIGNING_PASSWORD",
    "WINDOWS_CODE_SIGNING_FINGERPRINT",
    "WINDOWS_CODE_SIGNING_SUBJECT",
]
EXPECTED_PUBLIC_VARS = [
    "WINDOWS_CODE_SIGNING_ISSUER",
    "WINDOWS_CODE_SIGNING_NOT_BEFORE",
    "WINDOWS_CODE_SIGNING_EXPIRES_AT",
    "WINDOWS_SIGNING_RENEWAL_OWNER",
    "WINDOWS_SIGNING_RENEWAL_DATE",
]
RECEIPT_FIELDS = [
    "schema_version", "evidence_type", "repository", "workflow", "commit_sha",
    "run_id", "run_url", "observed_at", "actor_role", "approved_operation",
    "certificate", "chain_revocation", "timestamp_policy", "result",
    "artifact_signature", "final_byte_verification", "renewal",
]
ALLOWED_PATHS = {
    ".github/workflows/issue-2586-windows-readiness.yml",
    ".github/workflows/issue-2586-windows-contract.yml",
    "scripts/windows_signing_readiness.ps1",
    "scripts/verify_i2586_windows_readiness.py",
    "tests/test_i2586_windows_readiness.py",
}


def top_level_on_triggers(text: str) -> list[str]:
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if re.fullmatch(r"on:\s*", line)), None)
    if start is None:
        return []
    keys: list[str] = []
    for line in lines[start + 1 :]:
        if line and not line[0].isspace() and not line.startswith("#"):
            break
        match = re.match(r"^  ([A-Za-z_][A-Za-z0-9_-]*):(?:\s|$)", line)
        if match:
            keys.append(match.group(1))
    return keys


def workflow_secret_names(text: str) -> list[str]:
    return re.findall(r"^\s{10}(WINDOWS_CODE_SIGNING_[A-Z_]+):\s*\$\{\{\s*secrets\.(WINDOWS_CODE_SIGNING_[A-Z_]+)\s*}}\s*$", text, re.M)


def workflow_public_var_names(text: str) -> list[tuple[str, str]]:
    return re.findall(r"^\s{10}(WINDOWS_[A-Z_]+):\s*\$\{\{\s*vars\.(WINDOWS_[A-Z_]+)\s*}}\s*$", text, re.M)


def signer_contract_names(text: str) -> list[str]:
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if re.fullmatch(r"    secrets:\s*", line)), None)
    if start is None:
        return []
    names: list[str] = []
    for line in lines[start + 1 :]:
        if line and (not line[0].isspace() or len(line) - len(line.lstrip()) <= 4):
            break
        if line.startswith("      ") and not line.startswith("        "):
            match = re.fullmatch(r"      (WINDOWS_CODE_SIGNING_[A-Z_]+):\s*(?:#.*)?", line)
            if match:
                names.append(match.group(1))
            elif "WINDOWS_CODE_SIGNING_" in line and not line.lstrip().startswith("#"):
                names.append("<invalid-contract-entry>")
            elif "WINDOWS_CODE_SIGNING_" in line and line.lstrip().startswith("#"):
                names.append("<commented-contract-entry>")
    return names


def validate_texts(probe: str, ci: str, signer: str, script: str) -> list[str]:
    errors: list[str] = []
    if top_level_on_triggers(probe) != ["workflow_dispatch"]:
        errors.append("readiness workflow must have only the manual workflow_dispatch trigger")
    for required in (
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.repository_id == '1232040291'",
        "github.ref == 'refs/heads/main' && github.ref_protected",
        "runs-on: windows-2022",
        "environment: production",
        "permissions:\n  contents: read",
        "persist-credentials: false",
        "-ExpectedOperation 'authenticode-credential-binding-metadata-only'",
        "-TimestampPolicy 'DigiCert RFC 3161 http://timestamp.digicert.com; SHA-256 file and timestamp digests; not invoked'",
    ):
        if required not in probe:
            errors.append(f"readiness workflow missing protected boundary: {required}")
    actual_secret_refs = [secret for _binding, secret in workflow_secret_names(probe)]
    actual_secret_bindings = [binding for binding, _secret in workflow_secret_names(probe)]
    if actual_secret_refs != EXPECTED_SECRETS or actual_secret_bindings != EXPECTED_SECRETS:
        errors.append("readiness workflow secret bindings must exactly match the four contracted names in order")
    actual_vars = workflow_public_var_names(probe)
    if [value for _binding, value in actual_vars] != EXPECTED_PUBLIC_VARS or [binding for binding, _value in actual_vars] != EXPECTED_PUBLIC_VARS:
        errors.append("readiness workflow public metadata and renewal vars must match approved names in order")
    if signer_contract_names(signer) != EXPECTED_SECRETS:
        errors.append("sign-windows workflow_call contract changed, was reordered, duplicated, commented, or renamed")
    if "http://timestamp.digicert.com" not in signer or "/fd SHA256" not in signer or "/td SHA256" not in signer:
        errors.append("DigiCert RFC 3161 and SHA-256 policy no longer matches the signing workflow contract")
    if re.search(r"secrets\.(?!WINDOWS_CODE_SIGNING_(?:CERT|PASSWORD|FINGERPRINT|SUBJECT)\b)", probe):
        errors.append("readiness workflow references a secret outside the four-name contract")
    if re.search(r"(?i)(upload-artifact|upload-release-asset|gh\s+(?:release|api)|GITHUB_TOKEN|id-token:|signing[_ -]?key)", probe):
        errors.append("readiness workflow contains a publication, token, or signing surface")

    if top_level_on_triggers(ci) != ["workflow_dispatch"]:
        errors.append("credentialless CI workflow must be manual-only")
    for required in (
        "type: string",
        "EXPECTED_SHA: ${{ inputs.candidate_sha }}",
        "TARGET_BASE_SHA: ${{ inputs.target_base_sha }}",
        '[[ "$GITHUB_SHA" == "$EXPECTED_SHA" ]]',
        '[[ "$(git rev-parse HEAD)" == "$EXPECTED_SHA" ]]',
        "runs-on: ubuntu-24.04",
        "runs-on: windows-2022",
        "python3 -S -m unittest -q tests/test_i2586_windows_readiness.py",
        "scripts/windows_signing_readiness.ps1 -SelfTest",
        "ref: ${{ inputs.candidate_sha }}",
        "persist-credentials: false",
    ):
        if required not in ci:
            errors.append(f"credentialless CI pack missing exact-head or focused check: {required}")
    if ci.count("ref: ${{ inputs.candidate_sha }}") != 2:
        errors.append("both credentialless runner jobs must check out the exact candidate SHA")
    if re.search(r"(?i)(secrets\.|vars\.|sign-windows\.yml|issue-2586-windows-readiness\.yml\s+-|dispatch.*readiness)", ci):
        errors.append("credentialless CI pack accesses secrets or executes the readiness workflow")
    for path in sorted(ALLOWED_PATHS):
        if path not in ci:
            errors.append(f"credentialless CI path boundary omits {path}")

    for forbidden in (
        r"(?i)\bsigntool\b", r"(?i)\bosslsigncode\b", r"(?i)\bopenssl\b",
        r"(?i)\bgh\s+(?:release|api)\b", r"(?i)actions/upload-",
        r"(?i)\b(?:Invoke-WebRequest|Invoke-RestMethod|Start-BitsTransfer)\b",
        r"(?i)\b(?:Set-AuthenticodeSignature|Sign-File|Timestamp-File)\b",
        r"(?i)\b(?:Export-PfxCertificate|Export-Certificate)\b",
    ):
        if re.search(forbidden, probe + "\n" + ci + "\n" + script):
            errors.append(f"forbidden signing, export, or publication command found: {forbidden}")
    if not re.search(r"(?is)finally\s*\{.*?if\s*\(\$null\s*-ne\s*\$pfxBytes\)\s*\{\s*\[Array\]::Clear\(\$pfxBytes,\s*0,\s*\$pfxBytes\.Length\)\s*\}.*?\[Environment\]::SetEnvironmentVariable\(\$name,\s*\$null\)", script):
        errors.append("PowerShell implementation lacks finally cleanup for in-memory PFX bytes and secret environment values")
    if re.search(r"(?i)(New-TemporaryFile|WriteAllBytes|WriteAllText|Set-Content.+(?:\.pfx|\.p12)|ConvertTo-SecureString.+File)", script):
        errors.append("PowerShell implementation must not persist certificate or password material to disk")
    if not re.search(r"(?is)RevocationMode\s*=.*?::Online", script) or not re.search(r"(?is)RevocationFlag\s*=.*?::EntireChain", script):
        errors.append("PowerShell implementation must check the full chain online for revocation")
    if "EphemeralKeySet" not in script or "HasPrivateKey" not in script or "GetRSAPrivateKey" not in script:
        errors.append("PowerShell implementation must import ephemerally and verify private-key association")
    schema = re.search(r"(?is)\$script:ReceiptFields\s*=\s*@\((.*?)\)", script)
    actual_fields = re.findall(r"'([a-z_]+)'", schema.group(1)) if schema else []
    if actual_fields != RECEIPT_FIELDS:
        errors.append("PowerShell receipt fields do not exactly match the approved readiness schema")
    return errors


def validate(root: Path = ROOT) -> list[str]:
    paths = [
        root / ".github/workflows/issue-2586-windows-readiness.yml",
        root / ".github/workflows/issue-2586-windows-contract.yml",
        root / ".github/workflows/sign-windows.yml",
        root / "scripts/windows_signing_readiness.ps1",
    ]
    missing = [str(path.relative_to(root)) for path in paths if not path.is_file()]
    if missing:
        return [f"required contract file missing: {path}" for path in missing]
    return validate_texts(*(path.read_text(encoding="utf-8") for path in paths))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run verifier negative controls")
    args = parser.parse_args()
    if args.self_test:
        probe = PROBE_WORKFLOW.read_text(encoding="utf-8")
        ci = CI_WORKFLOW.read_text(encoding="utf-8")
        signer = SIGN_WORKFLOW.read_text(encoding="utf-8")
        script = SCRIPT.read_text(encoding="utf-8")
        accepted = validate_texts(probe, ci, signer, script)
        if accepted:
            print("self-test baseline unexpectedly rejected: " + "; ".join(accepted), file=sys.stderr)
            return 1
        mutations = (
            (probe.replace("  workflow_dispatch:\n", "  push:\n    branches: [main]\n  workflow_dispatch:\n", 1), ci, signer, script),
            (probe.replace("secrets.WINDOWS_CODE_SIGNING_CERT", "secrets.WINDOWS_CODE_SIGNING_PASSWORD", 1), ci, signer, script),
            (probe, ci + "\n      - name: unsafe\n        run: gh release publish\n", signer, script),
            (probe, ci, signer.replace("      WINDOWS_CODE_SIGNING_PASSWORD:", "      # WINDOWS_CODE_SIGNING_PASSWORD:", 1), script),
            (probe, ci, signer.replace("      WINDOWS_CODE_SIGNING_FINGERPRINT:", "      WINDOWS_CODE_SIGNING_SUBJECT:\n      WINDOWS_CODE_SIGNING_FINGERPRINT:", 1), script),
            (probe, ci, signer.replace("WINDOWS_CODE_SIGNING_FINGERPRINT", "__TEMP_FINGERPRINT__", 1).replace("WINDOWS_CODE_SIGNING_SUBJECT", "WINDOWS_CODE_SIGNING_FINGERPRINT", 1).replace("__TEMP_FINGERPRINT__", "WINDOWS_CODE_SIGNING_SUBJECT", 1), script),
            (probe, ci, signer, script.replace("$null -ne $pfxBytes", "$false -and $null -ne $pfxBytes", 1)),
            (probe, ci, signer, script.replace("GetRSAPrivateKey", "GetPublicKey", 1)),
            (probe, ci.replace("ref: ${{ inputs.candidate_sha }}", "ref: main", 1), signer, script),
        )
        for index, fixture in enumerate(mutations, start=1):
            if not validate_texts(*fixture):
                print(f"self-test negative control {index} was accepted", file=sys.stderr)
                return 1
        print("issue-2586 verifier negative controls passed")
        return 0
    errors = validate()
    if errors:
        print("issue-2586 Windows readiness contract failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("issue-2586 Windows readiness contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
