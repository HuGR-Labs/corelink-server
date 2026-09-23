"""Focal executable and inverted-mutation tests for B193..B214 closures."""
import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b193_b214_closures", ROOT / "scripts/verify_b193_b214_closures.py"
)
assert SPEC is not None and SPEC.loader is not None
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


def test_all_closures_are_green_and_mutations_are_red() -> None:
    assert gate.verify(ROOT) == {"closed": 21, "mutations": 21}


@pytest.mark.parametrize("identifier", tuple(gate.CONTRACTS))
def test_each_load_bearing_clause_is_mutation_sensitive(identifier: str) -> None:
    artifact, _, needle = gate.CONTRACTS[identifier]
    source = (ROOT / artifact).read_text(encoding="utf-8")
    mutated = source.replace(needle, "__REMOVED_BY_MUTATION__")
    with pytest.raises(gate.ClosureError):
        gate._check(identifier, ROOT, mutated)
