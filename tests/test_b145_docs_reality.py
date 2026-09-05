"""B-145 mutation guards for endpoint decision quality."""

import json
import re
import sys
from pathlib import Path

import pytest

import importlib.util

ROOT = Path(__file__).parents[1]
spec = importlib.util.spec_from_file_location("vdr", ROOT / "scripts/validate_docs_reality.py")
vdr = importlib.util.module_from_spec(spec)
sys.modules["vdr"] = vdr
spec.loader.exec_module(vdr)


def test_nonexistent_deep_path_does_not_resolve() -> None:
    routes = ["/v1", "/v1/customer/{id}"]
    regexes = [vdr._route_to_regex(route) for route in routes if not vdr._is_generic_catchall(route)]
    assert not vdr.endpoint_resolves("/v1/zzz-nonexistent-probe", regexes, set())


def test_parameter_route_does_not_launder_deeper_path() -> None:
    route = "/v1/customer/{id}"
    regexes = [vdr._route_to_regex(route)]
    assert vdr.endpoint_resolves("/v1/customer/alice", regexes, set())
    # Mutation guard: restoring the old `(?:/.*)?$` suffix makes this assert fail.
    assert not vdr.endpoint_resolves("/v1/customer/alice/phantom", regexes, set())


def test_flagship_allowlist_is_nonempty() -> None:
    cfg = json.loads((ROOT / "scripts/docs_reality_allowlist.json").read_text())
    assert cfg["endpoint"]["flagship_files"]


def test_route_mutation_would_reintroduce_prefix_match() -> None:
    mutated = re.compile(r"^/v1(?:/.*)?$")
    assert mutated.match("/v1/zzz-nonexistent-probe")
    assert not vdr.endpoint_resolves("/v1/zzz-nonexistent-probe", [vdr._route_to_regex("/v1")], set())


def test_parameter_root_is_not_public_endpoint_evidence(tmp_path, monkeypatch) -> None:
    source = tmp_path / "npm.rs"
    source.write_text('Router::new().route("/{pkg}", get(handle_metadata));\n', encoding="utf-8")
    monkeypatch.setattr(vdr, "ROUTE_SOURCE_ROOTS", [tmp_path])
    inventory = vdr.collect_route_inventory()
    assert vdr._is_generic_catchall("/{pkg}")
    assert not inventory.resolves("/v1/zzz-nonexistent-probe")


def test_route_method_is_checked_when_proven() -> None:
    inventory = vdr.RouteInventory((
        vdr.RouteRegistration("/v1/pats", "route", frozenset({"POST"})),
    ))
    assert inventory.resolves("/v1/pats", "POST")
    assert not inventory.resolves("/v1/pats", "DELETE")


def test_generic_wildcard_is_not_endpoint_evidence() -> None:
    routes = [r for r in ["/{*path}", "/v1/customer/{id}"] if not vdr._is_generic_catchall(r)]
    regexes = [vdr._route_to_regex(route) for route in routes]
    assert not vdr.endpoint_resolves("/v1/zzz-nonexistent-probe", regexes, set())


def test_comment_only_route_and_worker_paths_are_not_evidence(tmp_path, monkeypatch) -> None:
    source = tmp_path / "routes.rs"
    source.write_text(
        "/* outer\n/* nested .route(\"/v1/comment-only\") */\n*/\n"
        ".route(\"/v1/live\", get(handler));\n",
        encoding="utf-8",
    )
    worker = tmp_path / "worker.ts"
    worker.write_text(
        "/* worker-only /v1/worker-only\n"
        "   still comment */\nconst path = \"/v1/live-worker\";\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(vdr, "ROUTE_SOURCE_ROOTS", [tmp_path])
    inventory = vdr.collect_route_inventory()
    assert "/v1/live" in inventory.all
    assert "/v1/live-worker" in inventory.all
    assert "/v1/comment-only" not in inventory.all
    assert "/v1/worker-only" not in inventory.all
    assert not inventory.resolves("/v1/comment-only")
    assert not inventory.resolves("/v1/worker-only")


def test_route_scanner_fails_closed_on_unterminated_comment(tmp_path, monkeypatch) -> None:
    source = tmp_path / "routes.rs"
    source.write_text('/* .route("/v1/comment-only")', encoding="utf-8")
    monkeypatch.setattr(vdr, "ROUTE_SOURCE_ROOTS", [tmp_path])
    with pytest.raises(vdr.SourceSyntaxError):
        vdr.collect_route_inventory()


def test_cli_model_ignores_comment_only_variant(tmp_path) -> None:
    source = tmp_path / "main.rs"
    source.write_text(
        "enum Commands {\n"
        "    /* outer\n"
        "    /* FakeCommand, */\n"
        "    */\n"
        "    RealCommand,\n"
        "}\n",
        encoding="utf-8",
    )
    model = vdr.parse_cli_model(source)
    assert "real-command" in model.top
    assert "fake-command" not in model.top


def test_source_literals_keep_comment_markers() -> None:
    cleaned = vdr._strip_source_comments(
        r'''let a = "// /v1/literal"; let b = r#"/* /v1/raw */"#; /* hidden */'''
    )
    assert '"// /v1/literal"' in cleaned
    assert 'r#"/* /v1/raw */"#' in cleaned
    assert "hidden" not in cleaned
