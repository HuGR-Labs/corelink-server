"""Regression and mutation tests for the closed-world B-153 gate."""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/validate_permission_matrix.py"
spec = importlib.util.spec_from_file_location("published_permission_matrix", SCRIPT)
assert spec and spec.loader
gate = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = gate
spec.loader.exec_module(gate)


def _sources() -> dict[str, str]:
    paths = {path for row in gate.APPLIED.values() for path in row.sources}
    result: dict[str, str] = {}
    for path in paths:
        target = ROOT / path
        if target.is_dir():
            # Absence rows only need a closed-world empty surface.  Reading
            # every route into every mutation iteration makes this focused
            # suite needlessly expensive and does not add signal.
            result[path] = ""
        else:
            result[path] = target.read_text(encoding="utf-8")
    return result


def _matrix() -> str:
    return (ROOT / gate.MATRIX).read_text(encoding="utf-8")


def test_complete_published_population_is_green() -> None:
    assert gate.validate(ROOT) == []
    assert len(gate.APPLIED) == 22
    assert len(list(gate.rows(_matrix()))) == len(gate.APPLIED)


def test_every_published_role_cell_is_mutation_checked() -> None:
    source = _sources()
    matrix = _matrix()
    parsed = list(gate.rows(matrix))
    assert len(parsed) == 22
    for label, roles in parsed:
        key = gate.norm(label)
        expected = gate.APPLIED[key].roles
        for index, (actual, applied) in enumerate(zip(roles, expected)):
            replacement = gate.GRANTED if applied != gate.GRANTED else gate.DENIED
            # Replace only the first matching role cell on this row.  The
            # complete validator must report this row, in either direction.
            lines = matrix.splitlines()
            line_index = next(i for i, line in enumerate(lines) if label in line and line.lstrip().startswith("|"))
            parts = lines[line_index].split("|")
            parts[-5 + index] = f" {replacement} "
            lines[line_index] = "|".join(parts)
            mutated = "\n".join(lines)
            errors = gate.check(mutated, source)
            assert any(label in error and "drift" in error for error in errors), (label, index, errors)


def test_permissive_cas_delete_drift_is_named() -> None:
    lines = _matrix().splitlines()
    index = next(i for i, line in enumerate(lines) if "Delete a CAS blob / AC ref" in line)
    parts = lines[index].split("|")
    parts[-3] = " ❌ "
    lines[index] = "|".join(parts)
    matrix = "\n".join(lines)
    errors = gate.check(matrix, _sources())
    assert any("Delete a CAS blob / AC ref developer" in error for error in errors)
    assert any("permissive" in error for error in errors)


def test_missing_route_gate_fails_closed() -> None:
    source = _sources()
    path = "crates/corelink-container/src/routes/cas.rs"
    source[path] = source[path].replace("can_write()", "can_read()")
    errors = gate.check(_matrix(), source)
    assert any("Write CAS blobs + AC" in error and path in error for error in errors)


def test_comment_only_predicate_does_not_count() -> None:
    source = _sources()
    path = "crates/corelink-container/src/routes/cas.rs"
    for path in ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"):
        source[path] = "\n".join(f"// {line}" for line in source[path].splitlines())
    errors = gate.check(_matrix(), source)
    assert any("can_read" in error or "can_write" in error for error in errors)


def test_nested_block_comments_are_stripped() -> None:
    source = "before /* outer marker /* nested can_write() */ */ after"
    assert "can_write()" not in gate._strip_comments(source)


def test_comment_markers_inside_rust_literals_are_preserved() -> None:
    source = r'''let line = "// can_write()"; let raw = r#"/* can_read() */"#; /* hidden */'''
    cleaned = gate._strip_comments(source)
    assert '"// can_write()"' in cleaned
    assert 'r#"/* can_read() */"#' in cleaned
    assert "hidden" not in cleaned


def test_unterminated_block_comment_fails_closed() -> None:
    source = _sources()
    source["crates/corelink-container/src/routes/cas.rs"] = "/* can_write()"
    errors = gate.check(_matrix(), source)
    assert any("source comment parse failure" in error for error in errors)


def test_block_comment_only_predicates_and_routes_do_not_count() -> None:
    # Move every required predicate and route into a nested multiline block
    # comment.  The gate must reject the mutation, not find marker text inside
    # the comment and report a false green.
    source = {
        path: f"/* outer\n/* inner\n{text}\n*/\n*/"
        for path, text in _sources().items()
    }
    errors = gate.check(_matrix(), source)
    assert any("applied gate missing" in error or "served route missing" in error for error in errors)


def test_unknown_published_row_and_missing_row_fail_closed() -> None:
    source = _sources()
    matrix = _matrix().replace("| Read CAS blobs + AC", "| Unknown published permission", 1)
    errors = gate.check(matrix, source)
    assert any("unmapped published row" in error for error in errors)
    assert any("closed-world coverage failure" in error for error in errors)


def test_companion_published_claims_are_part_of_the_gate() -> None:
    role_catalog = (ROOT / gate.ROLE_CATALOG).read_text(encoding="utf-8")
    reference = (ROOT / gate.REFERENCE).read_text(encoding="utf-8")
    assert gate.check_published_claim_documents(role_catalog, reference) == []
    assert gate.check_published_claim_documents(
        role_catalog.replace("can** delete", "cannot** delete", 1), reference
    )
    assert gate.check_published_claim_documents(role_catalog, reference.replace("cache:delete", "cache:removed", 1))
