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
            # Route modules are include!-assembled; use the same source loader
            # as the production validator so mutations hit the live parts.
            errors: list[str] = []
            result[path] = gate._read(ROOT, path, errors)
            assert errors == [], errors
    return result


def _matrix() -> str:
    return (ROOT / gate.MATRIX).read_text(encoding="utf-8")


def test_complete_published_population_is_green() -> None:
    assert gate.validate(ROOT) == []
    assert len(gate.APPLIED) == 22
    assert len(list(gate.rows(_matrix()))) == len(gate.APPLIED)


def test_permission_matrix_notice_is_current_in_published_locales() -> None:
    paths = [ROOT / gate.MATRIX]
    paths.extend(sorted((ROOT / "apps" / "docs" / "i18n").glob(
        "*/docusaurus-plugin-content-docs/current/explanation/rbac/permission-matrix.mdx"
    )))
    assert len(paths) == 4
    for path in paths:
        contents = path.read_text(encoding="utf-8")
        assert "**No gate checks it.**" not in contents
        assert "row or gate drift fails closed" in contents


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


def test_load_bearing_gate_mutations_stay_red() -> None:
    source = _sources()
    route_path = "crates/corelink-container/src/routes/customer.rs"
    route = source[route_path]
    start = route.index("async fn handle_keys_revoke(")
    end = route.index("async fn handle_team_list(", start)
    route_mutant = route[:start] + route[start:end].replace(
        "caller_is_owner_or_admin(&headers)", "true", 1
    ) + route[end:]
    source[route_path] = route_mutant
    errors = gate.check(_matrix(), source)
    assert any("Revoke a PAT in tenant" in error and "context" in error for error in errors)

    source = _sources()
    for path, end_marker in (
        (
            "crates/corelink-container/src/routes/cas.rs",
            "async fn handle_list(",
        ),
        (
            "crates/corelink-container/src/routes/ac.rs",
            "async fn handle_list_refs(",
        ),
    ):
        text = source[path]
        start = text.index("async fn handle_delete(")
        end = text.index(end_marker, start)
        source[path] = text[:start] + text[start:end].replace(
            "scope.can_write()", "scope.can_read()", 1
        ) + text[end:]
    errors = gate.check(_matrix(), source)
    assert any("Delete a CAS blob / AC ref" in error and "context" in error for error in errors)


def test_exact_reviewer_reproducers_stay_red() -> None:
    """The B144/B153 proof must reject bait, dead branches, and all predicates."""

    route_path = "crates/corelink-container/src/routes/customer.rs"
    d1_path = "crates/corelink-container/src/customer_d1.rs"
    cas_path = "crates/corelink-container/src/routes/cas.rs"

    source = _sources()
    source[route_path] = source[route_path].replace(
        "caller_is_owner_or_admin(&headers)",
        '"caller_is_owner_or_admin(&headers)"',
        1,
    )
    errors = gate.check(_matrix(), source)
    assert any("Revoke a PAT in tenant" in error and "load-bearing" in error for error in errors)

    source = _sources()
    source[route_path] = source[route_path].replace(
        "if !caller_is_owner_or_admin(&headers)",
        "let _owner_admin = caller_is_owner_or_admin(&headers); if true",
        1,
    )
    errors = gate.check(_matrix(), source)
    assert any("Revoke a PAT in tenant" in error and "load-bearing" in error for error in errors)

    source = _sources()
    source[d1_path] = source[d1_path].replace(
        'if req.caller_role.trim().eq_ignore_ascii_case("admin")',
        'if false && req.caller_role.trim().eq_ignore_ascii_case("admin")',
        1,
    )
    errors = gate.check(_matrix(), source)
    assert any("Revoke a PAT in tenant" in error and "load-bearing" in error for error in errors)

    source = _sources()
    owner_guard = 'if req.caller_role.trim().eq_ignore_ascii_case("admin")'
    source[d1_path] = source[d1_path].replace(
        owner_guard,
        'if false { ' + owner_guard,
        1,
    )
    errors = gate.check(_matrix(), source)
    assert any("Revoke a PAT in tenant" in error and "load-bearing" in error for error in errors)

    # Three direct predicate mutations: route role, CAS delete scope, and D1
    # owner target safeguard. Each must independently reopen its row.
    mutations = (
        (route_path, "if !caller_is_owner_or_admin(&headers)", "if caller_is_owner_or_admin(&headers)"),
        (cas_path, "scope.can_write()", "scope.can_read()"),
        (d1_path, 'if req.caller_role.trim().eq_ignore_ascii_case("admin")', 'if !req.caller_role.trim().eq_ignore_ascii_case("admin")'),
    )
    for path, old, new in mutations:
        source = _sources()
        start = {
            route_path: "async fn handle_keys_revoke(",
            cas_path: "async fn handle_delete(",
            d1_path: "fn revoke(&self, req: KeyRevokeRequest)",
        }[path]
        end = {
            route_path: "async fn handle_team_list(",
            cas_path: "async fn handle_list(",
            d1_path: "impl CustomerTeamHandler",
        }[path]
        first = source[path].index(start)
        last = source[path].index(end, first)
        context = source[path][first:last]
        assert old in context
        source[path] = source[path][:first] + context.replace(old, new, 1) + source[path][last:]
        errors = gate.check(_matrix(), source)
        assert any("Revoke a PAT in tenant" in error or "Delete a CAS blob / AC ref" in error for error in errors), (path, errors)


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


def test_role_header_stale_missing_and_extra_fail_closed() -> None:
    matrix = _matrix()
    for replacement in (
        ("| Owner | Admin | Member | Viewer |", "| Owner | Admin | Developer | Viewer |"),
        ("| Owner | Admin | Member | Viewer |", "| Owner | Admin | Viewer |"),
        ("| Owner | Admin | Member | Viewer |", "| Owner | Admin | Member | Viewer | Support |"),
    ):
        mutated = matrix.replace(*replacement)
        errors = gate.check(mutated, _sources())
        assert any("role header drift" in error for error in errors), (replacement, errors)


def test_missing_manifest_source_fails_closed() -> None:
    source = _sources()
    source.pop("crates/corelink-container/src/routes/cas.rs")
    errors = gate.check(_matrix(), source)
    assert any("applied gate missing" in error for error in errors)


def test_extra_manifest_mapping_fails_closed() -> None:
    source = _sources()
    original = gate.APPLIED.get("extra-test-row")
    gate.APPLIED["extra-test-row"] = gate.Applied(
        (gate.DENIED,) * 4,
        ("crates/corelink-container/src/routes",),
        (),
        None,
        ("definitely-not-served",),
    )
    try:
        errors = gate.check(_matrix(), source)
    finally:
        if original is None:
            gate.APPLIED.pop("extra-test-row", None)
        else:
            gate.APPLIED["extra-test-row"] = original
    assert any("checked 22 of 23 mapped rows" in error for error in errors)


def test_companion_published_claims_are_part_of_the_gate() -> None:
    role_catalog = (ROOT / gate.ROLE_CATALOG).read_text(encoding="utf-8")
    reference = (ROOT / gate.REFERENCE).read_text(encoding="utf-8")
    assert gate.check_published_claim_documents(role_catalog, reference) == []
    assert gate.check_published_claim_documents(
        role_catalog.replace("can** delete", "cannot** delete", 1), reference
    )
    assert gate.check_published_claim_documents(role_catalog, reference.replace("cache:delete", "cache:removed", 1))
