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

# The customer-facing D1 team contract calls the cache-capable seat `member`.
# The legacy auth schema calls the equivalent role `Developer`; keep that
# spelling in diagnostics and the applied manifest so a stale role claim is
# visible rather than silently treated as a new grant.
ROLES = ("owner", "admin", "developer", "viewer")
PUBLISHED_ROLE_HEADERS = ("owner", "admin", "member", "viewer")
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
    # (source path, context start, context end, required markers).  Global
    # marker presence is insufficient when several handlers share one module:
    # a mutation can remove the revoke/delete gate while another route keeps
    # the same token alive.
    required_in_context: tuple[tuple[str, str, str, tuple[str, ...]], ...] = ()
    # (source path, context start, context end, predicate tokens, rejection
    # tokens, protected-effect tokens).  Presence in a module is not enough:
    # the predicate must be an executable branch that rejects before the
    # mutation it claims to protect.
    active_guards: tuple[
        tuple[str, str, str, tuple[str, ...], tuple[str, ...], tuple[str, ...], bool], ...
    ] = ()


class CommentSyntaxError(ValueError):
    """A Rust block comment was not terminated."""


@dataclass(frozen=True)
class RustToken:
    """One lexical Rust token, with comments and literals distinguished."""

    kind: str
    value: str


def _tokenize_rust(source: str) -> list[RustToken]:
    """Tokenize enough Rust to prove an authorization branch is executable.

    This is deliberately not a Rust parser.  It does, however, handle nested
    comments, normal/raw/byte strings, character literals, identifiers and
    punctuation.  Comment and literal text never becomes an identifier token,
    so a reviewer cannot satisfy a gate by planting ``"if can_write()"`` or a
    commented-out predicate.  Unterminated lexical states fail closed.
    """

    tokens: list[RustToken] = []
    index = 0
    length = len(source)
    punctuation = (
        "=>", "->", "::", "&&", "||", "==", "!=", "<=", ">=", "..", "+=", "-=",
    )
    while index < length:
        character = source[index]
        if character.isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = length if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            index += 2
            depth = 1
            while index < length and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise CommentSyntaxError("unterminated Rust block comment")
            continue

        # Raw strings (including byte raw strings) may contain any comment or
        # predicate-looking text. Keep one literal token and never expose its
        # contents to the control-flow matcher.
        raw = _raw_string_start(source, index)
        if raw is not None:
            quote, hash_count = raw
            closing = '"' + ("#" * hash_count)
            end = source.find(closing, quote + 1)
            if end < 0:
                raise CommentSyntaxError("unterminated Rust raw string")
            tokens.append(RustToken("string", source[index : end + len(closing)]))
            index = end + len(closing)
            continue

        # Ordinary and byte strings.
        string_start = character == '"' or (
            character == "b" and index + 1 < length and source[index + 1] == '"'
        )
        if string_start:
            start = index
            if character == "b":
                index += 1
            index += 1
            terminated = False
            while index < length:
                character = source[index]
                index += 1
                if character == "\\" and index < length:
                    index += 1
                elif character == '"':
                    terminated = True
                    break
            if not terminated:
                raise CommentSyntaxError("unterminated Rust string literal")
            tokens.append(RustToken("string", source[start:index]))
            continue

        # Character literals are opaque. A lifetime such as `'row` is not.
        if character == "'" and index + 1 < length and source[index + 1] != "\n":
            end = index + 1
            escaped = False
            while end < length and source[end] != "\n":
                candidate = source[end]
                if candidate == "'" and not escaped:
                    tokens.append(RustToken("char", source[index : end + 1]))
                    index = end + 1
                    break
                if candidate == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                end += 1
            else:
                # This is a lifetime/apostrophe, not a character literal.
                tokens.append(RustToken("punct", "'"))
                index += 1
            continue

        if character.isalpha() or character == "_":
            end = index + 1
            while end < length and (source[end].isalnum() or source[end] == "_"):
                end += 1
            tokens.append(RustToken("ident", source[index:end]))
            index = end
            continue
        if character.isdigit():
            end = index + 1
            while end < length and (source[end].isalnum() or source[end] in "_."):
                end += 1
            tokens.append(RustToken("number", source[index:end]))
            index = end
            continue
        operator = next((value for value in punctuation if source.startswith(value, index)), None)
        if operator is not None:
            tokens.append(RustToken("punct", operator))
            index += len(operator)
        else:
            tokens.append(RustToken("punct", character))
            index += 1
    return tokens


def _token_value(token: RustToken) -> str:
    return token.value


def _contains_tokens(tokens: list[RustToken], expected: tuple[str, ...]) -> bool:
    """Find a token sequence, ignoring comments but never string contents."""

    if not expected or len(tokens) < len(expected):
        return False
    values = [_token_value(token) for token in tokens]
    return any(tuple(values[index : index + len(expected)]) == expected for index in range(len(values) - len(expected) + 1))


def _matching_brace(tokens: list[RustToken], opening: int) -> int | None:
    depth = 0
    for index in range(opening, len(tokens)):
        if tokens[index].value == "{":
            depth += 1
        elif tokens[index].value == "}":
            depth -= 1
            if depth == 0:
                return index
    return None


def has_active_guard(
    source: str,
    context_start: str,
    context_end: str,
    predicate: tuple[str, ...],
    rejection: tuple[str, ...] = ("return",),
    protected_effect: tuple[str, ...] = (),
    require_positive: bool = False,
) -> bool:
    """Require a live ``if`` predicate to reject before its protected effect.

    The matcher intentionally rejects ``if false && predicate`` as dead and
    rejects a bare/unused predicate call.  ``protected_effect`` is searched
    only after the rejecting branch, tying the check to the mutation it is
    supposed to guard rather than to another route in the same module.
    """

    first = source.find(context_start)
    if first < 0:
        return False
    tail = source[first + len(context_start) :]
    last = tail.find(context_end)
    segment = source[first:] if last < 0 else tail[:last]
    try:
        tokens = _tokenize_rust(segment)
    except CommentSyntaxError:
        return False
    dead_ranges: list[tuple[int, int]] = []
    # Record statically unreachable blocks first, so a valid-looking nested
    # guard cannot hide inside an outer ``if false { ... }`` branch.
    for index, token in enumerate(tokens):
        if token.value != "if":
            continue
        condition_end = None
        parens = 0
        for cursor in range(index + 1, len(tokens)):
            value = tokens[cursor].value
            if value in ("(", "["):
                parens += 1
            elif value in (")", "]"):
                parens = max(0, parens - 1)
            elif value == "{" and parens == 0:
                condition_end = cursor
                break
        if condition_end is not None and any(
            item.kind == "ident" and item.value == "false"
            for item in tokens[index + 1 : condition_end]
        ):
            closing = _matching_brace(tokens, condition_end)
            if closing is not None:
                dead_ranges.append((condition_end, closing))
    for index, token in enumerate(tokens):
        if token.value != "if":
            continue
        if any(start < index < end for start, end in dead_ranges):
            continue
        condition_end = None
        parens = 0
        for cursor in range(index + 1, len(tokens)):
            value = tokens[cursor].value
            if value in ("(", "["):
                parens += 1
            elif value in (")", "]"):
                parens = max(0, parens - 1)
            elif value == "{" and parens == 0:
                condition_end = cursor
                break
        if condition_end is None:
            continue
        condition = tokens[index + 1 : condition_end]
        # A literal false anywhere in this condition makes the branch
        # statically unreachable, even when the required marker follows it.
        if any(item.kind == "ident" and item.value == "false" for item in condition):
            continue
        # These authorization predicates are intentionally canonical complete
        # conditions. Accepting a surrounding `|| true`, extra bypass term, or
        # a positive inversion would prove only marker presence, not the gate.
        if tuple(item.value for item in condition) != predicate:
            continue
        if require_positive:
            # The D1 owner safeguard is an admin equality check. A negated
            # comparator (`!= "admin"` / `!eq_ignore_ascii_case(...)`) must
            # never satisfy the positive owner-protection claim.
            predicate_start = next(
                (offset for offset in range(len(condition))
                 if tuple(item.value for item in condition[offset:offset + len(predicate)]) == predicate),
                None,
            )
            if predicate_start is None or predicate_start > 0 and condition[predicate_start - 1].value == "!":
                continue
        closing = _matching_brace(tokens, condition_end)
        if closing is None:
            continue
        branch = tokens[condition_end + 1 : closing]
        if not _contains_tokens(branch, rejection):
            continue
        if protected_effect and not _contains_tokens(tokens[closing + 1 :], protected_effect):
            continue
        return True
    return False


# The closed-world manifest is deliberately explicit.  Adding a row to the
# documentation without adding an applied record is an instrument failure.
APPLIED: dict[str, Applied] = {
    # CAS / AC
    "readcasblobsac": Applied((GRANTED,) * 4, ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"), ("can_read()",), "CAS_READ_ROUTE", required_by_source=(("crates/corelink-container/src/routes/cas.rs", ("can_read()",)), ("crates/corelink-container/src/routes/ac.rs", ("can_read()",)))),
    "writecasblobsac": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"), ("can_write()",), ".put(", required_by_source=(("crates/corelink-container/src/routes/cas.rs", ("can_write()",)), ("crates/corelink-container/src/routes/ac.rs", ("can_write()",)))),
    "findmissingblobs": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/bazel_v2.rs",), ("can_find_missing()",), "findMissingBlobs"),
    "deleteacasblobacref": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/cas.rs", "crates/corelink-container/src/routes/ac.rs"), ("can_write()",), ".delete(", required_by_source=(("crates/corelink-container/src/routes/cas.rs", ("can_write()",)), ("crates/corelink-container/src/routes/ac.rs", ("can_write()",))), required_in_context=(
        ("crates/corelink-container/src/routes/cas.rs", "async fn handle_delete(", "async fn handle_list(", ("scope.can_write()",)),
        ("crates/corelink-container/src/routes/ac.rs", "async fn handle_delete(", "async fn handle_list_refs(", ("scope.can_write()",)),
    ), active_guards=(
        ("crates/corelink-container/src/routes/cas.rs", "async fn handle_delete(", "async fn handle_list(", ("!", "scope", ".", "can_write", "(", ")"), ("return", "(", "StatusCode", "::", "FORBIDDEN"), (".", "delete", "("), False),
        ("crates/corelink-container/src/routes/ac.rs", "async fn handle_delete(", "async fn handle_list_refs(", ("!", "scope", ".", "can_write", "(", ")"), ("return", "(", "StatusCode", "::", "FORBIDDEN"), (".", "delete", "("), False),
    )),
    # Team membership and customer credentials
    "inviteusermemberviewer": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("caller_is_owner_or_admin", "role_is_privileged"), "/v1/customer/team/invite"),
    "inviteuseradmin": Applied((GRANTED, DENIED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("caller_is_owner_or_admin", "role_is_privileged"), "/v1/customer/team/invite"),
    "inviteuserowner": Applied((DENIED,) * 4, ("crates/corelink-container/src/routes/customer.rs",), ("owner role is not grantable",), "/v1/customer/team/invite"),
    "revokeusersoftdeleterow": Applied((GRANTED, GRANTED, DENIED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("caller_is_owner_or_admin",), "/v1/customer/team/"),
    "listpatsintenant": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_cache_write",), "/v1/customer/keys"),
    "mintpat": Applied((GRANTED, GRANTED, GRANTED, DENIED), ("crates/corelink-container/src/routes/customer.rs",), ("requires_cache_write",), "/v1/customer/keys"),
    "revokeapatintenant": Applied(
        (GRANTED, GRANTED, DENIED, DENIED),
        ("crates/corelink-container/src/routes/customer.rs", "crates/corelink-container/src/customer_d1.rs"),
        ("caller_is_owner_or_admin",),
        "/v1/customer/keys/",
        required_by_source=(("crates/corelink-container/src/routes/customer.rs", ("caller_is_owner_or_admin",)),),
        required_in_context=(
            ("crates/corelink-container/src/routes/customer.rs", "async fn handle_keys_revoke(", "async fn handle_team_list(", ("caller_is_owner_or_admin(&headers)",)),
        ),
        active_guards=(
        ("crates/corelink-container/src/routes/customer.rs", "async fn handle_keys_revoke(", "async fn handle_team_list(", ("!", "caller_is_owner_or_admin", "(", "&", "headers", ")"), ("return", "(", "StatusCode", "::", "FORBIDDEN"), ("state", ".", "keys", ".", "revoke", "("), False),
        ("crates/corelink-container/src/customer_d1.rs", "fn revoke(&self, req: KeyRevokeRequest)", "impl CustomerTeamHandler", ("req", ".", "caller_role", ".", "trim", "(", ")", ".", "eq_ignore_ascii_case", "(", '"admin"', ")", "&&", "col_opt_str", "(", "&", "row", ",", '"principal_id"', ")", ".", "is_none", "(", ")"), ("return", "Err", "(", "CustomerHandlerError", "::", "Unauthorized", "("), ("self", ".", "run", "("), True),
        ),
    ),
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


def _role_header(line: str) -> tuple[str, ...] | None:
    """Return the complete role suffix of a Permission table header.

    Looking only at the final four cells would let an added role column hide
    in the prefix/suffix.  We locate the Owner column and require the complete
    suffix to equal the closed D1 role population.
    """
    if not line.lstrip().startswith("|"):
        return None
    cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
    if not cells or cells[0].casefold() != "permission":
        return None
    try:
        owner_index = next(i for i, cell in enumerate(cells[1:], 1) if norm(cell) == "owner")
    except StopIteration:
        return ()
    return tuple(norm(cell) for cell in cells[owner_index:])


def rows(text: str):
    """Yield (label, four role cells) from canonical role tables.

    Malformed role headers intentionally yield no rows; ``check`` reports the
    header drift and the closed-world population failure together.
    """
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if _role_header(line) != PUBLISHED_ROLE_HEADERS:
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
        source = path.read_text(encoding="utf-8")
        # Route modules are intentionally split into include! parts.  Read the
        # assembled source while retaining the manifest's stable top-level
        # paths, so a route re-anchor cannot make its live predicate invisible.
        include_re = re.compile(r'include!\(\s*"([^"]+)"\s*\)')
        parts = [source]
        for include in include_re.findall(source):
            child = path.parent / include
            if child.is_file():
                parts.append(_read(root, str(child.relative_to(root)), errors))
            else:
                errors.append(f"missing required input: {child.relative_to(root)}")
        return "\n".join(parts)
    if path.is_dir():
        return "\n".join(p.read_text(encoding="utf-8") for p in sorted(path.rglob("*.rs")))
    errors.append(f"missing required input: {relative}")
    return ""


def check(matrix_text: str, source_texts: dict[str, str]) -> list[str]:
    errors: list[str] = []
    role_headers = [
        _role_header(line)
        for line in matrix_text.splitlines()
        if _role_header(line) is not None
    ]
    if not role_headers:
        errors.append(
            "role header drift: no Permission table has the required "
            "Owner | Admin | Member | Viewer suffix"
        )
    for header in role_headers:
        if header != PUBLISHED_ROLE_HEADERS:
            rendered = " | ".join(header) if header else "<missing Owner column>"
            errors.append(
                "role header drift: expected Owner | Admin | Member | Viewer; "
                f"found {rendered}"
            )
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
        for path, start, end, markers in expected.required_in_context:
            source = clean_sources.get(path, "")
            first = source.find(start)
            context = "" if first < 0 else source[first:]
            last = context.find(end, len(start)) if context else -1
            if last >= 0:
                context = context[:last]
            elif first < 0:
                context = ""
            for marker in markers:
                if marker not in context:
                    errors.append(
                        f"applied gate missing: {label} requires {marker!r} "
                        f"in {path} context {start!r}"
                    )
        for path, start, end, predicate, rejection, protected_effect, require_positive in expected.active_guards:
            source = clean_sources.get(path, "")
            if not has_active_guard(source, start, end, predicate, rejection, protected_effect, require_positive):
                errors.append(
                    f"applied gate is not load-bearing: {label} requires live "
                    f"predicate in {path} context {start!r}"
                )
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
    for role in ("Member", "Developer"):
        if re.search(rf"{role}\s+\*\*can\*\*\s+delete\s+CAS\s+blobs", role_catalog, re.I) is None:
            errors.append(f"role catalog is missing the published {role} CAS-delete claim")
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
    """Run mutations against the assembled, executable route sources."""
    baseline = validate(ROOT)
    if baseline:
        print("FAIL: baseline permission matrix is not green")
        return 1
    matrix = _read(ROOT, str(MATRIX), [])
    source_paths = {path for spec in APPLIED.values() for path in spec.sources}
    sources = {path: _read(ROOT, path, []) for path in source_paths}

    def mutate_context(path: str, start: str, end: str, old: str, new: str) -> dict[str, str]:
        mutated = dict(sources)
        text = mutated[path]
        first = text.index(start)
        last = text.index(end, first)
        context = text[first:last]
        if old not in context:
            raise ValueError(f"mutation marker missing in {path}: {old}")
        mutated[path] = text[:first] + context.replace(old, new, 1) + text[last:]
        return mutated

    mutations = (
        ("customer positive predicate", mutate_context(
            "crates/corelink-container/src/routes/customer.rs",
            "async fn handle_keys_revoke(", "async fn handle_team_list(",
            "if !caller_is_owner_or_admin(&headers)", "if caller_is_owner_or_admin(&headers)",
        )),
        ("customer string bait", mutate_context(
            "crates/corelink-container/src/routes/customer.rs",
            "async fn handle_keys_revoke(", "async fn handle_team_list(",
            "caller_is_owner_or_admin(&headers)", '"caller_is_owner_or_admin(&headers)"',
        )),
        ("customer unused predicate", mutate_context(
            "crates/corelink-container/src/routes/customer.rs",
            "async fn handle_keys_revoke(", "async fn handle_team_list(",
            "if !caller_is_owner_or_admin(&headers)",
            "let _owner_admin = caller_is_owner_or_admin(&headers); if true",
        )),
        ("CAS delete scope gate", mutate_context(
            "crates/corelink-container/src/routes/cas.rs",
            "async fn handle_delete(", "async fn handle_list(",
            "scope.can_write()", "scope.can_read()",
        )),
        ("PAT revoke false gate", mutate_context(
            "crates/corelink-container/src/routes/customer.rs",
            "async fn handle_keys_revoke(", "async fn handle_team_list(",
            "if !caller_is_owner_or_admin(&headers)", "if false",
        )),
        ("D1 owner guard false", mutate_context(
            "crates/corelink-container/src/customer_d1.rs",
            "fn revoke(&self, req: KeyRevokeRequest)", "impl CustomerTeamHandler",
            "if req.caller_role.trim().eq_ignore_ascii_case(\"admin\")",
            "if false && req.caller_role.trim().eq_ignore_ascii_case(\"admin\")",
        )),
        ("D1 owner predicate", mutate_context(
            "crates/corelink-container/src/customer_d1.rs",
            "fn revoke(&self, req: KeyRevokeRequest)", "impl CustomerTeamHandler",
            'if req.caller_role.trim().eq_ignore_ascii_case("admin")',
            'if !req.caller_role.trim().eq_ignore_ascii_case("admin")',
        )),
    )
    for name, mutated in mutations:
        if not check(matrix, mutated):
            print(f"FAIL: {name} mutation escaped")
            return 1
        print(f"mutation: {name} rejected")
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
