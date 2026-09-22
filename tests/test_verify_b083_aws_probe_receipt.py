"""Adversarial contract for B-083's redacted isolated AWS KMS probe."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path("scripts/verify_b083_aws_probe_receipt.py")
EVIDENCE = Path("evidence/owner-actions/B-083/aws-kms-isolated-probe.json")


def _run(tree: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(SCRIPT)], cwd=tree, text=True, capture_output=True, check=False)


def test_aws_probe_receipt_green_then_named_red_mutants() -> None:
    with tempfile.TemporaryDirectory(prefix="b083-aws-probe-") as raw:
        tree = Path(raw)
        for relative in (SCRIPT, EVIDENCE):
            target = tree / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        path = tree / EVIDENCE

        green = _run(tree)
        assert green.returncode == 0, green.stdout + green.stderr
        assert "partial provider boundary" in green.stdout

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["schema_version"] = True
        path.write_text(json.dumps(record), encoding="utf-8")
        bool_schema = _run(tree)
        assert bool_schema.returncode != 0
        assert "must be the integer 1" in bool_schema.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["conclusion"]["closure_eligibility"] = "ELIGIBLE"
        path.write_text(json.dumps(record), encoding="utf-8")
        forged_closure = _run(tree)
        assert forged_closure.returncode != 0
        assert "never eligible to close" in forged_closure.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["observations"]["rotation"].update(status="PASS", blocker=None)
        path.write_text(json.dumps(record), encoding="utf-8")
        forged_rotation = _run(tree)
        assert forged_rotation.returncode != 0
        assert "must remain NOT_COMPLETED" in forged_rotation.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["observations"]["wrap_unwrap"]["audit_receipts"] = []
        path.write_text(json.dumps(record), encoding="utf-8")
        missing_receipt = _run(tree)
        assert missing_receipt.returncode != 0
        assert "must not be empty" in missing_receipt.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["observations"]["context_rejection"]["audit_receipts"] = [
            record["observations"]["wrap_unwrap"]["audit_receipts"][0]
        ]
        path.write_text(json.dumps(record), encoding="utf-8")
        reused_receipt = _run(tree)
        assert reused_receipt.returncode != 0
        assert "reuses a receipt from another observation" in reused_receipt.stderr

        record = json.loads((ROOT / EVIDENCE).read_text(encoding="utf-8"))
        record["observations"]["wrap_unwrap"]["detail"] = "AKIA1234567890ABCDEF"
        path.write_text(json.dumps(record), encoding="utf-8")
        credential_shape = _run(tree)
        assert credential_shape.returncode != 0
        assert "credential-shaped" in credential_shape.stderr


if __name__ == "__main__":
    test_aws_probe_receipt_green_then_named_red_mutants()
    print("B083 AWS probe receipt adversarial: green baseline and named red mutants")
