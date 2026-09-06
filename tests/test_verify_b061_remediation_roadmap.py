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


def test_rendered_okf_surface_staleness_is_fail_closed(tmp_path: Path) -> None:
    site = tmp_path / verifier.OKF_RENDERED_SITE
    site.parent.mkdir(parents=True, exist_ok=True)
    site.write_text((ROOT / verifier.OKF_RENDERED_SITE).read_text(encoding="utf-8") + "\n", encoding="utf-8")

    with pytest.raises(verifier.RoadmapVerificationError, match="rendered site is stale"):
        verifier._validate_okf_surfaces(ROOT, site_path=site)


def test_manifest_population_mismatch_is_fail_closed(tmp_path: Path) -> None:
    manifest = tmp_path / verifier.OKF_MANIFEST
    manifest.parent.mkdir(parents=True, exist_ok=True)
    text = (ROOT / verifier.OKF_MANIFEST).read_text(encoding="utf-8")
    manifest.write_text(text.replace('id: "planes/container"', 'id: "planes/phantom"', 1), encoding="utf-8")

    with pytest.raises(verifier.RoadmapVerificationError, match="exact generated index"):
        verifier._validate_okf_surfaces(ROOT, manifest_path=manifest)


def test_tier_extractors_ignore_comment_and_string_bait() -> None:
    tier_source = '''
        /* pub enum TierKind { Fake, AlsoFake } */
        const BAIT: &str = "pub enum TierKind { StringFake }";
        const RAW_BAIT: &str = r#"pub enum TierKind { RawFake }"#;
        pub enum TierKind { RealOne, /* Fake */ RealTwo }
    '''
    ladder_source = '''
        // pub const TIER_RATE_LADDER: [(Tier, u32, u32); 99] = [];
        const BAIT: &str = "TIER_RATE_LADDER [(Tier, u32, u32); 98]";
        const RAW_BAIT: &str = r#"pub const TIER_RATE_LADDER: [(Tier, u32, u32); 97] = [];"#;
        pub const TIER_RATE_LADDER: [(Tier, u32, u32); 2] = [
            (Tier::Free, 1, 1), (Tier::Solo, 2, 2),
        ];
    '''
    assert verifier._tier_kind_variants(tier_source) == ["RealOne", "RealTwo"]
    assert verifier._rate_ladder_length(ladder_source) == 2


def test_split_validator_c5_comment_bait_is_not_an_ast_emission(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    runtime = tmp_path / "validate_okf_runtime.py"
    source = (ROOT / verifier.OKF_RUNTIME).read_text(encoding="utf-8")
    source = source.replace('                        "C5",\n', '                        # fails.add("C5", "comment-bait", "not executable")\n')
    runtime.write_text(source, encoding="utf-8")
    monkeypatch.setattr(verifier, "OKF_RUNTIME", runtime)
    monkeypatch.setattr(verifier, "OKF_SPLIT_MODULES", (runtime, *verifier.OKF_SPLIT_MODULES[1:]))

    with pytest.raises(verifier.RoadmapVerificationError, match="fail-closed checkpoint/content"):
        verifier._validate_okf_surfaces(ROOT)


def test_missing_split_validator_module_is_fail_closed(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    missing = tmp_path / "validate_okf_core2.py"
    monkeypatch.setattr(verifier, "OKF_SPLIT_MODULES", (verifier.OKF_RUNTIME, verifier.OKF_SPLIT_MODULES[1], missing, *verifier.OKF_SPLIT_MODULES[3:]))

    with pytest.raises(verifier.RoadmapVerificationError, match="split validator module"):
        verifier._validate_okf_surfaces(ROOT)


def test_split_paths_must_be_once_per_event_in_both_workflows(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    mutated_backlog = tmp_path / "backlog-verify.yml"
    backlog = (ROOT / verifier.WORKFLOW).read_text(encoding="utf-8")
    mutated_backlog.write_text(
        backlog.replace(
            '      - "scripts/validate_okf_core1.py"\n',
            '      # - "scripts/validate_okf_core1.py"\n'
            '      - "scripts/validate_okf_core1.py # string-bait"\n',
            1,
        ),
        encoding="utf-8",
    )
    with pytest.raises(verifier.RoadmapVerificationError, match="pull_request"):
        verifier._validate_projections(ROOT, workflow_path=mutated_backlog)

    mutated_okf = tmp_path / "okf_wiki.yml"
    okf = (ROOT / verifier.OKF_WORKFLOW).read_text(encoding="utf-8")
    mutated_okf.write_text(
        okf.replace(
            "      - 'scripts/validate_okf_core1.py'\n",
            "      # - 'scripts/validate_okf_core1.py'\n"
            "      - 'scripts/validate_okf_core1.py string-bait'\n",
            1,
        ),
        encoding="utf-8",
    )
    monkeypatch.setattr(verifier, "OKF_WORKFLOW", mutated_okf)
    with pytest.raises(verifier.RoadmapVerificationError, match="OKF workflow.*pull_request"):
        verifier._validate_projections(ROOT)


def test_b061_installs_python_requirements_before_pytest() -> None:
    workflow = (ROOT / verifier.WORKFLOW).read_text(encoding="utf-8")
    assert workflow.index("Prepare Python verifier dependencies") < workflow.index(
        "B-061 remediation roadmap verifier and mutations"
    )
