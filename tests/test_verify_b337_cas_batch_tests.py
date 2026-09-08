from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b337_cas_batch_tests.py"
spec = importlib.util.spec_from_file_location("b337_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


def _sources() -> tuple[str, dict[str, str]]:
    route = (ROOT / verifier.CAS_ROUTE).read_text(encoding="utf-8")
    names = tuple(name.removeprefix("cas/") for name in verifier.EXPECTED_TEST_INCLUDES) + (
        "tests_read_ceiling.rs",
    )
    parts = {
        name: (ROOT / verifier.CAS_PARTS / name).read_text(encoding="utf-8")
        for name in names
    }
    return route, parts


def test_exact_registration_and_28_test_census_are_green() -> None:
    assert verifier.assess(ROOT) == []


def test_missing_split_include_is_rejected() -> None:
    route, parts = _sources()
    mutated = route.replace(
        '    include!("cas/tests_batch_write_part2.rs");\n',
        "",
        1,
    )
    gaps = verifier.assess_source(mutated, parts)
    assert any("test-include-registration" in gap for gap in gaps)


def test_comment_or_string_decoy_cannot_satisfy_registration() -> None:
    route, parts = _sources()
    mutated = route.replace(
        '    include!("cas/tests_batch_write_part2.rs");\n',
        '    // include!("cas/tests_batch_write_part2.rs");\n'
        '    const DECOY: &str = "include!(\\"cas/tests_batch_write_part2.rs\\")";\n',
        1,
    )
    gaps = verifier.assess_source(mutated, parts)
    assert any("test-include-registration" in gap for gap in gaps)


def test_replacing_a_batch_test_does_not_preserve_semantic_identity() -> None:
    route, parts = _sources()
    mutated_parts = dict(parts)
    mutated_parts["tests_batch_write_part2.rs"] = mutated_parts[
        "tests_batch_write_part2.rs"
    ].replace(
        "async fn batch_upload_over_byte_cap_returns_413(",
        "async fn unrelated_regression_test(",
        1,
    )
    gaps = verifier.assess_source(route, mutated_parts)
    assert "batch-test-identity:tests_batch_write_part2.rs" in gaps
