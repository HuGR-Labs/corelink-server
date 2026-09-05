"""B-145 mutation guards for endpoint decision quality."""

import json
import re
import sys
from pathlib import Path

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


def test_flagship_allowlist_is_nonempty() -> None:
    cfg = json.loads((ROOT / "scripts/docs_reality_allowlist.json").read_text())
    assert cfg["endpoint"]["flagship_files"]


def test_route_mutation_would_reintroduce_prefix_match() -> None:
    mutated = re.compile(r"^/v1(?:/.*)?$")
    assert vdr._is_generic_catchall("/{*path}")
    assert mutated.match("/v1/zzz-nonexistent-probe")


def test_generic_wildcard_is_not_endpoint_evidence() -> None:
    routes = [r for r in ["/{*path}", "/v1/customer/{id}"] if not vdr._is_generic_catchall(r)]
    regexes = [vdr._route_to_regex(route) for route in routes]
    assert not vdr.endpoint_resolves("/v1/zzz-nonexistent-probe", regexes, set())
