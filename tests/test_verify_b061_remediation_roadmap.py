"""Adversarial contract tests for B-061's roadmap verifier."""

from __future__ import annotations

import json
import hashlib
from pathlib import Path
import sys

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b061_remediation_roadmap as verifier  # noqa: E402


def _projection_bundle(tmp_path: Path) -> dict[str, Path]:
    paths = {
        "roadmap_path": verifier.ROADMAP,
        "metrics_path": verifier.METRICS,
        "backlog_path": verifier.BACKLOG,
        "changelog_path": verifier.CHANGELOG,
        "workflow_path": verifier.WORKFLOW,
    }
    copies: dict[str, Path] = {}
    for key, relative in paths.items():
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text((ROOT / relative).read_text(encoding="utf-8"), encoding="utf-8")
        copies[key] = target
    return copies


def test_canonical_projection_matches_derived_sources() -> None:
    report = verifier.verify(ROOT)

    assert report["status"] == "b061_remediation_roadmap_verified"
    assert report["okf_documents"] == 170
    assert report["okf_checkpoint_claims"] == 169
    assert report["okf_orphan_anchors"] == 0
    assert report["tier_kind_variants"] == 11
    assert report["rate_ladder_variants"] == 5


def test_mutated_canonical_okf_count_is_red(tmp_path: Path) -> None:
    paths = _projection_bundle(tmp_path)
    text = paths["roadmap_path"].read_text(encoding="utf-8")
    paths["roadmap_path"].write_text(
        text.replace("okf_orphan_anchors=0", "okf_orphan_anchors=1"), encoding="utf-8"
    )

    with pytest.raises(verifier.RoadmapVerificationError, match="canonical metrics drifted"):
        verifier.verify(ROOT, **paths)


def test_mutated_sidecar_tier_count_is_red(tmp_path: Path) -> None:
    paths = _projection_bundle(tmp_path)
    sidecar = json.loads(paths["metrics_path"].read_text(encoding="utf-8"))
    sidecar["canonical_metrics"]["tier_kind_variants"] = 6
    paths["metrics_path"].write_text(json.dumps(sidecar), encoding="utf-8")

    with pytest.raises(verifier.RoadmapVerificationError, match="sidecar"):
        verifier.verify(ROOT, **paths)


def test_workflow_path_mutation_is_red(tmp_path: Path) -> None:
    paths = _projection_bundle(tmp_path)
    workflow = paths["workflow_path"].read_text(encoding="utf-8")
    paths["workflow_path"].write_text(
        workflow.replace('      - "docs/knowledge/**"\n', ""), encoding="utf-8"
    )

    with pytest.raises(verifier.RoadmapVerificationError, match="workflow paths"):
        verifier.verify(ROOT, **paths)


def test_changelog_projection_mutation_is_red(tmp_path: Path) -> None:
    paths = _projection_bundle(tmp_path)
    paths["changelog_path"].write_text("### Fixed\n- unrelated\n", encoding="utf-8")

    with pytest.raises(verifier.RoadmapVerificationError, match="changelog"):
        verifier.verify(ROOT, **paths)


def test_actions_inventory_mutation_is_red_without_hardcoded_counts(tmp_path: Path) -> None:
    paths = _projection_bundle(tmp_path)
    export = tmp_path / "b061-actions-census-mutated.json"
    evidence = json.loads((ROOT / verifier.ACTIONS_EXPORT).read_text(encoding="utf-8"))
    evidence["lanes"].pop()
    export.write_text(json.dumps(evidence), encoding="utf-8")
    sidecar = json.loads(paths["metrics_path"].read_text(encoding="utf-8"))
    sidecar["actions_evidence"]["sha256"] = hashlib.sha256(export.read_bytes()).hexdigest()
    paths["metrics_path"].write_text(json.dumps(sidecar), encoding="utf-8")
    paths["actions_export_path"] = export

    with pytest.raises(verifier.RoadmapVerificationError, match="metrics drifted"):
        verifier.verify(ROOT, **paths)


def test_actions_export_digest_is_fail_closed(tmp_path: Path) -> None:
    paths = _projection_bundle(tmp_path)
    export = tmp_path / "b061-actions-census-mutated.json"
    raw = (ROOT / verifier.ACTIONS_EXPORT).read_bytes() + b"\n"
    export.write_bytes(raw)
    paths["actions_export_path"] = export

    with pytest.raises(verifier.RoadmapVerificationError, match="digest"):
        verifier.verify(ROOT, **paths)


def test_b061_installs_python_requirements_before_pytest() -> None:
    workflow = (ROOT / verifier.WORKFLOW).read_text(encoding="utf-8")
    assert workflow.index("Prepare Python verifier dependencies") < workflow.index(
        "B-061 remediation roadmap verifier and mutations"
    )
