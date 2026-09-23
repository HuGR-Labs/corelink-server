"""Regression tests for the B-101 finding-to-decision ingestion gate."""

from __future__ import annotations

import copy
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/audit-finding-coverage.yml"
BACKLOG = ROOT / "BACKLOG.md"
sys.path.insert(0, str(ROOT / "scripts"))
import verify_audit_finding_coverage as coverage  # noqa: E402


def _manifest() -> dict:
    return json.loads((ROOT / coverage.MANIFEST_RELATIVE).read_text(encoding="utf-8"))


def _write_manifest(tmp_path: Path, manifest: dict) -> Path:
    path = tmp_path / "decisions.json"
    path.write_text(json.dumps(manifest), encoding="utf-8")
    return path


def _copy_gate_root(tmp_path: Path) -> Path:
    """Build a complete isolated gate fixture without mutating the checkout."""
    relative_paths = [Path("BACKLOG.md"), coverage.MANIFEST_RELATIVE, coverage.SOURCE_REGISTRY_RELATIVE]
    for relative_path in relative_paths:
        destination = tmp_path / relative_path
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative_path, destination)
    for root in (Path("docs/security"), Path("reports/audits")):
        shutil.copytree(ROOT / root, tmp_path / root, dirs_exist_ok=True)
    return tmp_path


def test_live_inputs_rederive_the_documented_live_population() -> None:
    report = coverage.verify(ROOT)

    # B-101 admits the current B-028 Dependabot snapshot as a fourth source;
    # the gate must retain its per-alert identities in the closed population.
    assert report == {
        "documents": {
            "due_diligence_2026_06_15": 87,
            "go_live_2026_08_26": 20,
            "pilot_identity_2026_07_02": 3,
            "b028_dependabot_2026_09_06": 9,
        },
        "total": 119,
        "tracked": 42,
        "duplicate": 4,
        "proposed": 73,
        "status": "historical_coverage_complete_semantic_review_complete",
    }


def test_post_squash_plain_tree_rederives_the_same_certificate(tmp_path: Path) -> None:
    """The checkpoint is content-based, so a copied/squashed tree remains valid."""
    fixture_root = _copy_gate_root(tmp_path)

    assert coverage.verify(fixture_root)["total"] == 119


def test_b028_historical_source_rejects_a_zero_alert_snapshot() -> None:
    with pytest.raises(
        coverage.CoverageError,
        match="Dependabot snapshot has no alerts",
    ):
        coverage.parse_b028_dependabot(
            json.dumps({"schema_version": 1, "alerts": []}),
            "docs/security/b028-dependabot-census-2026-09-06.json",
        )


def test_b373_dependabot_census_is_pinned_as_a_b101_non_source(tmp_path: Path) -> None:
    registry_path = ROOT / coverage.SOURCE_REGISTRY_RELATIVE
    registry_bytes = registry_path.read_bytes()
    registry = json.loads(registry_bytes)
    snapshot_path = "docs/security/b373-dependabot-census-2026-09-09.json"
    snapshot_bytes = (ROOT / snapshot_path).read_bytes()
    excluded = next(item for item in registry["excluded_audits"] if item["path"] == snapshot_path)

    assert "B-373's structured Dependabot snapshot" in excluded["reason"]
    assert excluded["sha256"] == hashlib.sha256(snapshot_bytes).hexdigest()
    assert snapshot_path not in {source["path"] for source in registry["sources"]}
    assert _manifest()["source_registry_sha256"] == hashlib.sha256(registry_bytes).hexdigest()

    fixture_root = _copy_gate_root(tmp_path)
    changed_snapshot = fixture_root / snapshot_path
    changed_snapshot.write_bytes(snapshot_bytes + b"\n")
    with pytest.raises(
        coverage.CoverageError,
        match=rf"excluded audit content changed; reclassify it: {re.escape(snapshot_path)}",
    ):
        coverage.verify(fixture_root)


def test_tree_certificate_rejects_a_mutation_even_when_registry_and_manifest_are_untouched(
    tmp_path: Path,
) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    source = fixture_root / "docs/security/2026-06-15-launch-due-diligence-audit.md"
    source.write_text(source.read_text(encoding="utf-8") + "\n# mutation\n", encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="governed audit tree differs from the B-101 content certificate"):
        coverage.verify(fixture_root)


@pytest.mark.parametrize(
    "relative",
    [
        Path("reports/audits/2026-08-25-comprehensive-audit-and-verification.md"),
        Path("reports/audits/2026-08-25-definitive-master-report.md"),
        Path("docs/security/b028-dependabot-census-2026-09-06-postmerge.json"),
    ],
)
def test_excluded_audit_digest_rejects_substituted_bytes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, relative: Path
) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    audit = fixture_root / relative
    audit.write_bytes(audit.read_bytes() + b"\nsubstituted bytes\n")

    # Refresh only the synthetic fixture's tree certificate so this test reaches
    # the independent excluded-audit pin instead of failing the outer tree seal.
    paths: set[str] = set()
    for root in coverage.B101_CENSUS_ROOTS:
        paths |= coverage._regular_files_in_root(fixture_root, coverage.AuditRoot(root))
    census = sorted(
        (
            path,
            hashlib.sha256(
                coverage._read_regular_file_beneath(fixture_root, path, "test census entry")
            ).hexdigest(),
        )
        for path in paths
    )
    tree_digest = hashlib.sha256(json.dumps(census, separators=(",", ":")).encode()).hexdigest()
    monkeypatch.setattr(coverage, "B101_CENSUS_TREE_SHA256", tree_digest)

    with pytest.raises(coverage.CoverageError, match="excluded audit content changed; reclassify it"):
        coverage.verify(fixture_root)


def test_manifest_unknown_field_is_malformed_and_red(tmp_path: Path) -> None:
    manifest = _manifest()
    manifest["unexpected"] = "reanchor"

    with pytest.raises(coverage.CoverageError, match="decision manifest has ambiguous extra fields"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_missing_decision_is_red_not_a_document_name_false_green(tmp_path: Path) -> None:
    manifest = _manifest()
    manifest["decisions"] = manifest["decisions"][1:]

    with pytest.raises(coverage.CoverageError, match="new or unassigned findings: DD-001"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_duplicate_decision_is_ambiguous_and_red(tmp_path: Path) -> None:
    manifest = _manifest()
    manifest["decisions"].append(copy.deepcopy(manifest["decisions"][0]))

    with pytest.raises(coverage.CoverageError, match="duplicate decisions are ambiguous: DD-001"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_missing_canonical_backlog_id_is_red(tmp_path: Path) -> None:
    manifest = _manifest()
    decision = next(item for item in manifest["decisions"] if item["kind"] == "tracked")
    decision["backlog_id"] = "B-999"

    with pytest.raises(coverage.CoverageError, match="existing canonical B-ID"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_missing_proposal_backlog_item_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    path = fixture_root / "BACKLOG.md"
    text = path.read_text(encoding="utf-8")
    start = text.index("### B-171 — ")
    end = text.index("### B-172 — ", start)
    path.write_text(text[:start] + text[end:], encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="DD-002: proposal B-ID needs an existing canonical backlog item"):
        coverage.verify(fixture_root)


def test_malformed_backlog_id_is_red_before_existence_lookup(tmp_path: Path) -> None:
    manifest = _manifest()
    decision = next(item for item in manifest["decisions"] if item["kind"] == "tracked")
    decision["backlog_id"] = "B-093 "

    with pytest.raises(coverage.CoverageError, match="canonical B-ID format"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_stale_canonical_title_is_red(tmp_path: Path) -> None:
    manifest = _manifest()
    decision = next(item for item in manifest["decisions"] if item["source_id"] == "DD-023")
    decision["semantic_disposition"]["canonical_title"] = (
        "dois endpoints Stripe vivos processam o mesmo evento duas vezes, há mais de sete dias"
    )

    with pytest.raises(
        coverage.CoverageError,
        match="DD-023: tracked decision needs an exact canonical equivalence proof",
    ):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_parked_b216_requires_its_runtime_owner_packet() -> None:
    manifest = _manifest()
    decision = next(
        item for item in manifest["decisions"] if item["source_id"] == "DD-068"
    )
    evidence = decision["semantic_disposition"]["evidence"]
    contract = coverage._backlog_proposal_contracts(ROOT)["B-216"]

    coverage._require_proposal_contract("B-216", contract, evidence)

    without_packet = contract.replace("owner packet", "owner receipt", 1)
    with pytest.raises(
        coverage.CoverageError,
        match="B-216: parked proposal needs an owner packet or bounded local retirement verifier",
    ):
        coverage._require_proposal_contract("B-216", without_packet, evidence)


def test_semantic_evidence_mutation_is_red(tmp_path: Path) -> None:
    manifest = _manifest()
    decision = manifest["decisions"][0]
    decision["semantic_disposition"]["evidence"]["finding_title"] = "not the parsed finding"

    with pytest.raises(coverage.CoverageError, match="semantic evidence does not match"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_proposal_outside_the_certified_new_id_set_is_red(tmp_path: Path) -> None:
    manifest = _manifest()
    proposal = next(item for item in manifest["decisions"] if item["kind"] == "proposed")
    proposal["proposal_id"] = "B-999"
    proposal["semantic_disposition"]["proposal_id"] = "B-999"

    with pytest.raises(coverage.CoverageError, match="outside the certified B-101 proposal set"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_duplicate_target_mutation_is_red(tmp_path: Path) -> None:
    manifest = _manifest()
    duplicate = next(item for item in manifest["decisions"] if item["kind"] == "duplicate")
    duplicate["duplicate_of"] = "DD-999"
    duplicate["semantic_disposition"]["duplicate_of"] = "DD-999"

    with pytest.raises(coverage.CoverageError, match="duplicate target must be another tracked or proposed finding"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_bazel_body_limit_finding_tracks_its_canonical_b093_item() -> None:
    decision = next(item for item in _manifest()["decisions"] if item["source_id"] == "DD-037")

    assert decision["source_id"] == "DD-037"
    assert decision["kind"] == "tracked"
    assert decision["backlog_id"] == "B-093"
    assert decision["semantic_disposition"]["canonical_id"] == "B-093"


def test_all_findings_have_an_auditable_equivalence_duplicate_or_new_proposal() -> None:
    manifest = _manifest()
    assert all(decision["kind"] in {"tracked", "duplicate", "proposed"} for decision in manifest["decisions"])
    duplicates = [decision for decision in manifest["decisions"] if decision["kind"] == "duplicate"]
    assert len(duplicates) == 4
    assert {decision["source_id"]: decision["duplicate_of"] for decision in duplicates} == {
        "DD-041": "DD-036",
        "DD-057": "DD-008",
        "DD-078": "DD-056",
        "DD-087": "DD-021",
    }
    assert all(
        decision["semantic_disposition"]["relation"] == "exact_duplicate_finding"
        for decision in duplicates
    )
    proposals = [decision for decision in manifest["decisions"] if decision["kind"] == "proposed"]
    assert len(proposals) == 73
    assert {decision["proposal_id"] for decision in proposals} == {
        f"B-{number}" for number in range(171, 244)
    }
    assert all(
        decision["semantic_disposition"]["relation"] == "new_canonical_finding_proposal"
        for decision in proposals
    )


def test_truncated_or_renumbered_due_diligence_findings_are_red() -> None:
    source = (ROOT / "docs/security/2026-06-15-launch-due-diligence-audit.md").read_text(encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="contiguous DD-001"):
        coverage.parse_due_diligence(source.replace("### 1. [HIGH]", "### 99. [HIGH]", 1), "fixture.md")


def test_unadmitted_registry_control_file_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    fourth_source = fixture_root / coverage.SOURCE_DIRECTORY_RELATIVE / "fourth-audit.md"
    fourth_source.write_text("# A fourth audit that has not been admitted\n", encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="unadmitted audit source files: fourth-audit.md"):
        coverage.verify(fixture_root)


def test_unadmitted_security_audit_file_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    audit = fixture_root / "docs/security/2026-09-02-new-audit.md"
    audit.write_text("# Newly discovered security audit\n", encoding="utf-8")

    with pytest.raises(
        coverage.CoverageError,
        match="unadmitted audit source files: docs/security/2026-09-02-new-audit.md",
    ):
        coverage.verify(fixture_root)


def test_case_variant_audit_filename_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    audit = fixture_root / "docs/security/2026-09-02-new-AUDIT.md"
    audit.write_text("# Case-variant security audit\n", encoding="utf-8")

    with pytest.raises(
        coverage.CoverageError,
        match="unadmitted audit source files: docs/security/2026-09-02-new-AUDIT.md",
    ):
        coverage.verify(fixture_root)


def test_nested_audit_file_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    audit = fixture_root / "docs/security/nested/2026-09-02-new-audit.md"
    audit.parent.mkdir()
    audit.write_text("# Nested security audit\n", encoding="utf-8")

    with pytest.raises(
        coverage.CoverageError,
        match="unadmitted audit source files: docs/security/nested/2026-09-02-new-audit.md",
    ):
        coverage.verify(fixture_root)


def test_symlinked_governed_subdirectory_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    linked_directory = fixture_root / "outside-audits"
    linked_directory.mkdir()
    (linked_directory / "2026-09-02-new-audit.md").write_text(
        "# Linked security audit\n", encoding="utf-8"
    )
    os.symlink(linked_directory, fixture_root / "docs/security/linked-audits")

    with pytest.raises(
        coverage.CoverageError,
        match="symlinked entry in governed audit root: docs/security/linked-audits",
    ):
        coverage.verify(fixture_root)


def test_admitted_audit_source_symlink_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    relative_source = coverage.source_specs(ROOT)[0][0].path
    source = fixture_root / relative_source
    replacement = fixture_root / "outside-admitted-source.md"
    replacement.write_bytes(source.read_bytes())
    source.unlink()
    os.symlink(replacement, source)

    with pytest.raises(
        coverage.CoverageError,
        match=rf"symlinked entry in governed audit root: {re.escape(relative_source)}",
    ):
        coverage.verify(fixture_root)


def test_unadmitted_reports_audit_file_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    audit = fixture_root / "reports/audits/2026-09-02-new-audit.md"
    audit.write_text("# Newly discovered readiness audit\n", encoding="utf-8")

    with pytest.raises(
        coverage.CoverageError,
        match="unadmitted audit source files: reports/audits/2026-09-02-new-audit.md",
    ):
        coverage.verify(fixture_root)


def test_unrelated_documents_under_governed_roots_do_not_fail_coverage(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    for document in (
        fixture_root / "docs/security/2026-09-02-release-notes.md",
        fixture_root / "reports/audits/2026-09-02-release-notes.md",
    ):
        document.write_text("# Unrelated documentation\n", encoding="utf-8")

    with pytest.raises(
        coverage.CoverageError,
        match="unadmitted audit source files: docs/security/2026-09-02-release-notes.md, reports/audits/2026-09-02-release-notes.md",
    ):
        coverage.verify(fixture_root)


def test_source_digest_drift_is_red_even_when_its_findings_still_parse(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    source = fixture_root / coverage.source_specs(ROOT)[0][0].path
    source.write_text(source.read_text(encoding="utf-8") + "\n", encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="governed audit tree differs from the B-101 content certificate"):
        coverage.verify(fixture_root)


def test_source_registry_digest_drift_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    registry = fixture_root / coverage.SOURCE_REGISTRY_RELATIVE
    registry.write_text(registry.read_text(encoding="utf-8") + "\n", encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="governed source registry differs from the B-101 content certificate"):
        coverage.verify(fixture_root)


def test_b101_contract_records_completed_semantic_review_stage() -> None:
    backlog = BACKLOG.read_text(encoding="utf-8")
    block = re.search(r"(?ms)^### B-101\b.*?(?=^### B-\d+\b|\Z)", backlog)

    assert block, "B-101 must remain a canonical backlog item"
    assert "87 + 20 + 3 + 9 = 119" in block.group(0)
    assert "status: done" in block.group(0)
    assert f"stage: {coverage.B101_STAGE}" in block.group(0)
    assert "verify: python3 scripts/verify_audit_finding_coverage.py --format json" in block.group(0)


def test_premature_b101_status_reopen_is_red(tmp_path: Path) -> None:
    fixture_root = _copy_gate_root(tmp_path)
    backlog = fixture_root / "BACKLOG.md"
    text = backlog.read_text(encoding="utf-8")
    changed, replacements = re.subn(
        r"(?ms)(^### B-101\b.*?^status:) done$", r"\1 open", text, count=1
    )
    assert replacements == 1
    backlog.write_text(changed, encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="B-101 must retain status: done"):
        coverage.verify(fixture_root)


def test_backlog_changes_trigger_the_coverage_gate_on_pull_request_and_push() -> None:
    """Keep the gate load-bearing when a backlog decision is changed."""
    workflow = WORKFLOW.read_text(encoding="utf-8")
    for event in ("pull_request", "push"):
        event_block = re.search(
            rf"(?ms)^  {event}:\n(.*?)(?=^  [A-Za-z_]+:|\Z)",
            workflow,
        )
        assert event_block, f"audit workflow must define the {event} trigger"
        paths_block = re.search(
            r"(?ms)^    paths:\n(.*?)(?=^    [A-Za-z_]+:|\Z)",
            event_block.group(1),
        )
        assert paths_block, f"{event} trigger must declare path filters"
        for path in (
            "BACKLOG.md",
            "docs/security/**",
            "reports/audits/**",
            "reports/audit-finding-decisions/**",
            "scripts/verify_b101_open_state.py",
            "scripts/verify_b101_proposals.py",
            "tests/test_b101_proposal_contracts.py",
        ):
            assert re.search(
                rf"^      - ['\"]?{re.escape(path)}['\"]?\s*$", paths_block.group(1), re.MULTILINE
            ), f"{path} must trigger the {event} coverage gate"


def test_workflow_runs_the_gate_and_its_mutation_suite() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    requirements = (ROOT / "requirements-ci.txt").read_text(encoding="utf-8")

    assert ".venv/bin/python3 scripts/verify_audit_finding_coverage.py --format json" in workflow
    assert ".venv/bin/python3 scripts/verify_b101_proposals.py" in workflow
    assert "run: python3 scripts/verify_audit_finding_coverage.py --format json" not in workflow
    assert ".venv/bin/python3 -m pytest tests/test_b101_proposal_contracts.py -q" in workflow
    assert ".venv/bin/python3 -m pytest tests/test_audit_finding_coverage.py -q" in workflow
    assert "pyyaml" in requirements.lower()
    assert workflow.index(".venv/bin/python3 -m pip install --quiet -r requirements-ci.txt") < workflow.index(
        ".venv/bin/python3 scripts/verify_audit_finding_coverage.py --format json"
    )


def test_unprovisioned_python_fails_closed_instead_of_faking_a_green_verifier() -> None:
    result = subprocess.run(
        [sys.executable, "-S", "scripts/verify_audit_finding_coverage.py", "--format", "json"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode != 0
    assert "yaml" in result.stderr.lower()
