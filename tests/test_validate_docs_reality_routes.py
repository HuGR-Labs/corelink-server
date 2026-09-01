"""Focused route-model regressions for the Docs Reality endpoint gate."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("validate_docs_reality_routes", ROOT /
                                               "scripts/validate_docs_reality.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def inventory(*routes: str):
    return MODULE.RouteInventory.from_routes(set(routes))


def test_exact_route_does_not_act_as_a_prefix():
    routes = inventory("/v1", "/v1/users/me")
    assert routes.resolves("/v1")
    assert routes.resolves("/v1/users/me")
    assert not routes.resolves("/v1/zzz-nonexistent")


def test_legacy_resolver_seam_does_not_reintroduce_prefix_matching():
    regexes = [MODULE._route_to_regex("/v1")]
    assert not MODULE.endpoint_resolves("/v1/zzz-nonexistent", regexes,
                                        {"/v1"})


def test_parameterized_route_matches_one_segment_only():
    routes = inventory("/v1/users/{user_id}", "/v1/items/:item_id")
    assert routes.resolves("/v1/users/alice")
    assert routes.resolves("/v1/items/widget")
    assert not routes.resolves("/v1/users/alice/profile")
    assert not routes.resolves("/v1/users/")


@pytest.mark.parametrize("route", ["/v1/files/{*rest}", "/v1/files/*rest"])
def test_explicit_catch_all_matches_deeper_paths(route: str):
    routes = inventory(route)
    assert routes.resolves("/v1/files/a/b/c")
    assert routes.resolves("/v1/files")
    assert not routes.resolves("/v1/file/a")


def test_regex_metacharacters_are_literal_not_suffix_patterns():
    routes = inventory("/v1/release.v1", "/v1/thing+")
    assert routes.resolves("/v1/release.v1")
    assert routes.resolves("/v1/thing+")
    assert not routes.resolves("/v1/releaseXv1")
    assert not routes.resolves("/v1/thingthing")


def test_comments_do_not_create_wired_routes(tmp_path: Path, monkeypatch):
    source = tmp_path / "routes.rs"
    source.write_text(
        '// .route("/v1/comment-only", get(handler))\n'
        '/* .route("/v1/block-comment-only", get(handler)) */\n'
        '.route("/v1/real", get(handler))\n', encoding="utf-8"
    )
    monkeypatch.setattr(MODULE, "ROUTE_SOURCE_ROOTS", [tmp_path])
    routes = MODULE.collect_routes()
    assert "/v1/real" in routes
    assert "/v1/comment-only" not in routes
    assert "/v1/block-comment-only" not in routes


def test_prose_does_not_create_documented_endpoint():
    doc = MODULE.DocFile(Path("fixture.md"), "fixture.md",
                         "The prose mentions /v1/ghost but does not execute it.\n")
    assert MODULE.extract_doc_endpoints(doc) == []


def test_commented_fenced_example_does_not_create_documented_endpoint():
    doc = MODULE.DocFile(Path("fixture.md"), "fixture.md",
                         "```bash\n# curl https://host/v1/ghost\n```\n")
    assert MODULE.extract_doc_endpoints(doc) == []


def test_empty_and_truncated_route_populations_fail_closed():
    assert MODULE.validate_route_population(inventory())
    assert MODULE.validate_route_population(inventory("/v1"), minimum=2)
    assert MODULE.validate_route_population(inventory("/v1"), minimum=1) == []


def test_empty_or_malformed_allowlist_fails_closed():
    assert MODULE.validate_allowlist({})
    errors = MODULE.validate_allowlist({"endpoint": {"flagship_files": []}})
    assert any("flagship_files" in error for error in errors)
    errors = MODULE.validate_allowlist({"endpoint": {"flagship_files": [""]}})
    assert any("non-empty strings" in error for error in errors)
    errors = MODULE.validate_allowlist({"endpoint": {"flagship_files": [{}]}})
    assert any("non-empty strings" in error for error in errors)


def test_flagship_recipe_is_required_for_high_severity_endpoint_failures():
    errors = MODULE.validate_allowlist({
        "cli": {}, "deferred_coherence": [], "hostname": {},
        "endpoint": {"ignore_path_prefixes": []},
    })
    assert any("fail-closed endpoint gate" in error for error in errors)
