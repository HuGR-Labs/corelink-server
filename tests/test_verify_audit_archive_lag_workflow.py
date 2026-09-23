from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_audit_archive_lag_workflow.py"
spec = importlib.util.spec_from_file_location("audit_archive_lag_workflow_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


def test_workflow_meets_protected_manual_no_page_contract() -> None:
    verifier.verify((ROOT / verifier.WORKFLOW).read_text(encoding="utf-8"))


def test_adversarial_workflow_mutations_are_rejected() -> None:
    source = (ROOT / verifier.WORKFLOW).read_text(encoding="utf-8")
    verifier.verify_adversarial_mutations(source)
