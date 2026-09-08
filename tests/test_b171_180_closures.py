"""Focused executable and mutation tests for the D03 B171-B180 closures."""
import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
_SPEC = importlib.util.spec_from_file_location(
    "verify_b171_180_closures", ROOT / "scripts/verify_b171_180_closures.py"
)
assert _SPEC is not None and _SPEC.loader is not None
gate = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(gate)


def test_all_ten_closures_are_behaviorally_green_and_mutation_red() -> None:
    assert gate.verify(ROOT) == {"closed": 10, "mutations": 10}


@pytest.mark.parametrize("identifier", tuple(gate.CONTRACTS))
def test_each_load_bearing_clause_is_mutation_sensitive(identifier: str) -> None:
    artifact, _, needle = gate.CONTRACTS[identifier]
    source = (ROOT / artifact).read_text(encoding="utf-8")
    mutated = source.replace(needle, "__REMOVED_BY_MUTATION__")
    with pytest.raises(gate.ClosureError):
        gate._check(identifier, ROOT, mutated)


def test_b174_probe_contract_is_fail_closed_and_mutation_sensitive() -> None:
    helper = (ROOT / "scripts/d1-apply-fk-parent.sh").read_text(encoding="utf-8")
    assert "query_json" in helper
    assert "refusing to infer database state" in helper
    assert 'run --file "$TMP_SQL" >/dev/null' in helper
    assert "|| true" not in helper


def test_b174_dead_branch_reproducer_is_red() -> None:
    artifact, _, needle = gate.CONTRACTS["B-174"]
    source = (ROOT / artifact).read_text(encoding="utf-8")
    dead = source.replace(needle, f"if false; then\n    {needle}\nfi", 1)
    with pytest.raises(gate.ClosureError, match="active shell code"):
        gate._check("B-174", ROOT, dead)


def test_b178_dead_branch_reproducer_is_red() -> None:
    artifact, _, needle = gate.CONTRACTS["B-178"]
    source = (ROOT / artifact).read_text(encoding="utf-8")
    start = source.index("export function monthlyRequestCountStatement")
    conflict = source.index('"ON CONFLICT(tenant_id, year_month) " +', start)
    replacement = '"ON CONFLICT(noop) " +'
    dead_bait = (
        'if (false) { db.prepare("INSERT INTO monthly_request_counts '
        'ON CONFLICT(tenant_id, year_month)").bind(1).first(); }\n'
    )
    mutated = (
        dead_bait
        + source[:conflict]
        + replacement
        + source[conflict + len('"ON CONFLICT(tenant_id, year_month) " +') :]
    )
    with pytest.raises(gate.ClosureError, match="active prepared SQL"):
        gate._check("B-178", ROOT, mutated)


def test_b173_upload_contract_is_streamed_and_bounded() -> None:
    handlers = (ROOT / "crates/corelink-adapter-host/src/oci/server/handlers.rs").read_text(
        encoding="utf-8"
    )
    assert "into_data_stream" in handlers
    assert "MAX_UPLOAD_REQUEST_BYTES" in handlers
    assert "collect_upload_body(body, max_body_bytes)" in handlers
    assert "to_bytes(body, max_body_bytes)" not in handlers
