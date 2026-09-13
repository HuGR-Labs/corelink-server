"""Mutation and closed-world tests for the bounded B-087 questionnaire guard."""

from __future__ import annotations

import importlib.util
import shutil
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b087_questionnaires", ROOT / "scripts/verify_b087_questionnaires.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def fixture_tree(tmp_path: Path) -> Path:
    """Copy only the bounded guard population and its source controls."""
    for relative, _ in MODULE.SOURCE_CHECKS:
        source = ROOT / relative
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for relative in (MODULE.CAIQ, MODULE.SIG, MODULE.OWNER_ACTIONS):
        source = ROOT / relative
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    return tmp_path


def mutate_line(path: Path, needle: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    assert text.count(needle) >= 1, needle
    path.write_text(text.replace(needle, replacement, 1), encoding="utf-8")


def mutate_all(path: Path, needle: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    assert text.count(needle) >= 1, needle
    path.write_text(text.replace(needle, replacement), encoding="utf-8")


def test_live_population_passes_with_owner_actions_only() -> None:
    result = MODULE.verify(ROOT)
    assert result["ok"] is True
    assert result["population"] == {"caiq": 26, "sig_lite": 17}
    assert len(result["owner_actions"]) == 4


def test_backlog_verify_means_matches_guard_population() -> None:
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    section = backlog.split("### B-087 —", 1)[1].split("### B-088 —", 1)[0]
    assert "26 CAIQ and 17 SIG-LITE rows" in section
    assert "status: done" in section


@pytest.mark.parametrize(
    ("document", "needle", "replacement", "reason"),
    [
        (
            MODULE.CAIQ,
            "| AIS-04.1 | Application security testing performed? | P |",
            "| AIS-04.1 | Application security testing performed? | Y |",
            "unsupported answer",
        ),
        (
            MODULE.CAIQ,
            "| CEK-10.1 | Kill-switch / key-revocation supported? | N |",
            "| CEK-10.1 | Kill-switch / key-revocation supported? | Y |",
            "unsupported answer",
        ),
        (
            MODULE.SIG,
            "| J.4 | Is 24×7 incident detection in place? | P |",
            "| J.4 | Is 24×7 incident detection in place? | Y |",
            "unsupported answer",
        ),
    ],
)
def test_restoring_unsupported_positive_answer_fails(
    tmp_path: Path, document: Path, needle: str, replacement: str, reason: str
) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(root / document, needle, replacement)
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any(reason in failure for failure in result["failures"])


def test_restoring_stale_sast_claim_fails_for_claim_reason(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(
        root / MODULE.SIG,
        "| G.10 | Are vulnerability scans performed at least quarterly? | P | **Partial cadence.",
        "| G.10 | Are vulnerability scans performed at least quarterly? | P | CodeQL + Semgrep custom rules on every PR.",
    )
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("unsupported positive claim" in failure for failure in result["failures"])


def test_missing_or_renamed_row_fails_closed(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / MODULE.CAIQ
    text = path.read_text(encoding="utf-8")
    original = "| STA-11.1 | Build provenance verifiable? | N |"
    assert text.count(original) == 1
    path.write_text(text.replace(original, "| STA-11-renamed | Build provenance verifiable? | N |"), encoding="utf-8")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("missing named row" in failure and "STA-11.1" in failure for failure in result["failures"])


@pytest.mark.parametrize("document", [MODULE.CAIQ, MODULE.SIG, MODULE.OWNER_ACTIONS])
def test_reintroducing_test_only_byok_provider_claim_fails_closed(
    tmp_path: Path, document: Path
) -> None:
    root = fixture_tree(tmp_path)
    path = root / document
    text = path.read_text(encoding="utf-8")
    marker = "byok-aws-real"
    assert marker in text
    path.write_text(text.replace(marker, "InMemoryFake", 1), encoding="utf-8")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("false default BYOK provider claim" in failure for failure in result["failures"])


def test_missing_shipped_reality_marker_fails_closed(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "crates/corelink-container/src/routes/byok_admin.rs"
    text = path.read_text(encoding="utf-8")
    assert text.count("byok_not_available") >= 1
    path.write_text(text.replace("byok_not_available", "byok_unavailable_mutation"), encoding="utf-8")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("source control changed or missing" in failure for failure in result["failures"])


def test_admin_fail_closed_branch_is_code_and_not_comment_safe(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "crates/corelink-container/src/routes/byok_admin.rs"
    mutate_all(path, "crate::byok_orchestrator::make_provider().await", "provider_constructor_removed()")
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n// crate::byok_orchestrator::make_provider().await\n")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("byok_admin.rs" in failure for failure in result["failures"])


def test_orchestrator_unavailable_branch_is_code_and_not_string_safe(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "crates/corelink-container/src/byok_orchestrator.rs"
    mutate_all(path, "ActiveProvider::Unavailable", "ActiveProvider::NoProvider")
    with path.open("a", encoding="utf-8") as handle:
        handle.write('\nconst DECOY: &str = "ActiveProvider::Unavailable";\n')
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("ActiveProvider::Unavailable" in failure for failure in result["failures"])


def test_shipped_aws_feature_is_required_not_comment_bait(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "Dockerfile"
    mutate_line(path, "--features byok-aws-real;", ";")
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n# cargo build -p corelink-server --bin corelink-server --features byok-aws-real;\n")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("Dockerfile: production corelink-server" in failure for failure in result["failures"])


def test_byok_receipt_transition_forces_questionnaire_review(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "evidence/owner-actions/B-083/byok-real-kms-lifecycle.json"
    mutate_line(path, '"status": "NOT_EXECUTED"', '"status": "PASS"')
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("runtime BYOK proof changed" in failure for failure in result["failures"])


def test_object_lock_probe_transition_forces_questionnaire_review(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "evidence/owner-actions/B-046/object-lock-probe.json"
    mutate_line(path, '"classification": "INDETERMINATE"', '"classification": "BLOCKED"')
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("Object Lock probe result changed" in failure for failure in result["failures"])


def test_unconditional_501_claim_is_not_accepted(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / MODULE.CAIQ
    mutate_line(path, " if provider construction or CMK access fails;", ";")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("unconditional 501 claim" in failure for failure in result["failures"])


@pytest.mark.parametrize(
    ("document", "needle", "replacement"),
    [
        (MODULE.CAIQ, "No HSM protection for customer CMK material is evidenced", "CoreLink holds no customer key material at all"),
        (MODULE.CAIQ, "staff cannot access customer CMK material is **unverified**", "staff cannot access customer CMK material is vacuously true"),
        (MODULE.CAIQ, "The `/deactivate` Shred route is implemented", "Customer-controlled crypto-shredding is not available"),
        (MODULE.CAIQ, "served product's compute and storage are hosted by Cloudflare", "All compute / storage hosted by Cloudflare / AWS / GCP / Azure"),
        (MODULE.CAIQ, "shipped native container compiles the AWS BYOK path", "Plaintext DEKs never leave request scope (V8 isolate memory)"),
        (MODULE.CAIQ, "Its Dockerfile pins the runtime base image digest", "Workers run as V8 isolates, not containers"),
        (MODULE.SIG, "no verified customer rollout", "not shipped"),
        (MODULE.SIG, "served product's compute and storage are hosted by Cloudflare", "compute / storage hosted by Cloudflare / AWS / GCP / Azure"),
    ],
)
def test_absolute_customer_assurance_claims_fail_closed(
    tmp_path: Path, document: Path, needle: str, replacement: str
) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(root / document, needle, replacement)
    result = MODULE.verify(root)
    assert result["ok"] is False


def test_production_empty_cron_table_is_structural_and_fail_closed(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "wrangler.toml"
    mutate_line(path, "[env.prod.triggers]\ncrons = []", "# [env.prod.triggers]\n# crons = []")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("env.prod.triggers.crons" in failure for failure in result["failures"])


@pytest.mark.parametrize("workflow", [".github/workflows/semgrep.yml", ".github/workflows/fuzz-nightly.yml"])
def test_parked_workflow_cannot_reactivate_schedule(tmp_path: Path, workflow: str) -> None:
    root = fixture_tree(tmp_path)
    path = root / workflow
    mutate_line(path, "# schedule:", "schedule:")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("active schedule" in failure for failure in result["failures"])


def test_owner_packet_keeps_legal_and_external_actions_explicit() -> None:
    packet = (MODULE.ROOT / "docs/internal/b087-questionnaire-owner-actions.md").read_text()
    assert "DPA" in packet and "SLA" in packet
    assert "PagerDuty" in packet
    assert "no notification is claimed here" in packet


def test_substantiated_signed_commit_control_remains_positive(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(
        root / MODULE.CAIQ,
        "| CCC-07.1 | Source-code repositories access-controlled? | Y |",
        "| CCC-07.1 | Source-code repositories access-controlled? | N |",
    )
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("CCC-07.1" in failure and "expected Y" in failure for failure in result["failures"])
