#!/usr/bin/env python3
"""Fail-closed executable contract guard for B-144 PAT revocation.

The production boundary is intentionally checked in two places: the HTTP route
must admit only a server-trusted owner/admin role, while D1 must prevent an
admin from targeting owner/legacy rows.  This guard also checks the adversarial
tests and published contract copies, and its mutation checks prove that the
important predicates are real gates rather than comments.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


try:
    from validate_permission_matrix import has_active_guard
except ModuleNotFoundError:
    # The verifier is also imported directly by pytest, where `scripts/` is
    # not on sys.path. Reuse the same lexer/control-flow proof as B-153.
    import importlib.util

    _validator_path = Path(__file__).with_name("validate_permission_matrix.py")
    _validator_spec = importlib.util.spec_from_file_location(
        "validate_permission_matrix", _validator_path
    )
    if _validator_spec is None or _validator_spec.loader is None:
        raise ImportError(f"cannot load {_validator_path}")
    _validator = importlib.util.module_from_spec(_validator_spec)
    sys.modules[_validator_spec.name] = _validator
    _validator_spec.loader.exec_module(_validator)
    has_active_guard = _validator.has_active_guard


ROOT = Path(__file__).resolve().parent.parent
FILES = {
    "route": Path("crates/corelink-container/src/routes/customer.rs"),
    "d1": Path("crates/corelink-container/src/customer_d1.rs"),
    "request": Path("crates/corelink-handler-customer/src/request.rs"),
    "route_tests": Path("crates/corelink-container/src/routes/customer/tests_keys.rs"),
    "d1_tests": Path("crates/corelink-container/src/customer_d1.rs"),
    "openapi": Path("openapi/corelink-v1.yaml"),
    "docs_http": Path("apps/docs/docs/api/http.md"),
    "docs_matrix": Path("apps/docs/docs/explanation/rbac/permission-matrix.mdx"),
    "docs_security": Path("apps/docs/docs/security.md"),
    "knowledge": Path("docs/knowledge/auth/d1-pat-store.md"),
    "workflow": Path(".github/workflows/backlog-verify.yml"),
    "backlog": Path("BACKLOG.md"),
}
LOCALE_MATRICES = tuple(
    Path(f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/explanation/rbac/permission-matrix.mdx")
    for locale in ("de", "es-419", "pt-BR")
)
LOCALE_SECURITY = tuple(
    Path(f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/security.md")
    for locale in ("de", "es-419", "pt-BR")
)
ROLE_CATALOGS = (
    Path("apps/docs/docs/explanation/rbac/role-catalog.mdx"),
    *(Path(f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/explanation/rbac/role-catalog.mdx") for locale in ("de", "es-419", "pt-BR")),
)
TENANCY_DOCS = (
    Path("apps/docs/docs/concepts/tenancy.md"),
    *(Path(f"apps/docs/i18n/{locale}/docusaurus-plugin-content-docs/current/concepts/tenancy.md") for locale in ("de", "es-419", "pt-BR")),
)
GENERATED_OPENAPI = (
    Path("openapi/corelink-v1.json"),
    Path("worker/src/lib/openapi_v1.ts"),
    Path("apps/docs/static/openapi-corelink-v1.yaml"),
)


class InstrumentError(RuntimeError):
    """A required input is absent or unreadable."""


def read(root: Path, relative: Path) -> str:
    try:
        path = root / relative
        source = path.read_text(encoding="utf-8")
        # Several governed Rust modules are split with include! while request
        # seams use `mod name;`. Read the assembled source so the guard remains
        # load-bearing after a re-anchor; the manifest still names stable root
        # paths for actionable diagnostics.
        parts = [source]
        include_re = re.compile(r'include!\(\s*"([^"]+)"\s*\)')
        for child_name in include_re.findall(source):
            child = path.parent / child_name
            if child.is_file():
                parts.append(read(root, child.relative_to(root)))
        module_re = re.compile(r"^\s*mod\s+([A-Za-z0-9_]+)\s*;", re.MULTILINE)
        for module_name in module_re.findall(source):
            candidates = (
                path.parent / f"{module_name}.rs",
                path.parent / module_name / "mod.rs",
                path.parent / path.stem / f"{module_name}.rs",
                path.parent / path.stem / module_name / "mod.rs",
            )
            child = next((candidate for candidate in candidates if candidate.is_file()), None)
            if child is not None:
                parts.append(read(root, child.relative_to(root)))
        return "\n".join(parts)
    except (OSError, UnicodeError) as exc:
        raise InstrumentError(f"cannot read {relative}: {exc}") from exc


def body(text: str, start: str, end: str) -> str:
    first = text.find(start)
    if first < 0:
        return ""
    last = text.find(end, first + len(start))
    return text[first:] if last < 0 else text[first:last]


def assess(root: Path = ROOT, overrides: dict[Path, str] | None = None) -> list[str]:
    overrides = overrides or {}

    def get(path: Path) -> str:
        return overrides.get(path, read(root, path))

    route = get(FILES["route"])
    revoke_route = body(route, "async fn handle_keys_revoke(", "/// `GET /v1/customer/team`")
    d1 = get(FILES["d1"])
    revoke_d1 = body(d1, "fn revoke(&self, req: KeyRevokeRequest)", "\n}\n\nimpl CustomerTeamHandler")
    request = get(FILES["request"])
    route_tests = get(FILES["route_tests"])
    gaps: list[str] = []

    if not revoke_route:
        gaps.append("route-revoke-body-missing")
    elif "caller_is_owner_or_admin(&headers)" not in revoke_route:
        gaps.append("route-owner-admin-gate-missing")
    elif not has_active_guard(
        revoke_route,
        "async fn handle_keys_revoke(",
        "async fn handle_team_list(",
        ("!", "caller_is_owner_or_admin", "(", "&", "headers", ")"),
        ("return", "(", "StatusCode", "::", "FORBIDDEN"),
        ("state", ".", "keys", ".", "revoke", "("),
    ):
        gaps.append("route-owner-admin-gate-not-load-bearing")
    if "KeyRevokeRequest::with_role(" not in revoke_route:
        gaps.append("route-role-not-forwarded")
    if "only the owner or an admin may revoke a credential" not in revoke_route:
        gaps.append("route-fail-closed-error-missing")
    if "requires_cache_write" in revoke_route:
        gaps.append("route-still-uses-cache-scope-as-authority")

    required_d1 = (
        "SELECT pat_id, token_id, name, scope, created_ms, revoked_at_ms, find_only, principal_id",
        "FROM pat WHERE pat_id = ?1 AND tenant_id = ?2 LIMIT 1",
        'req.caller_role.trim().eq_ignore_ascii_case("admin")',
        'col_opt_str(&row, "principal_id").is_none()',
        "CustomerHandlerError::Unauthorized(",
        "UPDATE pat SET revoked_at_ms = ?1",
        "tenant_id = ?3 AND revoked_at_ms IS NULL",
    )
    if not revoke_d1:
        gaps.append("d1-revoke-body-missing")
    for marker in required_d1:
        if marker not in revoke_d1:
            gaps.append(f"d1-missing:{marker}")
    if revoke_d1.find("AuditEventKind::KeyRevokeAttempted") > revoke_d1.find("SELECT pat_id"):
        gaps.append("d1-attempted-audit-after-select")
    if revoke_d1.find("AuditEventKind::KeyRevokeCommitted") < revoke_d1.find("UPDATE pat"):
        gaps.append("d1-committed-audit-before-update")
    if revoke_d1 and not has_active_guard(
        revoke_d1,
        "fn revoke(&self, req: KeyRevokeRequest)",
        "__no_earlier_end_marker__",
        ("req", ".", "caller_role", ".", "trim", "(", ")", ".", "eq_ignore_ascii_case", "(", '"admin"', ")", "&&", "col_opt_str", "(", "&", "row", ",", '"principal_id"', ")", ".", "is_none", "(", ")"),
        ("return", "Err", "(", "CustomerHandlerError", "::", "Unauthorized", "("),
        ("self", ".", "run", "("),
        True,
    ):
        gaps.append("d1-owner-protection-not-load-bearing")

    if "pub caller_role: String" not in request or "pub fn with_role(" not in request:
        gaps.append("request-role-carrying-seam-missing")

    for marker in (
        "keys_revoke_rejects_member_viewer_and_missing_role_before_handler",
        'for role in ["member", "viewer", "", "operator"]',
    ):
        if marker not in route_tests:
            gaps.append(f"route-adversarial-test-missing:{marker}")
    for marker in (
        "keys_revoke_admin_cannot_revoke_owner_pat",
        "keys_revoke_admin_can_revoke_member_pat_within_tenant",
        "keys_revoke_is_idempotent_no_second_update",
    ):
        if marker not in d1:
            gaps.append(f"d1-behavioral-test-missing:{marker}")

    openapi = get(FILES["openapi"])
    for marker in ("server-trusted", "owner", "admin", "cache-write scope alone", "cross-tenant"):
        if marker not in openapi:
            gaps.append(f"openapi-missing:{marker}")
    if "tenant-wide, so it requires a cache-write" in openapi:
        gaps.append("openapi-old-tenant-wide-claim")

    for relative in (FILES["docs_http"], FILES["docs_matrix"], FILES["docs_security"], FILES["knowledge"], *LOCALE_MATRICES, *LOCALE_SECURITY, *ROLE_CATALOGS, *TENANCY_DOCS):
        text = get(relative)
        if "tenant-wide operation, so it needs a cache-write" in text:
            gaps.append(f"stale-doc-cache-write-revoke:{relative}")
        if "Mint and revoke PATs for any user" in text or "non-destructive op" in text:
            gaps.append(f"stale-doc-role-claim:{relative}")
        if "server-trusted" not in text.lower() and "owner" not in text.lower():
            gaps.append(f"docs-role-claim-missing:{relative}")
    workflow = get(FILES["workflow"])
    if "B-144" not in workflow or "verify_b144_pat_revoke.py" not in workflow:
        gaps.append("workflow-b144-wiring-missing")

    for relative in GENERATED_OPENAPI:
        generated = get(relative)
        if "server-trusted" not in generated or "cache-write scope alone" not in generated:
            gaps.append(f"generated-openapi-stale:{relative}")

    backlog = get(FILES["backlog"])
    item = body(backlog, "id: B-144", "### B-145")
    if "status: done" not in item or "verify_b144_pat_revoke.py --expect done" not in item:
        gaps.append("backlog-done-guard-missing")
    if re.search(r"verify-means:\s*\|\s*\n\s*open\b", item):
        gaps.append("backlog-guard-still-open-polarity")
    return gaps


def mutation_checks(root: Path = ROOT) -> None:
    baseline = assess(root)
    if baseline:
        raise InstrumentError("baseline is not clean: " + ", ".join(baseline))

    route_path = FILES["route"]
    route = read(root, route_path)
    mutant = route.replace("caller_is_owner_or_admin(&headers)", "true", 1)
    if not assess(root, {route_path: mutant}):
        raise InstrumentError("mutation escaped: route role gate can be removed")

    mutant = route.replace(
        "if !caller_is_owner_or_admin(&headers)",
        "if caller_is_owner_or_admin(&headers)",
        1,
    )
    if not assess(root, {route_path: mutant}):
        raise InstrumentError("mutation escaped: route role gate can be inverted")

    mutant = route.replace(
        "caller_is_owner_or_admin(&headers)",
        '"caller_is_owner_or_admin(&headers)"',
        1,
    )
    if not assess(root, {route_path: mutant}):
        raise InstrumentError("mutation escaped: route string bait replaced the role gate")

    mutant = route.replace(
        "if !caller_is_owner_or_admin(&headers)",
        "let _owner_admin = caller_is_owner_or_admin(&headers); if true",
        1,
    )
    if not assess(root, {route_path: mutant}):
        raise InstrumentError("mutation escaped: route role predicate is unused")

    mutant = route.replace(
        "if !caller_is_owner_or_admin(&headers)",
        "if false && !caller_is_owner_or_admin(&headers)",
        1,
    )
    if not assess(root, {route_path: mutant}):
        raise InstrumentError("mutation escaped: route role gate is statically dead")

    d1_path = FILES["d1"]
    d1 = read(root, d1_path)
    mutant = d1.replace('eq_ignore_ascii_case("admin")', 'eq_ignore_ascii_case("owner")', 1)
    if not assess(root, {d1_path: mutant}):
        raise InstrumentError("mutation escaped: D1 admin owner safeguard can be changed")

    mutant = d1.replace(
        'if req.caller_role.trim().eq_ignore_ascii_case("admin")',
        'if false && req.caller_role.trim().eq_ignore_ascii_case("admin")',
        1,
    )
    if not assess(root, {d1_path: mutant}):
        raise InstrumentError("mutation escaped: D1 owner safeguard can be dead")

    # Exact reviewer reproducer: a valid owner branch hidden inside a dead
    # outer branch. A marker-only verifier would still report this as green.
    mutant = d1.replace(
        'if req.caller_role.trim().eq_ignore_ascii_case("admin")',
        'if false { if req.caller_role.trim().eq_ignore_ascii_case("admin")',
        1,
    )
    if not assess(root, {d1_path: mutant}):
        raise InstrumentError("mutation escaped: D1 owner safeguard can be nested in dead code")

    mutant = d1.replace(", find_only, principal_id \\", ", find_only \\", 1)
    if not assess(root, {d1_path: mutant}):
        raise InstrumentError("mutation escaped: D1 principal marker can be removed")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--expect", choices=("open", "done"), default="done")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.root)
        actual = "open" if gaps else "done"
        print(f"B-144 {actual}: {len(gaps)} gap(s)")
        for gap in gaps:
            print(f"- {gap}")
        if args.self_test:
            mutation_checks(args.root)
            print("B-144 mutation checks: passed")
    except InstrumentError as exc:
        print(f"instrument error: {exc}", file=sys.stderr)
        return 2
    return 0 if actual == args.expect else 1


if __name__ == "__main__":
    raise SystemExit(main())
