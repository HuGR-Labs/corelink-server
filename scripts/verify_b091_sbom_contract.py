#!/usr/bin/env python3
"""Verify the committed SBOM against Cargo.lock without invoking Cargo."""
from __future__ import annotations
import importlib.util
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def active_lines(text: str) -> list[str]:
    lines = []
    dead: list[int] = []
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if stripped and not stripped.startswith("#"):
            indent = len(raw) - len(stripped)
            while dead and indent < dead[-1]: dead.pop()
            if re.fullmatch(r"if:\s*(?:false|no|0)\s*", stripped, re.I):
                dead.append(indent)
                continue
            if dead:
                continue
            lines.append(raw)
    return lines


def validate_metadata(cargo_text: str, workflow_text: str) -> None:
    cargo_lines = active_lines(cargo_text)
    licenses = [line for line in cargo_lines if re.fullmatch(
        r'license\s*=\s*"([^"]+)"', line.strip())]
    if len(licenses) != 1 or licenses[0].strip() != 'license = "LicenseRef-CoreLink-Proprietary"':
        raise RuntimeError("active workspace license is missing or ambiguous")
    if any("UNLICENSED" in line for line in cargo_lines):
        raise RuntimeError("invalid UNLICENSED license remains active")
    placeholder = re.compile(r"placeholder[^\s-]*[\s-]*pin|pin[^\s-]*[\s-]*placeholder", re.I)
    if any(placeholder.search(line) for line in active_lines(workflow_text)):
        raise RuntimeError("placeholder integrity pin remains active")


def verify_metadata() -> None:
    cargo = ROOT / "Cargo.toml"
    workflow = ROOT / ".github/workflows/sbom.yml"
    if any(path.is_symlink() or not path.is_file() for path in (cargo, workflow)):
        raise RuntimeError("SBOM metadata target is missing")
    validate_metadata(cargo.read_text(encoding="utf-8"), workflow.read_text(encoding="utf-8"))


def validate_workflow(text: str) -> None:
    lines = active_lines(text)
    if not lines or "on:" not in {line.strip() for line in lines}:
        raise RuntimeError("SBOM workflow is empty or has no active trigger block")
    if not any(re.fullmatch(r"\s{2}release:\s*", line) for line in lines):
        raise RuntimeError("release trigger is missing")
    if not any(re.fullmatch(r"\s{2}workflow_dispatch:\s*", line) for line in lines):
        raise RuntimeError("workflow_dispatch trigger is missing")
    required_jobs = ("sbom-generate", "sbom-ntia-validate", "sbom-tsa-attest")
    for job in required_jobs:
        if not any(re.fullmatch(rf"\s{{2}}{job}:\s*", line) for line in lines):
            raise RuntimeError(f"SBOM job missing or inactive: {job}")
    if not any(line.strip() == "needs: sbom-generate" for line in lines):
        raise RuntimeError("NTIA job is not wired to generated artifact")
    joined = "\n".join(lines)
    for needle in (
        "tests/verify_rust_sbom.py --check",
        "cp .sbom/cyclonedx-rust.json sbom.cdx.json",
        "actions/upload-artifact@",
        "name: sbom-cdx-json",
        "path: sbom.cdx.json",
    ):
        if needle not in joined:
            raise RuntimeError(f"active SBOM wiring missing: {needle}")
    validate_starts = [
        index for index, line in enumerate(lines)
        if line.strip() == "./cyclonedx-cli validate \\"
    ]
    if len(validate_starts) != 1:
        raise RuntimeError("active SBOM wiring must contain exactly one CycloneDX validate invocation")
    validate_lines = [lines[validate_starts[0]].strip()]
    while validate_lines[-1].endswith("\\"):
        next_index = validate_starts[0] + len(validate_lines)
        if next_index >= len(lines):
            raise RuntimeError("CycloneDX validate invocation has a dangling continuation")
        validate_lines.append(lines[next_index].strip())
    validate_command = " ".join(validate_lines)
    for needle in (
        "./cyclonedx-cli validate",
        "--input-file sbom.cdx.json",
        "--input-format json",
        "--input-version v1_5",
        "--fail-on-errors",
    ):
        if needle not in validate_command:
            raise RuntimeError(f"strict CycloneDX validate invocation missing: {needle}")
    # CycloneDX CLI 0.27.2 validates a BOM with an explicit input format and
    # schema version. These flags belonged to a different/older command
    # contract and must never silently return to the release gate.
    for unsupported in ("--minimum-required-fields", "--output-format text"):
        if unsupported in validate_command:
            raise RuntimeError(f"unsupported CycloneDX validate flag is active: {unsupported}")


def self_test() -> None:
    cargo = 'license = "LicenseRef-CoreLink-Proprietary"\n'
    workflow = 'env:\n  CLI_SHA256: "real"\n'
    try: validate_metadata(cargo.replace("LicenseRef-CoreLink-Proprietary", "UNLICENSED"), workflow)
    except RuntimeError: pass
    else: raise RuntimeError("invalid-license mutation passed")
    try: validate_metadata(cargo, workflow.replace("real", "placeholder-pin"))
    except RuntimeError: pass
    else: raise RuntimeError("placeholder mutation passed")
    workflow = (ROOT / ".github/workflows/sbom.yml").read_text(encoding="utf-8")
    for label, mutation in (
        ("empty workflow", "name: empty\n"),
        ("trigger comment bait", workflow.replace("  release:\n", "  # release:\n", 1)),
        ("verify step comment bait", workflow.replace("          \"$SBOM_PYTHON\" tests/verify_rust_sbom.py --check", "          # \"$SBOM_PYTHON\" tests/verify_rust_sbom.py --check", 1)),
        ("dead SBOM job", workflow.replace("  sbom-generate:\n", "  sbom-generate:\n    if: false\n", 1)),
        ("legacy CycloneDX minimum-fields flag", workflow.replace("            --input-format json \\\n", "            --minimum-required-fields ntia \\\n", 1)),
        ("legacy CycloneDX output-format flag", workflow.replace("            --fail-on-errors\n", "            --output-format text\n", 1)),
    ):
        try: validate_workflow(mutation)
        except RuntimeError: pass
        else: raise RuntimeError(f"{label} mutation passed")

def main() -> int:
    verify_metadata()
    self_test()
    workflow = ROOT / ".github/workflows/sbom.yml"
    validate_workflow(workflow.read_text(encoding="utf-8"))
    target = ROOT / "tests/verify_rust_sbom.py"
    if target.is_symlink() or not target.is_file():
        raise RuntimeError("SBOM verifier missing or non-regular")
    spec = importlib.util.spec_from_file_location("sbom_contract", target)
    if spec is None or spec.loader is None: raise RuntimeError("SBOM verifier missing")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)
    lock = module.lock_packages()
    if len(lock) == 0: raise RuntimeError("Cargo.lock package population is empty")
    path = ROOT / ".sbom/cyclonedx-rust.json"
    if path.is_symlink() or not path.is_file():
        raise RuntimeError("committed SBOM missing or non-regular")
    try: sbom = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error: raise RuntimeError(f"committed SBOM invalid: {error}") from error
    module.verify(sbom)
    if len(sbom.get("components", [])) != len(lock): raise RuntimeError("SBOM component count differs from Cargo.lock")
    print(f"B-091 semantic check PASS: {len(lock)} Cargo.lock identities represented with checksums/licenses")
    return 0

if __name__ == "__main__":
    try: raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, TypeError) as error:
        print(f"B-091 semantic check FAILED: {error}", file=sys.stderr); raise SystemExit(1)
