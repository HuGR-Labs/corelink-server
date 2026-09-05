"""Load-bearing tests for the deterministic B-094 published-claims gate."""

from __future__ import annotations

import importlib.util
import json
import subprocess
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


def _rederive(root: Path, manifest: Path) -> None:
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")


def test_complete_inventory_passes(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    assert MODULE.validate(root, manifest) == []


def test_checked_in_published_corpus_passes_with_a_bounded_process():
    """Keep the production manifest gate load-bearing in the CI test glob."""
    result = subprocess.run(
        [sys.executable, str(SCRIPT_PATH)],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=45,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "B-094 OK" in result.stdout


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
        "Buck2 integration available on CoreLink.",
        "Connect Buck2 to CoreLink.",
        "Buck2 uses CoreLink as a remote cache.",
        "CoreLink is fully compatible with Buck2.",
        "CoreLink is compatible with Buck2.",
        "Buck2 connects to CoreLink.",
        "Buck2 supports CoreLink.",
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
        "Buck2 integration available on CoreLink.",
        "Connect Buck2 to CoreLink.",
        "Buck2 uses CoreLink as a remote cache.",
        "CoreLink is fully compatible with Buck2.",
        "CoreLink is compatible with Buck2.",
        "Buck2 connects to CoreLink.",
        "Buck2 supports CoreLink.",
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


def test_searchable_html_metadata_is_guarded(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "marketing/catalog.html", '<div data-status="shipped" data-text="Connect Buck2 to CoreLink"></div>\n')
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize("attribute", ["data-text", "data-claim", "aria-label"])
def test_hidden_and_data_attributes_are_guarded(tmp_path: Path, attribute: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "marketing/catalog.html", f'<div {attribute}="CoreLink is compatible with Buck2"></div>\n')
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize("locale", ["en-US", "de", "es-419", "pt-BR"])
def test_locale_rest_only_state_remains_neutral(tmp_path: Path, locale: str):
    root, manifest = _fixture(tmp_path)
    _write(
        root,
        f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/rest-only.mdx",
        "Buck2 and Pants are gRPC-only REAPI clients; CoreLink exposes HTTP/REST only and they cannot connect.\n",
    )
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert MODULE.validate(root, manifest) == []


@pytest.mark.parametrize(
    "claim",
    [
        "Roadmap: CoreLink support for Buck2 is planned.",
        "CoreLink will support Buck2 in a future release.",
        "CoreLink's planned integration with Pants is a roadmap item.",
    ],
)
def test_explicit_roadmap_state_is_not_current_support(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert MODULE.validate(root, manifest) == []


def test_unrelated_modal_does_not_exempt_a_present_claim(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "This may be relevant. CoreLink supports Buck2.\n")
    manifest.write_text(json.dumps(MODULE.build_manifest(root), indent=2), encoding="utf-8")
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink is fully compatible with Buck2.",
        "CoreLink is compatible with Buck2.",
        "Buck2 connects to CoreLink.",
        "Buck2 supports CoreLink.",
        '<div data-text="CoreLink is compatible with Buck2"></div>',
    ],
)
def test_required_semantic_patterns_are_load_bearing(tmp_path: Path, claim: str):
    """A re-derived inventory cannot launder any required claim wording."""
    root, manifest = _fixture(tmp_path)
    rel = "marketing/catalog.html" if claim.startswith("<") else "apps/docs/page.mdx"
    _write(root, rel, claim + "\n")
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


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink connects to Buck2.",
        "CoreLink connects with Pants.",
        "CoreLink supports only Buck2.",
        "CoreLink is only compatible with Pants.",
        "**CoreLink** **supports** **Buck2**.",
        "[CoreLink](https://corelink.example) supports [Buck2](https://buck2.build).",
        "CoreLink&nbsp;supports&nbsp;Buck2.",
        "CoreLink is compatible with\nBuck2.",
        '<div data-text="CoreLink is compatible with\nBuck2"></div>',
    ],
)
def test_production_path_rejects_normalized_claim_bypasses(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    _rederive(root, manifest)
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize(
    ("locale", "claim"),
    [
        ("de", "Buck2 und Pants sprechen ausschließlich gRPC; CoreLink stellt keinen gRPC-Eingang bereit und kann sich nicht verbinden."),
        ("es-419", "Buck2 y Pants solo hablan gRPC; CoreLink no expone entrada gRPC y no pueden conectarse."),
        ("pt-BR", "Buck2 e Pants falam apenas gRPC; o CoreLink não oferece entrada gRPC e eles não conseguem se conectar."),
        ("en-US", "Buck2 and Pants are gRPC-only clients; CoreLink exposes no gRPC ingress and they cannot connect."),
    ],
)
def test_multilingual_neutral_negative_copy_remains_allowed(tmp_path: Path, locale: str, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/status.mdx", claim + "\n")
    _rederive(root, manifest)
    assert MODULE.validate(root, manifest) == []


def test_rederived_entity_and_markup_claim_is_not_laundered(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "marketing/catalog.html", '<div data-claim="**CoreLink**&nbsp;supports&nbsp;Buck2"></div>\n')
    _rederive(root, manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


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


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink has Buck2 support.",
        "CoreLink integration with Buck2.",
        "CoreLink compatibility with Buck2.",
        "Buck2 integration with CoreLink.",
        "Buck2 compatibility with CoreLink.",
        "Use CoreLink with Buck2.",
        "Connect CoreLink to Buck2.",
        "Configure CoreLink for Buck2.",
        "Point CoreLink at Buck2.",
        "Pair CoreLink with Buck2.",
        "CoreLink is first-class with Buck2.",
    ],
)
def test_common_positive_formulations_remain_load_bearing_after_rederivation(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    _rederive(root, manifest)
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink supports not only Bazel but Buck2.",
        "CoreLink is not only compatible with Buck2.",
    ],
)
def test_not_only_is_not_treated_as_negation(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    _rederive(root, manifest)
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink supports\nthe\nBuck2.",
        "CoreLink is compatible with\nthe\nBuck2.",
        "Buck2 connects to\nthe\nCoreLink.",
        '<div data-text="CoreLink supports\nthe\nBuck2"></div>',
        '<div title="Buck2 connects to\nthe\nCoreLink"></div>',
    ],
)
def test_term_free_soft_wrap_connectors_do_not_hide_claims(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    rel = "marketing/catalog.html" if claim.startswith("<") else "apps/docs/page.mdx"
    _write(root, rel, claim + "\n")
    _rederive(root, manifest)
    failures = MODULE.validate(root, manifest)
    assert any("positive support claim" in failure for failure in failures)


def test_blank_or_unrelated_soft_wraps_do_not_combine_distant_prose(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "CoreLink supports\nunrelated prose\nBuck2 is a separate build tool.\n")
    _rederive(root, manifest)
    assert MODULE.validate(root, manifest) == []

    _write(root, "apps/docs/page.mdx", "CoreLink supports\nunrelated\nBuck2 is a separate build tool.\n")
    _rederive(root, manifest)
    assert MODULE.validate(root, manifest) == []

    _write(root, "apps/docs/page.mdx", "CoreLink supports\n\nBuck2 is a separate build tool.\n")
    _rederive(root, manifest)
    assert MODULE.validate(root, manifest) == []


@pytest.mark.parametrize(
    ("locale", "positive", "negative"),
    [
        ("de", "CoreLink ist mit Buck2 kompatibel.", "CoreLink ist nicht mit Buck2 kompatibel."),
        ("es-419", "CoreLink es compatible con Buck2.", "CoreLink no es compatible con Buck2."),
        ("pt-BR", "CoreLink é compatível com Buck2.", "CoreLink não é compatível com Buck2."),
    ],
)
def test_supported_locale_positive_claims_are_guarded_without_blocking_negation(
    tmp_path: Path, locale: str, positive: str, negative: str
):
    positive_root, positive_manifest = _fixture(tmp_path / "positive")
    positive_path = f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/status.mdx"
    _write(positive_root, positive_path, positive + "\n")
    _rederive(positive_root, positive_manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(positive_root, positive_manifest))

    negative_root, negative_manifest = _fixture(tmp_path / "negative")
    negative_path = f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/status.mdx"
    _write(negative_root, negative_path, negative + "\n")
    _rederive(negative_root, negative_manifest)
    assert MODULE.validate(negative_root, negative_manifest) == []


@pytest.mark.parametrize(
    ("locale", "claim"),
    [
        ("de", "CoreLink wird Buck2 unterstützen."),
        ("es-419", "CoreLink planea admitir Buck2."),
        ("pt-BR", "O suporte a Buck2 está planejado para o futuro."),
    ],
)
def test_supported_locale_roadmap_copy_remains_allowed(tmp_path: Path, locale: str, claim: str):
    root, manifest = _fixture(tmp_path)
    path = f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/roadmap.mdx"
    _write(root, path, claim + "\n")
    _rederive(root, manifest)
    assert MODULE.validate(root, manifest) == []


def test_modal_after_noun_phrase_is_local_roadmap_but_later_clause_is_not(tmp_path: Path):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", "CoreLink support for Buck2 is a future roadmap item.\n")
    _rederive(root, manifest)
    assert MODULE.validate(root, manifest) == []

    _write(root, "apps/docs/page.mdx", "CoreLink supports Buck2; the roadmap is documented separately.\n")
    _rederive(root, manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink can be used with Buck2.",
        "CoreLink interoperates with Buck2.",
        "CoreLink and Buck2 work together.",
        "CoreLink includes a Buck2 connector.",
        "CoreLink exposes a Buck2 integration.",
        "Buck2 support is provided by CoreLink.",
    ],
)
def test_relationship_grammar_blocks_common_bidirectional_claims(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    _write(root, "apps/docs/page.mdx", claim + "\n")
    _rederive(root, manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


@pytest.mark.parametrize(
    "claim",
    [
        "CoreLink supports\nboth\nBuck2.",
        "CoreLink is compatible with\nall\nBuck2.",
        '<div data-text="CoreLink supports\nboth\nBuck2"></div>',
    ],
)
def test_short_soft_wrap_connectors_are_not_an_allowlist_bypass(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    rel = "marketing/catalog.html" if claim.startswith("<") else "apps/docs/page.mdx"
    _write(root, rel, claim + "\n")
    _rederive(root, manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))


@pytest.mark.parametrize(
    ("locale", "positive", "negative", "roadmap"),
    [
        ("de", "CoreLink funktioniert mit Buck2.", "CoreLink funktioniert nicht mit Buck2.", "CoreLink wird Buck2 unterstützen."),
        ("es-419", "CoreLink ofrece soporte para Buck2.", "CoreLink no ofrece soporte para Buck2.", "CoreLink soportará Buck2 en el futuro."),
        ("pt-BR", "O CoreLink oferece suporte a Buck2.", "O CoreLink não oferece suporte a Buck2.", "O CoreLink suportará Buck2 no futuro."),
    ],
)
def test_expanded_locale_relationships_preserve_negative_and_roadmap_states(
    tmp_path: Path, locale: str, positive: str, negative: str, roadmap: str
):
    path = f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/status.mdx"
    positive_root, positive_manifest = _fixture(tmp_path / "positive")
    _write(positive_root, path, positive + "\n")
    _rederive(positive_root, positive_manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(positive_root, positive_manifest))

    negative_root, negative_manifest = _fixture(tmp_path / "negative")
    _write(negative_root, path, negative + "\n")
    _rederive(negative_root, negative_manifest)
    assert MODULE.validate(negative_root, negative_manifest) == []

    roadmap_root, roadmap_manifest = _fixture(tmp_path / "roadmap")
    _write(roadmap_root, path, roadmap + "\n")
    _rederive(roadmap_root, roadmap_manifest)
    assert MODULE.validate(roadmap_root, roadmap_manifest) == []


@pytest.mark.parametrize(
    "claim",
    [
        '<div data-text="CoreLink supports &#66;uck2"></div>',
        '<div aria-label="CoreLink &quot;supports&quot; Buck2"></div>',
        "CoreLink supports &#x42;uck2.",
    ],
)
def test_named_and_numeric_entities_are_decoded_before_visible_claim_scan(tmp_path: Path, claim: str):
    root, manifest = _fixture(tmp_path)
    rel = "marketing/catalog.html" if claim.startswith("<") else "apps/docs/page.mdx"
    _write(root, rel, claim + "\n")
    _rederive(root, manifest)
    assert any("positive support claim" in failure for failure in MODULE.validate(root, manifest))
