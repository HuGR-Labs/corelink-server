#!/usr/bin/env python3
"""Compare every role-bearing published permission row with live route gates.

This is intentionally a small, stdlib-only structural gate.  The published
matrix is a claim, not an authority: each claim is paired with the route and
predicate that enforce it.  Unknown rows, missing sources, route removal,
comment-only predicates, and either direction of role drift fail closed.

The checker covers the complete role-bearing population in
``permission-matrix.mdx`` (22 rows at the time of writing), rather than
silently focusing on the original CAS example.
"""
from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MATRIX = Path("apps/docs/docs/explanation/rbac/permission-matrix.mdx")
ROLE_CATALOG = Path("apps/docs/docs/explanation/rbac/role-catalog.mdx")
REFERENCE = Path("apps/docs/docs/reference/rbac/permissions.mdx")

ROLES = ("owner", "admin", "developer", "viewer")
GRANTED = "✅"
DENIED = "❌"
ABSENT = "—"
_RUST_TOKEN = re.compile(r"[/rb\"']")
_BLOCK_TOKEN = re.compile(r"[/*\n]")
_COMMENT_TEXT = re.compile(r"[^\n]")


@dataclass(frozen=True)
class Applied:
    roles: tuple[str, str, str, str]
    sources: tuple[str, ...]
    required: tuple[str, ...]
    route: str | None = None
    absent: tuple[str, ...] = ()
    required_by_source: tuple[tuple[str, tuple[str, ...]], ...] = ()


class CommentSyntaxError(ValueError):
    """A Rust block comment was not terminated."""


# The closed-world manifest is deliberately explicit.  Adding a row to the
# documentation without adding an applied record is an instrument failure.
APPLIED: dict[str, Applied] = {
    # CAS / AC
    "readcasblobsac": Applied((GRANTED,) * 4, ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"), ("can_read()",), "CAS_READ_ROUTE", required_by_source=(("crates/corelink-container/src/routes/cas.rs", ("can_read()",)), ("crates/corelink-container/src/routes/ac.rs", ("can_read()",)))),
    "writecasblobsac": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"), ("can_write()",), ".put(", required_by_source=(("crates/corelink-container/src/routes/cas.rs", ("can_write()",)), ("crates/corelink-container/src/routes/ac.rs", ("can_write()",)))),
    "findmissingblobs": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/bazel_v2.rs",), ("can_find_missing()",), "findMissingBlobs"),
    "deleteacasblobacref": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"), ("can_write()",), ".delete(", required_by_source=(("crates/corelink-container/src/routes/cas.rs", ("can_write()",)), ("crates/corelink-container/src/routes/ac.rs", ("can_write()",)))),
    # Team membership and customer credentials
    "inviteuserdeveloperviewer": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("caller_is_owner_or_admin", "role_is_privileged"), "/v1/customer/team/invite"),
    "inviteuseradmin": Applied((GRANTED, DENIED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("caller_is_owner_or_admin", "role_is_privileged"), "/v1/customer/team/invite"),
    "inviteuserowner": Applied((DENIED,) * 4, ("crates/corelink-container/src/routes/customer.rs",), ("owner role is not grantable",), "/v1/customer/team/invite"),
    "revokeusersoftdeleterow": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("caller_is_owner_or_admin",), "/v1/customer/team/"),
    "listpatsintenant": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_cache_write",), "/v1/customer/keys"),
    "mintpat": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_cache_write",), "/v1/customer/keys"),
    "revokeapatintenant": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_cache_write",), "/v1/customer/keys/"),
    "deletethewholeaccount": Applied((GRANTED, DENIED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("CLERK_TOKEN_PREFIX", "ROLE_HEADER", '!= "owner"'), "/v1/customer/account/delete"),
    # Audit
    "readauditlogofthetenantdashboard": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_billing_admin",), "/v1/customer/audit"),
    "exportauditloganalytics": Applied((GRANTED, GRANTED, GRANTED, GRANTED), ("crates/corelink-container/src/routes/audit_export.rs", "crates/corelink-container/src/routes/audit_export/handler.rs", "crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs", "crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs"), ("requires_audit_read",), "AUDIT_EXPORT_ROUTE"),
    "rfc6962inclusionproof": Applied((GRANTED,) * 4, ("crates/corelink-container/src/routes/audit_export.rs", "crates/corelink-container/src/routes/audit_export/handler.rs", "crates/corelink-container/src/routes/audit_export/stream.rs"), ("requires_audit_read", "verify_inclusion_proof"), "AUDIT_EXPORT_ROUTE"),
    "queryadminoplog": Applied((ABSENT,) * 4, ("crates/corelink-container/src/routes",), (), None, ("admin_op_log",)),
    # Billing
    "viewbillingdetail": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_billing_admin",), "/v1/customer/billing"),
    "managepaymentmethodcancelsubscription": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_billing_admin",), "/v1/customer/billing/portal"),
    "selectchangetier": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/tier_select.rs",), ("verify_internal_auth", "RequestedTier"), "/v1/onboarding/tier-select"),
    "applytaxexemption": Applied((ABSENT,) * 4, ("crates/corelink-container/src/routes",), (), None, ("tax exemption",)),
    # Phase-2 executor identities are not human roles; their published rows
    # are still role-bearing and must remain denied for every human role.
    "reportactionstarted": Applied((DENIED,) * 4, ("crates/corelink-container/src/routes",), (), None, ("/v2/execution/action/start",)),
    "reportresult": Applied((DENIED,) * 4, ("crates/corelink-container/src/routes",), (), None, ("/v2/execution/result",)),
}


def norm(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", value.lower())


def _clean_cell(value: str) -> str:
    value = re.sub(r"\*+", "", value).strip()
    if value.startswith("❌"):
        return DENIED
    if value.startswith("✅"):
        return GRANTED
    if value.startswith("—") or value == "-":
        return ABSENT
    return value


def rows(text: str):
    """Yield (label, four role cells) from every canonical role table."""
    lines = text.splitlines()
    role_header = re.compile(r"^\s*\|\s*Permission\s*\|.*\|\s*Owner\s*\|\s*Admin\s*\|\s*Developer\s*\|\s*Viewer\s*\|\s*$", re.I)
    for index, line in enumerate(lines):
        if not role_header.match(line):
            continue
        for candidate in lines[index + 2 :]:
            if not candidate.lstrip().startswith("|"):
                break
            cells = [cell.strip() for cell in candidate.strip().strip("|").split("|")]
            if len(cells) < 5 or set(cells) <= {"-", "—", ""}:
                continue
            # CAS tables have Scope + Served route before the roles; the other
            # tables have Served route + Gate.  In either shape roles are last.
            yield cells[0], tuple(_clean_cell(c) for c in cells[-4:])


def _raw_string_start(source: str, index: int) -> tuple[int, int] | None:
    """Return (opening-quote index, hash count) for a Rust raw string."""
    prefix_end = index
    if source.startswith("br", index):
        prefix_end += 2
    elif source.startswith("r", index):
        prefix_end += 1
    else:
        return None
    hash_count = 0
    while prefix_end + hash_count < len(source) and source[prefix_end + hash_count] == "#":
        hash_count += 1
    quote = prefix_end + hash_count
    if quote >= len(source) or source[quote] != '"':
        return None
    return quote, hash_count


def _strip_comments(source: str) -> str:
    """Remove Rust comments while retaining literals and source line numbers.

    Rust permits nested block comments.  Comment text is replaced with spaces
    (newlines are retained), so a predicate can never satisfy the manifest from
    inside `/* ... */` or `// ...`.  An unterminated block or raw string is an
    instrument error rather than a permissive result.
    """
    output: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        token = _RUST_TOKEN.search(source, index)
        if token is None:
            output.append(source[index:])
            break
        if token.start() > index:
            output.append(source[index : token.start()])
            index = token.start()
        character = source[index]
        if character == "/" and source.startswith("//", index):
            output.extend((" ", " "))
            index += 2
            while index < length and source[index] != "\n":
                output.append(" ")
                index += 1
            continue
        if character == "/" and source.startswith("/*", index):
            output.extend((" ", " "))
            index += 2
            depth = 1
            while index < length and depth:
                token = _BLOCK_TOKEN.search(source, index)
                if token is None:
                    output.append(_COMMENT_TEXT.sub(" ", source[index:]))
                    index = length
                    break
                if token.start() > index:
                    output.append(_COMMENT_TEXT.sub(" ", source[index : token.start()]))
                    index = token.start()
                character = source[index]
                if character == "/" and source.startswith("/*", index):
                    output.extend((" ", " "))
                    index += 2
                    depth += 1
                elif character == "*" and source.startswith("*/", index):
                    output.extend((" ", " "))
                    index += 2
                    depth -= 1
                elif character == "\n":
                    output.append("\n")
                    index += 1
                else:
                    output.append(" ")
                    index += 1
            if depth:
                raise CommentSyntaxError("unterminated Rust block comment")
            continue

        if character in {"r", "b"}:
            raw = _raw_string_start(source, index)
            if raw is not None:
                quote, hash_count = raw
                closing = '"' + ("#" * hash_count)
                end = source.find(closing, quote + 1)
                if end < 0:
                    raise CommentSyntaxError("unterminated Rust raw string")
                output.extend(source[index : end + len(closing)])
                index = end + len(closing)
                continue

        # Byte strings/chars enter the same literal states as their ordinary
        # counterparts.  The prefix is copied before consuming the quote.
        if character == "b" and index + 1 < length and source[index + 1] in {'"', "'"}:
            output.append(source[index])
            index += 1

        if index < length and source[index] == '"':
            output.append(source[index])
            index += 1
            terminated = False
            while index < length:
                character = source[index]
                output.append(character)
                index += 1
                if character == "\\" and index < length:
                    output.append(source[index])
                    index += 1
                elif character == '"':
                    terminated = True
                    break
            if not terminated:
                raise CommentSyntaxError("unterminated Rust string literal")
            continue

        if index < length and source[index] == "'":
            # A lifetime (`'a`) is not a char literal.  Only consume a char
            # literal when a closing quote occurs on the same source line.
            end = index + 1
            escaped = False
            while end < length and source[end] != "\n":
                character = source[end]
                if character == "'" and not escaped:
                    break
                if character == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                end += 1
            if end < length and source[end] == "'":
                output.extend(source[index : end + 1])
                index = end + 1
                continue

        output.append(source[index])
        index += 1
    return "".join(output)


def _read(root: Path, relative: str, errors: list[str]) -> str:
    path = root / relative
    if path.is_file():
        return path.read_text(encoding="utf-8")
    if path.is_dir():
        return "\n".join(p.read_text(encoding="utf-8") for p in sorted(path.rglob("*.rs")))
    errors.append(f"missing required input: {relative}")
    return ""


def check(matrix_text: str, source_texts: dict[str, str]) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    clean_sources: dict[str, str] = {}
    for path, source in source_texts.items():
        try:
            clean_sources[path] = _strip_comments(source)
        except CommentSyntaxError as exc:
            errors.append(f"source comment parse failure in {path}: {exc}")
            clean_sources[path] = ""
    for label, actual_roles in rows(matrix_text):
        key = norm(label)
        if key not in APPLIED:
            errors.append(f"closed-world coverage failure: unmapped published row {label!r}")
            continue
        if key in seen:
            errors.append(f"duplicate published permission row: {label}")
            continue
        seen.add(key)
        expected = APPLIED[key]
        if actual_roles != expected.roles:
            for role, actual, applied in zip(ROLES, actual_roles, expected.roles):
                if actual != applied:
                    direction = "permissive" if actual == GRANTED and applied != GRANTED else "denial"
                    errors.append(f"permissive/denial drift ({direction}): {label} {role}: published {actual!r}, applied {applied!r}")
        code = "\n".join(clean_sources.get(path, "") for path in expected.sources)
        for marker in expected.required:
            if marker not in code:
                errors.append(f"applied gate missing: {label} requires {marker!r}")
        for path, markers in expected.required_by_source:
            source = clean_sources.get(path, "")
            for marker in markers:
                if marker not in source:
                    errors.append(f"applied gate missing: {label} requires {marker!r} in {path}")
        if expected.route and expected.route not in code:
            errors.append(f"served route missing: {label} requires {expected.route!r}")
        for marker in expected.absent:
            if marker in code:
                errors.append(f"unserved permission became served: {label} found {marker!r}")
    missing = set(APPLIED) - seen
    if missing:
        errors.append(f"closed-world coverage failure: checked {len(seen)} of {len(APPLIED)} mapped rows; missing {sorted(missing)}")
    return errors


def check_published_claim_documents(role_catalog: str, reference: str) -> list[str]:
    """Keep the two companion published RBAC pages in the same gate.

    The matrix is the row population; these documents carry the narrative and
    scope-level claims that caused B-153.  Requiring the concrete destructive
    claim in both documents prevents a stale companion page from becoming an
    unobserved, more permissive authorization description.
    """
    errors: list[str] = []
    if re.search(r"Developer\s+\*\*can\*\*\s+delete\s+CAS\s+blobs", role_catalog, re.I) is None:
        errors.append("role catalog is missing the published Developer CAS-delete claim")
    for marker in ("cache-WRITE", "cache:delete"):
        if marker not in role_catalog:
            errors.append(f"role catalog is missing the CAS-delete distinction: {marker}")
    for marker in ("### `cache:delete`", "DELETE /v1/cas/{tenant}/{hash}", "scope.can_write()"):
        if marker not in reference:
            errors.append(f"permissions reference is missing the CAS-delete claim: {marker!r}")
    if re.search(r"Holding\s+`cache:delete`\s+therefore\s+grants\s+nothing;\s+holding\s+`cas:rw`\s+already\s+permits\s+deletion\.", reference) is None:
        errors.append("permissions reference must state that cache:delete grants nothing while cas:rw permits deletion")
    return errors


def validate(repo_root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    matrix = _read(repo_root, str(MATRIX), errors)
    role_catalog = _read(repo_root, str(ROLE_CATALOG), errors)
    reference = _read(repo_root, str(REFERENCE), errors)
    source_paths = {path for spec in APPLIED.values() for path in spec.sources}
    sources = {path: _read(repo_root, path, errors) for path in source_paths}
    if errors:
        return errors
    return check(matrix, sources) + check_published_claim_documents(role_catalog, reference)


def self_test() -> int:
    """Mutation controls: every population row is checked, both directions fail."""
    matrix = "\n".join(
        f"| {label} | {' | '.join(roles)} |" for label, roles in (
            ("Read CAS blobs + AC", (GRANTED,) * 4),
            ("Write CAS blobs + AC", (GRANTED, GRANTED, GRANTED, DENIED)),
            ("FindMissingBlobs", (GRANTED, GRANTED, GRANTED, DENIED)),
            ("Delete a CAS blob / AC ref", (GRANTED, GRANTED, GRANTED, DENIED)),
            ("Invite user (Developer / Viewer)", (GRANTED, GRANTED, DENIED, DENIED)),
            ("Invite user (Admin)", (GRANTED, DENIED, DENIED, DENIED)),
            ("Invite user (Owner)", (DENIED,) * 4),
            ("Revoke user (soft-delete row)", (GRANTED, GRANTED, DENIED, DENIED)),
            ("List PATs in tenant", (GRANTED, GRANTED, GRANTED, DENIED)),
            ("Mint PAT", (GRANTED, GRANTED, GRANTED, DENIED)),
            ("Revoke a PAT in tenant", (GRANTED, GRANTED, GRANTED, DENIED)),
            ("Delete the whole account", (GRANTED, DENIED, DENIED, DENIED)),
            ("Read audit log of the tenant (dashboard)", (GRANTED, GRANTED, DENIED, DENIED)),
            ("Export audit log / analytics", (GRANTED,) * 4),
            ("RFC 6962 inclusion proof", (GRANTED,) * 4),
            ("Query `admin_op_log`", (ABSENT,) * 4),
            ("View billing detail", (GRANTED, GRANTED, DENIED, DENIED)),
            ("Manage payment method / cancel subscription", (GRANTED, GRANTED, DENIED, DENIED)),
            ("Select / change tier", (GRANTED, GRANTED, DENIED, DENIED)),
            ("Apply tax exemption", (ABSENT,) * 4),
            ("Report action started", (DENIED,) * 4),
            ("Report result", (DENIED,) * 4),
        )
    )
    all_markers = "\n".join(
        marker
        for spec in APPLIED.values()
        for marker in (*spec.required, *(tuple([spec.route]) if spec.route else ()))
    )
    sources = {path: all_markers for path in {p for spec in APPLIED.values() for p in spec.sources}}
    # Absent rows are represented by empty route directories in this fixture.
    mutated = matrix.replace("| ✅ | ❌ |\n| Report action", "| ✅ | ✅ |\n| Report action")
    if not check(mutated, sources):
        print("FAIL: permissive mutation was not detected")
        return 1
    print("mutation: permissive published role claim rejected")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    errors = validate(args.repo_root)
    if errors:
        print(f"PUBLISHED PERMISSION MATRIX INVALID: {len(errors)} failure(s)")
        for error in errors:
            print(f"- {error}")
        return 1
    print(f"OK: closed-world published permission matrix ({len(APPLIED)} rows), no role/gate drift")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
