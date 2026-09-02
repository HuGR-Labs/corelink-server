"""Load-bearing tests for the deterministic B-094 published-claims gate."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT_PATH = REPO_ROOT / "scripts" / "verify_b094_published_claims.py"
SPEC = importlib.util.spec_from_file_location("verify_b094_published_claims", SCRIPT_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def _write(root: Path, rel: str, text: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _fixture(tmp_path: Path) -> tuple[Path, Path]:
    _write(tmp_path, "README.md", "CoreLink publishes an HTTP/REST cache.\n")
    _write(tmp_path, "apps/docs/page.mdx", "Context retained.\nBuck2 is not supported because CoreLink serves no gRPC ingress.\n")
    _write(
        tmp_path,
        "apps/docs/docusaurus.config.ts",
        'metadata: "CoreLink documentation metadata"\n',
    )
    _write(tmp_path, "crates/corelink-container/src/routes/bazel_v2.rs", "// Buck2 cannot use these routes\n")
    manifest_path = tmp_path / "inventory.json"
    manifest_path.write_text(json.dumps(MODULE.build_manifest(tmp_path), indent=2), encoding="utf-8")
    return tmp_path, manifest_path


def test_complete_inventory_passes(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    assert MODULE.validate(root, manifest) == []


@pytest.mark.parametrize(
    "claim",
    [
        "Buck2 works with CoreLink.",
        "CoreLink accepts Buck2 builds.",
        "Pants fully supported by CoreLink.",
        "Buck2 can use CoreLink as a remote cache.",
        "CoreLink offers native Buck2 integration.",
        "CoreLink provides Buck2 compatibility.",
        "Buck2 is a first-class CoreLink integration.",
        "Use Buck2 with CoreLink as your remote cache.",
        "Use CoreLink as a remote cache for Buck2.",
        "CoreLink has native Buck2 support.",
    ],
)
def test_positive_support_claim_is_rejected_even_when_new(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)
    assert any("new or changed occurrence" in failure for failure in failures)


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink offers native Buck2 integration.",
        "CoreLink provides Buck2 compatibility.",
        "Buck2 is a first-class CoreLink integration.",
        "Use Buck2 with CoreLink as your remote cache.",
        "Use CoreLink as a remote cache for Buck2.",
        "CoreLink has native Buck2 support.",
    ],
)
def test_rederived_manifest_still_rejects_positive_claim_variants(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    # The manifest is deliberately re-derived here: the semantic guard, not
    # merely the unknown-occurrence/hash guard, must reject a reviewed lie.
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


@pytest.mark.parametrize(
    "claim",
    [
        "BuildBuddy supports Buck2 and Pants.",
        "Compared with CoreLink, BuildBuddy supports Buck2.",
        "CoreLink competitor BuildBuddy supports Buck2.",
    ],
)
def test_competitor_support_claim_is_context_not_a_corelink_claim(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    # A reviewer has deliberately re-derived the changed population; this cell
    # isolates the semantic classification from the separate hash guard.
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert MODULE.validate(root, manifest) == []


@pytest.mark.parametrize(
    "claim",
    [
        "BuildBuddy comparison: CoreLink supports Buck2.",
        "Protocol note: CoreLink supports Buck2.",
        "Historical note: CoreLink supports Buck2.",
        "A competitor does not support Buck2; CoreLink supports Pants.",
    ],
)
def test_ambient_context_does_not_exempt_corelink_claim(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize(
    "claim",
    [
        "Competitor FAQ: our platform supports Buck2.",
        "BuildBuddy comparison: The platform supports Buck2.",
        "Protocol note: the service supports Buck2.",
        "Historical note: it supports Buck2.",
        'Quoted competitor claim: "it supports Buck2."',
    ],
)
def test_unknown_context_subjects_fail_closed(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize(
    "claim",
    [
        'e.g., "CoreLink supports Buck2."',
        "Example: CoreLink supports Buck2.",
    ],
)
def test_example_marker_does_not_exempt_a_positive_support_claim(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


def test_exact_reviewed_instructional_counterexample_remains_valid(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    path, text = next(iter(MODULE.INSTRUCTIONAL_COUNTEREXAMPLES))
    _write(root, path, "Never send an unverified product claim.\n" + text + "\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert MODULE.validate(root, manifest) == []


def test_example_with_an_explicit_negative_remains_valid(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "Example: Buck2 is not supported by CoreLink.\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert MODULE.validate(root, manifest) == []


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink does not support Buck2; CoreLink supports Pants.",
        "CoreLink does not expose a gRPC endpoint, but CoreLink supports Buck2.",
        "CoreLink does not support Buck2 and CoreLink supports Pants.",
        "CoreLink does not support Buck2 — CoreLink supports Pants.",
        "CoreLink does not expose a gRPC endpoint – CoreLink supports Buck2.",
        "CoreLink does not support Buck2 / CoreLink supports Pants.",
        "CoreLink does not support Buck2 (but CoreLink supports Pants).",
    ],
)
def test_clause_local_negation_does_not_exempt_following_positive_claim(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    # Re-derive so the test exercises the semantic guard rather than the
    # inventory's changed-occurrence halt.
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


def test_unknown_neutral_rewording_requires_reclassification(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "Buck2 uses a gRPC-only client path.\n")
    failures = MODULE.validate(root, manifest)
    assert any("new or changed occurrence" in failure for failure in failures)


def test_line_movement_does_not_change_occurrence_identity(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "Buck2 is not supported because CoreLink serves no gRPC ingress.\nContext retained.\n")
    assert MODULE.validate(root, manifest) == []


def test_nonterm_context_truncation_halts(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    # Keep the inventoried occurrence intact while deleting unrelated context;
    # the file-content population must still stop the gate.
    _write(root, "apps/docs/page.mdx", "Buck2 is not supported because CoreLink serves no gRPC ingress.\n")
    failures = MODULE.validate(root, manifest)
    assert any("file content/population" in failure for failure in failures)


def test_published_docusaurus_metadata_is_inventoried_and_guarded(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    assert "apps/docs/docusaurus.config.ts" in MODULE.build_manifest(root)["population"]["files"]
    _write(root, "apps/docs/docusaurus.config.ts", 'metadata: "CoreLink offers Buck2 integration"\n')
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


def test_negative_docusaurus_metadata_remains_valid_after_rederivation(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/docusaurus.config.ts", 'metadata: "Buck2 is not supported by CoreLink"\n')
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert MODULE.validate(root, manifest) == []


def test_manifest_generation_is_reproducible(tmp_path: Path):
    root, _manifest = _fixture(tmp_path)
    assert MODULE.build_manifest(root) == MODULE.build_manifest(root)


def test_empty_or_truncated_population_halts(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "CoreLink docs remain published.\n")
    failures = MODULE.validate(root, manifest)
    assert any("empty or truncated" in failure or "disappeared" in failure for failure in failures)


def test_anchor_removal_halts_independently(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "crates/corelink-container/src/routes/bazel_v2.rs", "// route implementation changed\n")
    failures = MODULE.validate(root, manifest)
    assert any("product anchor" in failure for failure in failures)
