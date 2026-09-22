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


def _verified_record() -> dict[str, object]:
    record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
    record["evidence_state"] = "VERIFIED"
    record["tenant_redacted"] = "tenant-redacted"
    record["runtime"] = {
        "provider": "aws",
        "image_digest": "sha256:" + ("a" * 64),
        "execution_region": "us-east-1",
    }
    record["external_prerequisites"] = {
        name: "PROVISIONED" for name in record["external_prerequisites"]
    }
    for name, receipt in record["lifecycle"].items():
        receipt.update(
            status="PASS",
            receipt_reference=f"audit://{name}/receipt-001",
            completed_at="2026-09-22T00:00:00Z",
            blocker=None,
        )
    return record


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

        verified = _verified_record()
        path.write_text(json.dumps(verified), encoding="utf-8")
        verified_green = _run(tree)
        assert verified_green.returncode == 0, verified_green.stdout + verified_green.stderr

        verified["external_prerequisites"]["isolated_test_tenant"] = "MISSING"
        path.write_text(json.dumps(verified), encoding="utf-8")
        missing_prerequisite = _run(tree)
        assert missing_prerequisite.returncode != 0
        assert "requires every external prerequisite" in missing_prerequisite.stderr

        verified = _verified_record()
        verified["lifecycle"]["rotate"]["receipt_reference"] = "audit://wrap_unwrap/receipt-001"
        path.write_text(json.dumps(verified), encoding="utf-8")
        reused_receipt = _run(tree)
        assert reused_receipt.returncode != 0
        assert "audit://rotate/ receipt_reference" in reused_receipt.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["lifecycle"]["wrap_unwrap"]["blocker"] = "password=not-allowed"
        path.write_text(json.dumps(record), encoding="utf-8")
        password_shape = _run(tree)
        assert password_shape.returncode != 0
        assert "credential or private-key material" in password_shape.stderr


if __name__ == "__main__":
    test_b083_lifecycle_evidence_green_then_named_red_mutants()
    print("B083 lifecycle evidence adversarial: green baseline and named red mutants")
