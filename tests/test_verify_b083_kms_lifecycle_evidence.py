"""Adversarial contract for B-083's provider-neutral KMS evidence index."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path("scripts/verify_b083_kms_lifecycle_evidence.py")
EVIDENCE = Path("evidence/owner-actions/B-083/byok-real-kms-lifecycle.json")


def _run(tree: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(SCRIPT)], cwd=tree, text=True, capture_output=True, check=False)


def _copy_contract(destination: Path) -> Path:
    for relative in (SCRIPT, EVIDENCE):
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)
    return destination / EVIDENCE


def test_b083_lifecycle_evidence_green_then_named_red_mutants() -> None:
    with tempfile.TemporaryDirectory(prefix="b083-kms-evidence-") as raw:
        tree = Path(raw)
        path = _copy_contract(tree)
        green = _run(tree)
        assert green.returncode == 0, green.stdout + green.stderr
        assert "typed external blocker" in green.stdout

        record = json.loads(path.read_text(encoding="utf-8"))
        record["lifecycle"]["wrap_unwrap"]["status"] = "PASS"
        record["lifecycle"]["wrap_unwrap"]["blocker"] = None
        path.write_text(json.dumps(record), encoding="utf-8")
        forged_completion = _run(tree)
        assert forged_completion.returncode != 0
        assert "cannot PASS while evidence_state is BLOCKED" in forged_completion.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["external_prerequisites"]["durable_audit_receipt_sink"] = "PROVISIONED"
        path.write_text(json.dumps(record), encoding="utf-8")
        incomplete_blocker = _run(tree)
        assert incomplete_blocker.returncode != 0
        assert "every exact external prerequisite as MISSING" in incomplete_blocker.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["lifecycle"]["wrap_unwrap"]["receipt_reference"] = "AKIA1234567890ABCDEF"
        path.write_text(json.dumps(record), encoding="utf-8")
        secret_material = _run(tree)
        assert secret_material.returncode != 0
        assert "credential or private-key material" in secret_material.stderr


if __name__ == "__main__":
    test_b083_lifecycle_evidence_green_then_named_red_mutants()
    print("B083 lifecycle evidence adversarial: green baseline and named red mutants")
