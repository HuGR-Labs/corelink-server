#!/usr/bin/env python3
"""Compare the DOCUMENTED API surface against the SERVED one (B-121).

The documented surface is `openapi/corelink-v1.yaml` — the hand-written spec
that `scripts/gen-api-reference.py` turns into the 45 MDX pages under
`apps/docs/docs/reference/api/endpoints/`.  The pages are propagation, not
source: regenerating them fixes nothing.  The spec is what drifts.

The served surface is `crates/`, `worker/src`, and the Worker entrypoints under
`apps/**`.  Three registration sites:

  * axum `.route(<path-expr>, ...)` in `crates/` — the path expression may be
    a string literal, may sit on the line AFTER `.route(`, and is frequently a
    `const` (`AUDIT_EXPORT_ROUTE`, `TURBO_GET_ROUTE`, `ROUTE_EVENT_COUNT`, …);
  * exact path comparisons in the Worker's `matchRoute` table
    (`worker/src/index.ts`) — `path === "/api/health"` and friends, which are
    served AT THE EDGE and never reach the container.

  * exact `url.pathname === "/..."` dispatch in sibling Workers under
    `apps/**` (the source file is retained for an actionable finding).

The app scan is intentionally structural and closed-world: it examines every
non-test TypeScript/TSX file under `apps/`, rather than an allowlist of today's
Worker names. Static exact dispatch is extracted, while a lexical census
consumes every live `.pathname` token not handled by a supported dispatch form
or an exact non-dispatch allowlist entry. A newly added Worker route therefore
cannot be hidden by forgetting to add its directory, changing its dispatch
spelling, or using an operator the matcher does not know. Non-API app paths
remain outside the OpenAPI contract through the same explicit public-path
policy used for Rust routes below. `/v1/event` is currently served by
`apps/analytics-worker/src/index.ts` and is absent from the spec; the ledger
keeps that finding visible until the contract owner documents or excludes it.

A line-oriented `grep '\\.route("'` sees only the first of those three forms.
That instrument reported 32 phantom absences once already; the extractor here
is deliberately expression-oriented, and `--self-test` proves it can still see
one known instance of each form before any absence is believed.

Two directions, one instrument:

  MISSING_ROUTE  a documented path that nothing serves   (B-116/119/120/121)
  MISSING_DOC    a served public path that nothing documents          (B-117)

Known divergences are carried in the ledger below, each pinned to its backlog
id.  The gate fails on anything NOT in the ledger, and equally on a ledger
entry that no longer diverges — so a fix cannot land while leaving its excuse
behind.  `--strict` ignores the ledger entirely and reports the raw truth;
that is the mode that must be seen accusing the eight known divergences.

stdlib only, on purpose: this runs in a `pull_request` lane with no install
step.
"""

from __future__ import annotations

import argparse
import re
import tempfile
import sys
from collections.abc import Iterator
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

OPENAPI = REPO / "openapi" / "corelink-v1.yaml"
CRATES = REPO / "crates"
WORKER = REPO / "worker" / "src"
APPS = REPO / "apps"
APP_TYPESCRIPT_SUFFIXES = {".ts", ".tsx"}

HTTP_METHODS = {
    "get",
    "put",
    "post",
    "delete",
    "options",
    "head",
    "patch",
    "trace",
}

# --------------------------------------------------------------------------
# Ledger of DECLARED divergences.
#
# Every entry is a divergence that is KNOWN, TRACKED, and not yet fixed.  An
# entry must name the backlog item that owns the fix.  Entries are exact:
# adding one silences exactly one path, and an entry whose divergence has been
# repaired makes the gate FAIL (STALE_LEDGER) so the excuse is removed with
# the fix.
# --------------------------------------------------------------------------
LEDGER_MISSING_ROUTE: dict[str, str] = {
    # --- ABSENCE: the spec promises an endpoint nothing registers ----------
    "/v1/admin/ops": "B-119",
    "/v1/admin/ops/{op_id}": "B-119",
    "/v1/admin/ops/{op_id}/approve": "B-119",
    "/v1/admin/ops/{op_id}/reject": "B-119",
    "/v1/enterprise/inquire": "B-120",
    "/v1/admin/audit/events": "B-121",
    "/v1/admin/tenants": "B-121 (only the /{tenant_id}/... sub-routes exist)",
    "/v1/data-categories": "B-121 (only admin-ui/src/lib/dsr-client.ts calls it)",
    # --- PATH DIVERGENCE: the endpoint exists under a different path -------
    # Worse for the caller than absence: the doc looks right and the call
    # 404s AFTER authenticating.  Each of these has a MISSING_DOC twin below.
    "/v1/dpa/accept": "B-116 (served at /v1/onboarding/dpa-accept; Clerk, not PAT)",
    "/v1/dpa/re-accept": "B-116 (same family; no route anywhere — only a proptest names it)",
    "/v1/audit/export": "B-121 (served at /v1/audit/{tenant}/export — the tenant segment is missing)",
    # --- WRONG SERVICE ------------------------------------------------------
    "/api/csp-report": "B-121 (served by apps/admin-ui, not the API; it does not belong in this spec)",
}

LEDGER_MISSING_DOC: dict[str, str] = {
    # --- the two B-117 named -----------------------------------------------
    "/v1/customer/account/delete": "B-117",
    "/v1/customer/account/export": "B-117",
    # --- the rest of the same undocumented customer portal ------------------
    "/v1/customer/audit": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/billing": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/billing/portal": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/keys": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/keys/{pat_id}/revoke": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/overview": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/runners/allowlist": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/runners/entitlement": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/runners/runs": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/team": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/team/invite": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/team/{user_id}": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/usage": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/workspaces": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/workspaces/{workspace_id}": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/workspaces/{workspace_id}/pin": "B-117 (same customer-portal family; never documented)",
    # --- twins of the path divergences above --------------------------------
    "/v1/onboarding/dpa-accept": "B-116 (the real path behind the documented /v1/dpa/accept)",
    "/v1/audit/{tenant}/export": "B-121 (the real path behind the documented /v1/audit/export)",
    # --- admin surface served, absent from the spec -------------------------
    "/v1/admin/approve": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/byok/activate": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/byok/deactivate": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/tenants/{tenant_id}/billing": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/tenants/{tenant_id}/consents": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/tenants/{tenant_id}/dsr": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/tenants/{tenant_id}/pats": "B-121 (admin surface served, absent from the spec)",
    "/v1/admin/tenants/{tenant_id}/usage": "B-121 (admin surface served, absent from the spec)",
    # --- served shapes the spec does not cover ------------------------------
    "/v1/ac/{tenant}": "B-121 (documented only as /v1/ac/{tenant}/{action_digest})",
    "/v1/privacy/dsr": "B-121 (outside the documented /v1/privacy/dsr/{action} shape)",
    "/v1/privacy/dsr/{request_id}/verify-mfa": "B-121 (outside the documented /v1/privacy/dsr/{action} shape)",
    "/v1/public/attestation/{request_id}": "B-121 (public attestation surface, never documented)",
    "/v1/public/keys/erasure/{region_pub}": "B-121 (public attestation surface, never documented)",
    # --- app Worker surface (B-130) ----------------------------------------
    "/v1/event": "B-130 (apps/analytics-worker public ingest route)",
    "/v1/digest/preview": "B-130 (apps/analytics-worker dev-only preview; contract owner must decide)",
}

# --------------------------------------------------------------------------
# Which served paths are EXPECTED to be documented.
#
# The public product surface is what a customer can call with a PAT or a Clerk
# session.  Internal, machine-to-machine and test-scaffold routes are out of
# scope for the customer-facing OpenAPI by design, and are excluded here by
# PREFIX rather than one by one — a prefix cannot silently swallow a new public
# route the way a per-path allowlist can.
# --------------------------------------------------------------------------
NON_PUBLIC_PREFIXES = (
    "/_internal/",
    "/internal/",
    "/_health",
    "/health",
    "/metrics",
    "/__",
)

# Registry-protocol surfaces speak their own upstream wire protocols (OCI
# distribution-spec, npm, PyPI simple, Homebrew, cargo sparse, WebDAV). They are
# documented as protocol surfaces in the docs site, not as per-path OpenAPI
# operations, so they are out of the OpenAPI contract by construction.
PROTOCOL_PREFIXES = (
    "/v2/",
    "/v2",
    "/token",
    "/npm/",
    "/pip/",
    "/brew/",
    "/cargo/",
)


# ==========================================================================
# Documented surface
# ==========================================================================
def parse_openapi_paths(text: str) -> dict[str, set[str]]:
    """Return {path: {METHOD, …}} from the `paths:` block.

    A hand-rolled scanner rather than a YAML dependency: the lane has no
    install step.  It keys off indentation, which is why it also asserts that
    it found a `paths:` block at all instead of quietly returning {}.
    """
    lines = text.splitlines()
    start = None
    for i, line in enumerate(lines):
        if line.rstrip() == "paths:":
            start = i + 1
            break
    if start is None:
        raise SystemExit(
            "FATAL: no top-level `paths:` block in openapi/corelink-v1.yaml — "
            "the extractor cannot see the documented surface, which is NOT the "
            "same as the surface being empty."
        )

    out: dict[str, set[str]] = {}
    current: str | None = None
    for line in lines[start:]:
        if line.strip() == "" or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        if indent == 0:
            break  # left the paths: block
        stripped = line.strip()
        if indent == 2 and stripped.endswith(":"):
            key = stripped[:-1].strip().strip("'\"")
            if key.startswith("/"):
                current = key
                out.setdefault(current, set())
            else:
                current = None
        elif indent == 4 and current is not None and stripped.endswith(":"):
            verb = stripped[:-1].strip().lower()
            if verb in HTTP_METHODS:
                out[current].add(verb.upper())
    return out


# ==========================================================================
# Served surface — Rust
# ==========================================================================
STR_LIT = re.compile(r'"((?:[^"\\]|\\.)*)"')
CONST_DECL = re.compile(
    r'const\s+([A-Z][A-Z0-9_]*)\s*:\s*&\s*(?:\'static\s+)?str\s*=\s*"([^"]*)"'
)
ROUTE_CALL = re.compile(r"\.route\s*\(")


class SourceSyntaxError(ValueError):
    """A source comment or literal is incomplete for static inspection."""


def _raw_string_start(source: str, index: int) -> tuple[int, int] | None:
    """Return (opening quote, hash count) for a Rust raw string."""
    if source.startswith("br", index):
        prefix_end = index + 2
    elif source.startswith("r", index):
        prefix_end = index + 1
    else:
        return None
    hashes = 0
    while prefix_end + hashes < len(source) and source[prefix_end + hashes] == "#":
        hashes += 1
    quote = prefix_end + hashes
    if quote >= len(source) or source[quote] != '"':
        return None
    return quote, hashes


def _blank_comment(value: str) -> str:
    return "".join("\n" if character == "\n" else " " for character in value)


def strip_source_comments(src: str, *, nested: bool = False) -> str:
    """Blank Rust/Worker comments without disturbing literals or offsets.

    Rust permits nested block comments; TypeScript/JavaScript block comments do
    not, so callers select the language's syntax with ``nested``. Unterminated
    comments and strings raise rather than silently shrinking the evidence set.
    """
    out: list[str] = []
    index = 0
    length = len(src)
    while index < length:
        if src.startswith("//", index):
            out.extend((" ", " "))
            index += 2
            while index < length and src[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if src.startswith("/*", index):
            out.extend((" ", " "))
            index += 2
            depth = 1
            while index < length and depth:
                if nested and src.startswith("/*", index):
                    out.extend((" ", " "))
                    index += 2
                    depth += 1
                elif src.startswith("*/", index):
                    out.extend((" ", " "))
                    index += 2
                    depth -= 1
                elif src[index] == "\n":
                    out.append("\n")
                    index += 1
                else:
                    end = index
                    while end < length and src[end] not in "/*\n":
                        end += 1
                    if end == index:
                        out.append(" ")
                        index += 1
                    else:
                        out.append(_blank_comment(src[index:end]))
                        index = end
            if depth:
                raise SourceSyntaxError("unterminated block comment")
            continue

        if src[index] in {"r", "b"}:
            raw = _raw_string_start(src, index)
            if raw is not None:
                quote, hashes = raw
                closing = '"' + ("#" * hashes)
                end = src.find(closing, quote + 1)
                if end < 0:
                    raise SourceSyntaxError("unterminated raw string")
                out.append(src[index : end + len(closing)])
                index = end + len(closing)
                continue

        quote = src[index]
        if quote == "b" and index + 1 < length and src[index + 1] in {'"', "'"}:
            out.append(quote)
            index += 1
            quote = src[index]
        if quote in {'"', "'", "`"}:
            # A Rust lifetime (`'a`) is not a character literal. Only consume
            # an apostrophe when a closing quote exists on this source line.
            if quote == "'":
                end = index + 1
                escaped = False
                while end < length and src[end] != "\n":
                    character = src[end]
                    if character == "'" and not escaped:
                        break
                    if character == "\\" and not escaped:
                        escaped = True
                    else:
                        escaped = False
                    end += 1
                if end >= length or src[end] != "'":
                    out.append(quote)
                    index += 1
                    continue
                out.append(src[index : end + 1])
                index = end + 1
                continue
            out.append(quote)
            index += 1
            terminated = False
            while index < length:
                character = src[index]
                out.append(character)
                index += 1
                if character == "\\" and index < length:
                    out.append(src[index])
                    index += 1
                elif character == quote:
                    terminated = True
                    break
            if not terminated:
                raise SourceSyntaxError("unterminated string literal")
            continue

        out.append(src[index])
        index += 1
    return "".join(out)


def strip_line_comments(src: str) -> str:
    """Compatibility wrapper for callers that need Rust lexical stripping."""
    return strip_source_comments(src, nested=True)


def first_argument(src: str, open_paren: int) -> str:
    """Text of the first argument of the call whose '(' is at `open_paren`.

    Balanced over parens/brackets/braces and string-aware, so a path on the
    line AFTER `.route(` is read exactly like one on the same line.
    """
    depth = 0
    i = open_paren
    n = len(src)
    start = open_paren + 1
    in_str = False
    while i < n:
        c = src[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue
        if c == '"':
            in_str = True
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return src[start:i]
        elif c == "," and depth == 1:
            return src[start:i]
        i += 1
    return ""


def collect_rust_consts() -> dict[str, str]:
    consts: dict[str, str] = {}
    for path in CRATES.rglob("*.rs"):
        try:
            src = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        src = strip_source_comments(src, nested=True)
        for name, value in CONST_DECL.findall(src):
            if value.startswith("/"):
                consts.setdefault(name, value)
    return consts


def is_test_path(path: Path) -> bool:
    # Path parts are case-sensitive on some runners and not on others.  Keep
    # the production census stable, and exclude both singular and plural
    # fixture directories (including Playwright's usual capitalisation).
    parts = {part.casefold() for part in path.parts}
    name = path.name.casefold()
    return (
        "tests" in parts
        or "test" in parts
        or "spec" in parts
        or "benches" in parts
        or "__tests__" in parts
        or "playwright" in parts
        or "e2e" in parts
        or name.startswith("test_")
        or name.startswith("tests")
        or name.endswith((".test.ts", ".spec.ts", ".test.tsx", ".spec.tsx"))
        or "_tests" in Path(name).stem
    )


def iter_app_typescript() -> Iterator[Path]:
    """Yield every TypeScript source file in ``apps/``, including TSX."""
    if not APPS.is_dir():
        return
    for file in sorted(APPS.rglob("*")):
        if file.is_file() and file.suffix.casefold() in APP_TYPESCRIPT_SUFFIXES:
            yield file


def collect_rust_routes() -> dict[str, set[str]]:
    """{path: {source "file:line", …}} for every axum route registration."""
    consts = collect_rust_consts()
    routes: dict[str, set[str]] = {}
    for file in sorted(CRATES.rglob("*.rs")):
        if is_test_path(file):
            continue
        try:
            raw = file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if ".route" not in raw:
            continue
        src = strip_source_comments(raw, nested=True)
        for m in ROUTE_CALL.finditer(src):
            open_paren = src.index("(", m.start())
            arg = first_argument(src, open_paren).strip()
            line = src.count("\n", 0, m.start()) + 1
            where = f"{file.relative_to(REPO)}:{line}"
            value = None
            lit = STR_LIT.fullmatch(arg)
            if lit:
                value = lit.group(1)
            elif re.fullmatch(r"[A-Za-z_][A-Za-z0-9_:]*", arg):
                value = consts.get(arg.rsplit("::", 1)[-1])
            elif arg.startswith('"'):
                lit = STR_LIT.match(arg)
                if lit and "+" not in arg and "format!" not in arg:
                    value = lit.group(1)
            if value and value.startswith("/"):
                routes.setdefault(value, set()).add(where)
    return routes


# ==========================================================================
# Served surface — Worker (edge-terminated exact paths only)
# ==========================================================================
WORKER_EXACT = re.compile(r'path\s*===\s*"(/[^"]*)"')


def collect_worker_routes() -> dict[str, set[str]]:
    """Paths the Worker answers ITSELF, via exact comparison in matchRoute.

    Only `===` comparisons count.  `path.startsWith(...)` arms are FORWARDERS:
    treating a prefix as coverage would make `/v1/` vouch for every `/v1/*`
    path in the spec and hide precisely the absences this gate exists to find.
    """
    routes: dict[str, set[str]] = {}
    if not WORKER.is_dir():
        return routes
    for file in sorted(WORKER.rglob("*.ts")):
        if file.name.endswith(".test.ts") or is_test_path(file):
            continue
        src = strip_source_comments(
            file.read_text(encoding="utf-8", errors="replace"), nested=False
        )
        for m in WORKER_EXACT.finditer(src):
            value = m.group(1).rstrip("/") or "/"
            line = src.count("\n", 0, m.start()) + 1
            routes.setdefault(value, set()).add(f"{file.relative_to(REPO)}:{line}")
    return routes


# ============================================================================
# Served surface — sibling Workers under apps/
# ============================================================================
APP_IDENT = r"[A-Za-z_$][A-Za-z0-9_$]*"
APP_TERM = rf"{APP_IDENT}(?:\s*\.\s*{APP_IDENT})*"
APP_QUOTED = r'''(?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)'''
APP_COMPARE = re.compile(
    rf"(?P<left>{APP_TERM})\s*(?P<op>===|==)\s*"
    rf"(?P<right>{APP_QUOTED}|{APP_IDENT})"
)
APP_COMPARE_REVERSED = re.compile(
    rf"(?P<left>{APP_QUOTED})\s*(?P<op>===|==)\s*"
    rf"(?P<right>{APP_TERM})"
)
APP_COMPARE_REVERSED_TERM = re.compile(
    rf"(?P<left>(?!pathname\b){APP_IDENT})\s*(?P<op>===|==)\s*"
    rf"(?P<right>{APP_TERM})"
)
APP_STARTS_WITH = re.compile(
    rf"(?P<term>{APP_TERM})\s*\.\s*startsWith\s*\(\s*"
    rf"(?P<value>{APP_QUOTED}|{APP_IDENT})"
)
APP_SWITCH = re.compile(
    rf"switch\s*\(\s*(?P<term>{APP_TERM})\s*\)\s*\{{(?P<body>.*?)\}}",
    re.DOTALL,
)
APP_CASE = re.compile(rf"\bcase\s+(?P<value>{APP_QUOTED}|{APP_IDENT})\s*:")
APP_CONST = re.compile(
    rf"\b(?:const|let)\s+(?P<name>{APP_IDENT})\s*=\s*(?P<value>{APP_QUOTED})"
)
APP_ALIAS = re.compile(
    rf"\b(?:const|let)\s+(?P<name>{APP_IDENT})\s*=\s*"
    rf"(?P<source>{APP_TERM}\s*\.\s*pathname)\b"
)


def app_comparisons(src: str):
    """Yield forward and reversed static/path comparisons in source order."""
    matches = list(APP_COMPARE.finditer(src))
    matches.extend(APP_COMPARE_REVERSED.finditer(src))
    matches.extend(APP_COMPARE_REVERSED_TERM.finditer(src))
    yield from sorted(matches, key=lambda match: match.start())


def collect_app_routes() -> dict[str, set[str]]:
    """Paths selected by static exact pathname dispatch in app TypeScript.

    App Workers do not use axum's route table. Their public entrypoints route
    on ``url.pathname`` instead, so limiting the walk to ``crates/`` silently
    loses an entire deployed API. Test/fixture source is excluded because it
    models requests rather than serving them; all production app TypeScript is
    intentionally in the walk.
    """
    routes: dict[str, set[str]] = {}
    if not APPS.is_dir():
        return routes
    for file in iter_app_typescript():
        if is_test_path(file):
            continue
        try:
            raw = file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "pathname" not in raw:
            continue
        src = strip_ts_comments(raw)
        constants = app_string_constants(src)
        aliases = app_path_aliases(src)
        for match in app_comparisons(src):
            left, right = match.group("left"), match.group("right")
            if not (is_app_path_term(left, aliases) or is_app_path_term(right, aliases)):
                continue
            value = app_static_value(right, constants)
            if value is None:
                value = app_static_value(left, constants)
            if value is None or not value.startswith("/"):
                continue
            value = value.rstrip("/") or "/"
            line = src.count("\n", 0, match.start()) + 1
            routes.setdefault(value, set()).add(f"{file.relative_to(REPO)}:{line}")
    return routes


def strip_ts_comments(src: str) -> str:
    """Blank JS/TS comments without shifting source offsets.

    Templates need a little more than ordinary string handling: comments are
    live only inside a ``${...}`` expression, while the rest of the template
    is literal text. Every lexical state must terminate before this helper
    returns; otherwise a truncated fixture could hide a live ``.pathname``
    token from the parity census.
    """
    out: list[str] = []
    n = len(src)

    def looks_like_regex_start(i: int) -> bool:
        previous = i - 1
        while previous >= 0 and src[previous].isspace():
            previous -= 1
        if previous < 0 or src[previous] in "=([{,:;!&|?":
            return True
        end = previous + 1
        while previous >= 0 and (src[previous].isalnum() or src[previous] in "_$"):
            previous -= 1
        return src[previous + 1 : end] in {
            "case",
            "delete",
            "do",
            "else",
            "in",
            "instanceof",
            "of",
            "return",
            "throw",
            "typeof",
            "void",
            "yield",
        }

    def copy_regex(i: int) -> int:
        out.append("/")
        i += 1
        in_class = False
        while i < n:
            character = src[i]
            out.append(character)
            i += 1
            if character == "\\":
                if i < n:
                    out.append(src[i])
                    i += 1
                continue
            if character == "[":
                in_class = True
            elif character == "]":
                in_class = False
            elif character == "/" and not in_class:
                while i < n and (src[i].isalnum() or src[i] in "_$"):
                    out.append(src[i])
                    i += 1
                return i
            elif character == "\n":
                raise SourceSyntaxError("unterminated regex literal")
        raise SourceSyntaxError("unterminated regex literal")

    def copy_quoted(i: int, quote: str) -> int:
        out.append(src[i])
        i += 1
        while i < n:
            character = src[i]
            out.append(character)
            i += 1
            if character == "\\":
                if i < n:
                    out.append(src[i])
                    i += 1
            elif character == quote:
                return i
        raise SourceSyntaxError("unterminated quoted string literal")

    def blank_block_comment(i: int) -> int:
        end = src.find("*/", i + 2)
        if end < 0:
            raise SourceSyntaxError("unterminated block comment")
        end += 2
        out.extend("\n" if character == "\n" else " " for character in src[i:end])
        return end

    def copy_template(i: int) -> int:
        out.append("`")
        i += 1
        while i < n:
            character = src[i]
            if character == "\\":
                out.append(character)
                i += 1
                if i < n:
                    out.append(src[i])
                    i += 1
                continue
            if character == "`":
                out.append(character)
                return i + 1
            if src.startswith("${", i):
                out.extend(("$", "{"))
                i = copy_template_expression(i + 2)
                continue
            out.append(character)
            i += 1
        raise SourceSyntaxError("unterminated template literal")

    def copy_template_expression(i: int) -> int:
        depth = 0
        while i < n:
            character = src[i]
            if src.startswith("//", i):
                end = src.find("\n", i)
                end = n if end < 0 else end
                out.extend(" " for _ in range(end - i))
                i = end
                continue
            if src.startswith("/*", i):
                i = blank_block_comment(i)
                continue
            if character in "\"'":
                i = copy_quoted(i, character)
                continue
            if character == "`":
                i = copy_template(i)
                continue
            if character == "{":
                out.append(character)
                depth += 1
                i += 1
                continue
            if character == "}":
                out.append(character)
                if depth == 0:
                    return i + 1
                depth -= 1
                i += 1
                continue
            out.append(character)
            i += 1
        raise SourceSyntaxError("unterminated template interpolation")

    i = 0
    while i < n:
        if src.startswith("//", i):
            end = src.find("\n", i)
            end = n if end < 0 else end
            out.extend(" " for _ in range(end - i))
            i = end
            continue
        if src.startswith("/*", i):
            i = blank_block_comment(i)
            continue
        if src[i] == "/" and looks_like_regex_start(i):
            i = copy_regex(i)
            continue
        if src[i] in "\"'":
            i = copy_quoted(i, src[i])
            continue
        if src[i] == "`":
            i = copy_template(i)
            continue
        out.append(src[i])
        i += 1
    return "".join(out)


def app_string_constants(src: str) -> dict[str, str]:
    constants: dict[str, str] = {}
    for match in APP_CONST.finditer(src):
        value = app_unquote(match.group("value"))
        if value is not None:
            constants[match.group("name")] = value
    return constants


def app_unquote(token: str) -> str | None:
    if len(token) < 2 or token[0] not in "\"'`" or token[-1] != token[0]:
        return None
    value = token[1:-1]
    if token[0] == "`" and "${" in value:
        return None
    if "\\" in value:
        # Route literals in this repo are plain ASCII. Refuse escaped strings
        # rather than turning a partially decoded value into false coverage.
        return None
    return value


def app_static_value(token: str, constants: dict[str, str]) -> str | None:
    if token.startswith(("\"", "'", "`")):
        plain = app_unquote(token)
        if plain is not None:
            return plain
        if token.startswith("`") and token.endswith("`"):
            value = token[1:-1]
            names = re.findall(r"\$\{(" + APP_IDENT + r")\}", value)
            if names and all(name in constants for name in names):
                return re.sub(
                    r"\$\{(" + APP_IDENT + r")\}",
                    lambda match: constants[match.group(1)],
                    value,
                )
        return None
    return constants.get(token)


def app_path_aliases(src: str) -> set[str]:
    """Names directly assigned from a URL-like object's pathname.

    Keep aliases useful for Worker code (`const path = url.pathname`) while
    avoiding generic framework locals such as `req.nextUrl.pathname`, which
    are page/middleware matching rather than an app Worker dispatch surface.
    The distinction is based on expression shape, never file or entrypoint
    names.
    """
    aliases: set[str] = set()
    for match in APP_ALIAS.finditer(src):
        source = match.group("source").replace(" ", "")
        if source.count(".") == 1 or source.endswith(".url.pathname"):
            aliases.add(match.group("name"))
    return aliases


def is_app_path_term(term: str, aliases: set[str]) -> bool:
    term = term.replace(" ", "")
    # A bare local named `pathname` is common in Next.js middleware and page
    # helpers, but is not evidence of a Worker request dispatcher.  Require a
    # property access (`url.pathname`, `requestUrl.pathname`, …) or an alias
    # explicitly derived from one.  This is syntax-based, not an app-name or
    # Wrangler-entrypoint allowlist, so every actual Worker dispatch remains
    # covered by the fail-closed census.
    return term.endswith(".pathname") or term in aliases


def _skip_quoted(src: str, start: int, quote: str) -> int:
    """Return the first offset after a JS string literal."""
    i = start + 1
    while i < len(src):
        if src[i] == "\\":
            i += 2
        elif src[i] == quote:
            return i + 1
        else:
            i += 1
    return len(src)


def _live_pathname_tokens(src: str, *, validated: bool = False) -> list[tuple[int, str]]:
    """Find live ``.pathname`` tokens, including template interpolations.

    A regex over source text cannot distinguish a route token from a comment or
    string fixture.  This small lexer only needs JS lexical boundaries: normal
    strings are skipped, and a template's ``${...}`` expression is scanned as
    code while its literal portions remain inert.  Offsets are preserved so a
    dispatch matcher can consume the exact token it recognized.
    """
    # Reuse the comment/literal lexer as a syntax precondition. The scanner
    # below works on the original source to preserve exact token offsets.
    if not validated:
        strip_ts_comments(src)
    tokens: list[tuple[int, str]] = []
    n = len(src)

    def scan_template(i: int) -> int:
        while i < n:
            if src[i] == "\\":
                i += 2
            elif src[i] == "`":
                return i + 1
            elif src.startswith("${", i):
                i = scan_code(i + 2, stop_at_closing_brace=True)
            else:
                i += 1
        return n

    def scan_code(i: int, stop_at_closing_brace: bool = False) -> int:
        brace_depth = 0
        while i < n:
            if src.startswith("//", i):
                newline = src.find("\n", i + 2)
                i = n if newline == -1 else newline + 1
                continue
            if src.startswith("/*", i):
                end = src.find("*/", i + 2)
                i = n if end == -1 else end + 2
                continue
            if src[i] in "\"'":
                i = _skip_quoted(src, i, src[i])
                continue
            if src[i] == "`":
                i = scan_template(i + 1)
                continue
            if src[i] == "{":
                brace_depth += 1
                i += 1
                continue
            if src[i] == "}":
                if stop_at_closing_brace and brace_depth == 0:
                    return i + 1
                brace_depth = max(0, brace_depth - 1)
                i += 1
                continue
            if src.startswith(".pathname", i):
                end = i + len(".pathname")
                if end == n or not (src[end].isalnum() or src[end] in "_$"):
                    tokens.append((i, normalize_path_expression(src, i)))
                i = end
                continue
            i += 1
        return n

    scan_code(0)
    return tokens


def _identifier_start(src: str, end: int) -> int | None:
    end = end
    while end > 0 and src[end - 1].isspace():
        end -= 1
    i = end
    while i > 0 and (src[i - 1].isalnum() or src[i - 1] in "_$"):
        i -= 1
    return i if i < end else None


def _member_chain_start(src: str, end: int) -> int | None:
    """Return the start of an identifier/member chain ending before ``end``."""
    end = end
    start = _identifier_start(src, end)
    if start is None:
        return None
    while True:
        before = start
        while before > 0 and src[before - 1].isspace():
            before -= 1
        if before == 0 or src[before - 1] != ".":
            return start
        previous = _identifier_start(src, before - 1)
        if previous is None:
            return start
        start = previous


def normalize_path_expression(src: str, dot_offset: int) -> str:
    """Canonicalize the receiver expression immediately before ``.pathname``."""
    end = dot_offset
    while end > 0 and src[end - 1].isspace():
        end -= 1
    if end > 0 and src[end - 1] == ")":
        # Calls such as ``new URL(...).pathname`` are non-dispatch allowlist
        # entries.  Keep the whole immediate call, not an outer open call.
        # Find the opening paren by balancing from the nearest expression
        # boundary; this is intentionally lexical and never interprets route
        # operators.
        depth = 0
        i = end - 1
        while i >= 0:
            if src[i] == ")":
                depth += 1
            elif src[i] == "(":
                depth -= 1
                if depth == 0:
                    callee_start = _member_chain_start(src, i)
                    if callee_start is not None:
                        before = callee_start
                        while before > 0 and src[before - 1].isspace():
                            before -= 1
                        if (
                            before >= 3
                            and src[before - 3:before] == "new"
                            and (before == 3 or not (src[before - 4].isalnum() or src[before - 4] in "_$"))
                        ):
                            callee_start = before - 3
                        return re.sub(r"\s+", "", src[callee_start:end]) + ".pathname"
                    break
            i -= 1
    start = _member_chain_start(src, end)
    if start is None:
        return ".pathname"
    return re.sub(r"\s+", "", src[start:end]) + ".pathname"


# These are deliberately exact source identities, not directory or filename
# heuristics.  They are URL/path reads in UI, docs tooling, or assignment code,
# not Worker request dispatch.  Any new expression must be modeled or fail.
APP_NON_DISPATCH_ALLOWLIST: dict[tuple[str, str], str] = {
    ("apps/admin-ui/src/middleware.ts", "req.nextUrl.pathname"): "Next middleware page matching",
    ("apps/admin-ui/src/components/UpgradeButton.tsx", "window.location.pathname"): "browser UI URL read",
    ("apps/docs/worker/index.ts", "dest.pathname"): "docs asset URL rewrite",
    ("apps/docs/worker/index.ts", "url.pathname"): "docs asset URL rewrite",
}
for _docs_script in (
    "i18n-coverage.ts",
    "export-xliff.ts",
    "i18n-coverage-report.ts",
    "mt-stub-seed.ts",
    "import-xliff.ts",
    "translation-quality-check.ts",
):
    APP_NON_DISPATCH_ALLOWLIST[
        (f"apps/docs/scripts/{_docs_script}", 'newURL(".",import.meta.url).pathname')
    ] = "docs filesystem tooling"


def app_dispatch_spans(src: str, aliases: set[str]) -> list[tuple[int, int]]:
    """Spans whose pathname token was consumed by a modeled dispatch form."""
    spans: list[tuple[int, int]] = []
    for match in app_comparisons(src):
        if is_app_path_term(match.group("left"), aliases) or is_app_path_term(match.group("right"), aliases):
            spans.append((match.start(), match.end()))
    for match in APP_STARTS_WITH.finditer(src):
        if is_app_path_term(match.group("term"), aliases):
            spans.append((match.start(), match.end()))
    for switch in APP_SWITCH.finditer(src):
        if is_app_path_term(switch.group("term"), aliases):
            spans.append((switch.start(), switch.end()))
    return spans


def collect_app_pathname_census() -> list[str]:
    """Fail closed on every live app pathname token not otherwise consumed."""
    findings: list[str] = []
    for file in iter_app_typescript():
        if is_test_path(file):
            continue
        try:
            raw = file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if ".pathname" not in raw:
            continue
        src = strip_ts_comments(raw)
        aliases = app_path_aliases(src)
        spans = app_dispatch_spans(src, aliases)
        relative = str(file.relative_to(REPO))
        # ``src`` has the same offsets as ``raw`` and has already passed the
        # syntax-checked comment/literal pass, so avoid lexing each file twice.
        for offset, expression in _live_pathname_tokens(src, validated=True):
            if any(start <= offset < end for start, end in spans):
                continue
            if (relative, expression) in APP_NON_DISPATCH_ALLOWLIST:
                continue
            line = raw.count("\n", 0, offset) + 1
            findings.append(
                f"{relative}:{line}: unsupported app pathname token {expression}"
            )
    return sorted(set(findings))


def collect_app_unsupported() -> list[str]:
    """Return source locations for app pathname dispatches we cannot model.

    This is deliberately fail-closed for public API paths: an app Worker may
    use a prefix, switch arm, dynamic template, or unresolved route constant,
    but the parity gate must stop until that form is modeled or explicitly
    reviewed. Non-public literals are retained in the census for diagnostics
    but do not fail the contract.
    """
    findings: list[str] = []
    if not APPS.is_dir():
        return findings
    for file in iter_app_typescript():
        if is_test_path(file):
            continue
        try:
            raw = file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "pathname" not in raw:
            continue
        src = strip_ts_comments(raw)
        constants = app_string_constants(src)
        aliases = app_path_aliases(src)

        def record(match: re.Match[str], kind: str, value: str | None) -> None:
            # An unresolved expression in apps/** is deliberately public until
            # proven otherwise.  Entrypoint names and constants are not a
            # sound boundary: a helper can be imported by a deployed Worker,
            # and guessing from names made the old census silently green.
            public = True if value is None else is_public(value)
            if public:
                line = src.count("\n", 0, match.start()) + 1
                where = f"{file.relative_to(REPO)}:{line}"
                shown = value or "<unresolved>"
                findings.append(f"{where}: unsupported app pathname {kind} {shown}")

        for match in app_comparisons(src):
            left, right = match.group("left"), match.group("right")
            if not (is_app_path_term(left, aliases) or is_app_path_term(right, aliases)):
                continue
            value = app_static_value(right, constants)
            if value is None:
                value = app_static_value(left, constants)
            token = right if is_app_path_term(left, aliases) else left
            if value is None:
                record(match, "comparison", None)
            elif (
                value is not None
                and value.startswith("/v1")
                and app_static_value(token, constants) is None
            ):
                record(match, "dynamic comparison", value)

        for match in APP_STARTS_WITH.finditer(src):
            if not is_app_path_term(match.group("term"), aliases):
                continue
            value = app_static_value(match.group("value"), constants)
            record(match, "startsWith", value if value and value.startswith("/") else None)

        for switch in APP_SWITCH.finditer(src):
            if not is_app_path_term(switch.group("term"), aliases):
                continue
            for case in APP_CASE.finditer(switch.group("body")):
                value = app_static_value(case.group("value"), constants)
                if value is None or value.startswith("/v1"):
                    record(case, "switch", value)
    findings.extend(collect_app_pathname_census())
    return sorted(set(findings))


# ==========================================================================
# Comparison
# ==========================================================================
PARAM = re.compile(r"(?:\{[A-Za-z0-9_]+\}|:[A-Za-z0-9_]+|\{\*[A-Za-z0-9_]+\})")


def segments(path: str) -> list[str]:
    return (path.rstrip("/") or "/").split("/")


def covers(doc_path: str, served_path: str) -> bool:
    """Does the DOCUMENTED path `doc_path` describe the SERVED `served_path`?

    Segment-wise, same arity.  A documented parameter is a wildcard — the spec
    writes `/v1/privacy/dsr/{action}` where the code registers the seven
    literal actions, and those are the same endpoint.

    A SERVED parameter, however, only satisfies a documented parameter, never
    a documented literal.  The asymmetry is deliberate: letting a served
    `/v1/audit/{tenant}` vouch for a documented `/v1/audit/export` is exactly
    the B-116/B-121 path-divergence class — same shape, different meaning,
    404 after a successful auth.
    """
    d, s = segments(doc_path), segments(served_path)
    if len(d) != len(s):
        return False
    for dseg, sseg in zip(d, s):
        if PARAM.fullmatch(dseg):
            continue  # documented parameter absorbs anything
        if dseg != sseg:
            return False
    return True


def is_public(path: str) -> bool:
    if path.startswith(NON_PUBLIC_PREFIXES):
        return False
    if path.startswith(PROTOCOL_PREFIXES):
        return False
    return path.startswith("/v1/") or path == "/v1"


def compare(documented, rust_routes, worker_routes, app_routes=None):
    app_routes = app_routes or {}
    # Direction A (documented -> served) counts BOTH sources: a path the
    # Worker terminates at the edge (/api/health) is served even though no
    # crate registers it.
    served_all = sorted(set(rust_routes) | set(worker_routes) | set(app_routes))
    missing_route = [
        p for p in sorted(documented) if not any(covers(p, s) for s in served_all)
    ]

    # Direction B (served -> documented) counts crate registrations and exact
    # app pathname dispatch. A Worker `path === "/v1/onboarding"` arm is a
    # DISPATCH guard, not an endpoint declaration, and there is no structural
    # way to tell the two apart in matchRoute. Conservative on purpose: this
    # direction may under-report a genuinely edge-only endpoint, and never
    # invents one.
    missing_doc = []
    for path in sorted(set(rust_routes) | set(app_routes)):
        if not is_public(path):
            continue
        if any(covers(d, path) for d in documented):
            continue
        where = set(rust_routes.get(path, set())) | set(app_routes.get(path, set()))
        missing_doc.append((path, sorted(where)))
    return missing_route, missing_doc


# ==========================================================================
# Self-test — the positive control
# ==========================================================================
SELF_TEST_CASES = [
    # (form the extractor must see, a path it must therefore find)
    ("string literal on the .route( line", "/v1/onboarding/tier-select"),
    ("path literal on the line AFTER .route(", None),  # filled in dynamically
    ("route registered via a const", "/v1/audit/{tenant}/export"),
    ("Worker-terminated exact path", "/api/health"),
]


def app_mutation_self_test() -> list[str]:
    """Run app-dispatch mutations through the real census and comparison.

    Regex-only controls can prove that a pattern matches a string while the
    directory walk remains blind.  This fixture creates actual files under an
    ``apps/`` tree, runs both app extractors, and requires the resulting
    served public route to be red in the raw (strict) comparison.
    """
    global REPO, APPS
    old_repo, old_apps = REPO, APPS
    failures: list[str] = []
    try:
        with tempfile.TemporaryDirectory(prefix="validate-api-b130-") as tmp:
            root = Path(tmp)
            apps = root / "apps" / "fixture-worker"
            apps.mkdir(parents=True)
            fixtures = {
                "exact.ts": 'if (url.pathname === "/v1/b130-mutation") return response;\n',
                "reversed.ts": 'if ("/v1/b130-reversed" === url.pathname) return response;\n',
                "constant.ts": (
                    'const ROUTE = "/v1/b130-constant";\n'
                    "if (url.pathname === ROUTE) return response;\n"
                ),
                "alias.ts": (
                    "const path = request.url.pathname;\n"
                    'if (path === "/v1/b130-alias") return response;\n'
                ),
                "dynamic.ts": (
                    'if (url.pathname === `/v1/b130/${requestId}`) return response;\n'
                ),
                "not-equal.ts": (
                    "if (url.pathname !== routeFromConfig()) return notFound;\n"
                ),
                "includes.ts": (
                    'if (url.pathname.includes("/v1/b130-hidden")) return response;\n'
                ),
                "prefix.ts": (
                    'if (url.pathname.startsWith("/v1/b130/")) return response;\n'
                ),
                "switch.ts": (
                    'switch (url.pathname) { case "/v1/b130-switch": return response; }\n'
                ),
                "tsx-route.tsx": (
                    'if (url.pathname === "/v1/b130-tsx") return response;\n'
                ),
                "unresolved.ts": (
                    "const UNKNOWN_ROUTE = routeFromConfig();\n"
                    "if (url.pathname === UNKNOWN_ROUTE) return response;\n"
                ),
                "ignored.SPEC.TSX": (
                    'if (url.pathname === "/v1/b130-excluded") return response;\n'
                ),
                "lexical.ts": (
                    '// url.pathname in a comment\n'
                    'const text = "url.pathname";\n'
                    'const template = `.pathname`;\n'
                ),
            }
            for name, source in fixtures.items():
                (apps / name).write_text(source, encoding="utf-8")
            for directory, name in (
                ("TEST", "test-route.ts"),
                ("SPEC", "spec-route.tsx"),
                ("PLAYWRIGHT", "playwright-route.tsx"),
                ("__TESTS__", "tests-route.tsx"),
                ("E2E", "e2e-route.tsx"),
            ):
                path = root / "apps" / "fixture-worker" / directory / name
                path.parent.mkdir()
                path.write_text(
                    'if (url.pathname === "/v1/b130-excluded") return response;\n',
                    encoding="utf-8",
                )

            REPO, APPS = root, root / "apps"
            routes = collect_app_routes()
            unsupported = collect_app_unsupported()
            expected_routes = {
                "/v1/b130-mutation",
                "/v1/b130-reversed",
                "/v1/b130-constant",
                "/v1/b130-alias",
                "/v1/b130-tsx",
            }
            missing_route, missing_doc = compare({}, {}, {}, routes)
            missing_doc_paths = {path for path, _ in missing_doc}
            if not expected_routes.issubset(routes):
                failures.append("static app mutations were not extracted")
            if "/v1/b130-mutation" not in missing_doc_paths:
                failures.append("strict comparison did not turn mutation into MISSING_DOC")
            expected_unsupported = (
                "dynamic",
                "token",
                "startsWith",
                "switch",
                "<unresolved>",
            )
            if not all(any(kind in finding for finding in unsupported) for kind in expected_unsupported):
                failures.append("unsupported app mutations did not reach fail-closed census")
            if "/v1/b130-excluded" in routes or any(
                "/v1/b130-excluded" in finding for finding in unsupported
            ):
                failures.append("test/spec/Playwright/e2e app fixtures entered production census")
            if any("lexical.ts" in finding for finding in unsupported):
                failures.append("comments or strings entered live pathname census")

            # A malformed app source must make the real directory walk red at
            # the lexical state that is incomplete. Otherwise a truncated
            # comment/string/template can swallow a route and produce a false
            # green parity report.
            malformed = (
                ("unterminated-block.ts", "/* url.pathname\n", "block comment"),
                ("unterminated-string.ts", 'const hidden = "url.pathname;\n', "quoted string"),
                ("unterminated-template.ts", "const hidden = `url.pathname;\n", "template literal"),
                (
                    "unterminated-interpolation.ts",
                    "const hidden = `${url.pathname\n",
                    "template interpolation",
                ),
            )
            for name, source, reason in malformed:
                path = apps / name
                path.write_text(source, encoding="utf-8")
                try:
                    collect_app_routes()
                except SourceSyntaxError as exc:
                    if reason not in str(exc):
                        failures.append(f"{name} failed for wrong reason: {exc}")
                else:
                    failures.append(f"{name} did not fail closed in app route extraction")
                try:
                    _live_pathname_tokens(source)
                except SourceSyntaxError as exc:
                    if reason not in str(exc):
                        failures.append(f"{name} live lexer failed for wrong reason: {exc}")
                else:
                    failures.append(f"{name} did not fail closed in live pathname lexer")
                path.unlink()
            # `missing_route` is intentionally unused above: the strict-red
            # assertion is the served -> documented direction exercised here.
            del missing_route
    finally:
        REPO, APPS = old_repo, old_apps
    return failures


def source_comment_mutation_self_test() -> list[str]:
    """Prove Rust/Worker comment-only registrations never enter the inventory."""
    global REPO, CRATES, WORKER
    old_repo, old_crates, old_worker = REPO, CRATES, WORKER
    failures: list[str] = []
    try:
        with tempfile.TemporaryDirectory(prefix="validate-api-comments-") as tmp:
            root = Path(tmp)
            crates = root / "crates" / "fixture"
            worker = root / "worker" / "src"
            crates.mkdir(parents=True)
            worker.mkdir(parents=True)
            rust = crates / "routes.rs"
            rust.write_text(
                "/* outer\n"
                "   /* .route(\"/v1/b130-comment-only\", get(handler)) */\n"
                "*/\n"
                ".route(\"/v1/b130-live\", get(handler));\n",
                encoding="utf-8",
            )
            worker_file = worker / "index.ts"
            worker_file.write_text(
                "/* /api/b130-comment-only */\n"
                "if (path === \"/api/b130-live\") return response;\n",
                encoding="utf-8",
            )
            REPO, CRATES, WORKER = root, root / "crates", root / "worker" / "src"
            rust_routes = collect_rust_routes()
            worker_routes = collect_worker_routes()
            if "/v1/b130-live" not in rust_routes:
                failures.append("live Rust route was not extracted")
            if "/api/b130-live" not in worker_routes:
                failures.append("live Worker path was not extracted")
            if "/v1/b130-comment-only" in rust_routes:
                failures.append("comment-only Rust route entered inventory")
            if "/api/b130-comment-only" in worker_routes:
                failures.append("comment-only Worker path entered inventory")
            literals = strip_source_comments(
                'const text = "// /api/string"; '
                'const template = `/* ${path} */`; /* hidden */',
                nested=False,
            )
            if '"// /api/string"' not in literals or "`/* ${path} */`" not in literals:
                failures.append("Worker string/template literal was altered")
            rust.write_text("/* .route(\"/v1/b130-unterminated\")", encoding="utf-8")
            try:
                collect_rust_routes()
            except SourceSyntaxError:
                pass
            else:
                failures.append("unterminated Rust comment did not fail closed")
            worker_file.write_text("/* path === \"/api/b130-unterminated\"", encoding="utf-8")
            try:
                collect_worker_routes()
            except SourceSyntaxError:
                pass
            else:
                failures.append("unterminated Worker comment did not fail closed")
    finally:
        REPO, CRATES, WORKER = old_repo, old_crates, old_worker
    return failures


def self_test(documented, rust_routes, worker_routes, app_routes=None, app_unsupported=None) -> int:
    """Prove each extractor can SEE before any of its silences is believed.

    "found nothing" and "my command broke" are indistinguishable without this.
    """
    failures = []
    app_routes = app_routes or {}
    app_unsupported = app_unsupported or []

    def check(label, ok, detail):
        status = "ok  " if ok else "FAIL"
        print(f"  [{status}] {label}: {detail}")
        if not ok:
            failures.append(label)

    check(
        "openapi paths parsed",
        len(documented) >= 30,
        f"{len(documented)} documented paths (expected >= 30)",
    )
    # A spec with no `paths:` block must RAISE, never return {}.  An empty
    # documented surface silently satisfies every MISSING_ROUTE check, so a
    # parser that degrades to {} on malformed input is a gate that passes
    # because it read nothing.
    try:
        parse_openapi_paths("openapi: 3.1.0\ninfo:\n  title: no paths here\n")
        raised = False
    except SystemExit:
        raised = True
    check(
        "malformed spec REFUSES rather than reporting an empty surface",
        raised,
        "a `paths:`-less spec raises instead of returning {}",
    )
    check(
        "rust routes extracted",
        len(rust_routes) >= 50,
        f"{len(rust_routes)} distinct registered paths (expected >= 50)",
    )
    check(
        "worker exact paths extracted",
        len(worker_routes) >= 3,
        f"{len(worker_routes)} edge-terminated paths (expected >= 3)",
    )
    check(
        "apps pathname routes extracted",
        "/v1/event" in app_routes,
        "/v1/event (apps/analytics-worker/src/index.ts)",
    )
    check(
        "form 1 — literal on the .route( line",
        "/v1/onboarding/tier-select" in rust_routes,
        "/v1/onboarding/tier-select (routes/tier_select.rs)",
    )
    # Form 2: a path whose `.route(` and literal are on DIFFERENT lines. This
    # is the form a line-oriented grep cannot see at all.
    multiline = [
        p
        for p, where in rust_routes.items()
        if any(_literal_is_on_a_later_line(w, p) for w in where)
    ]
    check(
        "form 2 — literal on the line AFTER .route(",
        bool(multiline),
        f"{len(multiline)} such routes, e.g. {sorted(multiline)[:2]}",
    )
    check(
        "form 3 — registered via a const",
        "/v1/audit/{tenant}/export" in rust_routes,
        "/v1/audit/{tenant}/export (AUDIT_EXPORT_ROUTE)",
    )
    check(
        "form 4 — Worker-terminated exact path",
        "/api/health" in worker_routes,
        "/api/health (worker/src/index.ts matchRoute)",
    )
    # A path that is documented AND served must NOT be reported missing — a
    # gate that flags everything is as useless as one that flags nothing.
    missing_route, missing_doc = compare(documented, rust_routes, worker_routes, app_routes)
    check(
        "no false positive on a known-served documented path",
        "/v1/onboarding/tier-select" not in missing_route,
        "/v1/onboarding/tier-select is documented and served",
    )
    check(
        "apps routes participate in parity comparison",
        any(path == "/v1/event" for path, _ in missing_doc),
        "/v1/event is reported as an undocumented app route",
    )
    check(
        "apps unsupported pathname census is empty",
        not app_unsupported,
        "; ".join(app_unsupported[:3]) or "all app dispatch forms are modeled",
    )
    # Mutation controls exercise the forms that previously went invisible:
    # reversed equality, template interpolation, prefix dispatch, and switch
    # cases. The production census above then turns any newly introduced public
    # form into a hard failure instead of trusting this synthetic control.
    reversed_mutation = APP_COMPARE_REVERSED.search('"/v1/b130-reversed" === url.pathname')
    constant_reverse = APP_COMPARE_REVERSED_TERM.search("ROUTE === url.pathname")
    dynamic_mutation = APP_COMPARE.search('url.pathname === `/v1/b130/${id}`')
    prefix_mutation = APP_STARTS_WITH.search('url.pathname.startsWith("/v1/b130/")')
    switch_mutation = APP_SWITCH.search(
        'switch (url.pathname) { case "/v1/b130-switch": return response; }'
    )
    check("mutation — reversed pathname equality visible", reversed_mutation is not None, "reversed form")
    check("mutation — reversed constant equality visible", constant_reverse is not None, "constant reversed form")
    check("mutation — dynamic template reaches census", dynamic_mutation is not None, "template form")
    check("mutation — startsWith reaches census", prefix_mutation is not None, "prefix form")
    check("mutation — switch pathname reaches census", switch_mutation is not None, "switch form")
    check(
        "test fixtures excluded from app production scan",
        is_test_path(Path("apps/example/src/__tests__/route.test.ts")),
        ".test.ts and __tests__ are excluded",
    )
    mutation_failures = app_mutation_self_test()
    check(
        "mutation — temporary app tree reaches census and strict red",
        not mutation_failures,
        "; ".join(mutation_failures) or "real files extracted and raw comparison reports MISSING_DOC",
    )
    source_comment_failures = source_comment_mutation_self_test()
    check(
        "mutation — comment-only Rust/Worker registrations stay out",
        not source_comment_failures,
        "; ".join(source_comment_failures) or "comment-only registrations are ignored and malformed comments fail closed",
    )

    if failures:
        print(f"\nself-test FAILED ({len(failures)}): {', '.join(failures)}")
        print("The instrument is broken. Its silence means nothing. Fix it first.")
        return 1
    print("\nself-test passed — every extractor form is demonstrably visible.")
    return 0


def _literal_is_on_a_later_line(where: str, path: str) -> bool:
    file_part, _, line_no = where.rpartition(":")
    try:
        lines = (REPO / file_part).read_text(encoding="utf-8", errors="replace").splitlines()
        idx = int(line_no) - 1
    except (OSError, ValueError):
        return False
    if idx >= len(lines) or path in lines[idx]:
        return False
    return any(path in line for line in lines[idx + 1 : idx + 6])


# ==========================================================================
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--strict",
        action="store_true",
        help="ignore the declared-divergence ledger and report the raw truth",
    )
    ap.add_argument(
        "--self-test",
        action="store_true",
        help="prove the extractor can see each registration form, then exit",
    )
    args = ap.parse_args()

    if not OPENAPI.is_file():
        print(f"FATAL: {OPENAPI} not found — cannot read the documented surface.")
        return 2
    if not CRATES.is_dir():
        print(f"FATAL: {CRATES} not found — cannot read the served surface.")
        return 2
    if not APPS.is_dir():
        print(f"FATAL: {APPS} not found — cannot read the app Worker surface.")
        return 2

    documented = parse_openapi_paths(OPENAPI.read_text(encoding="utf-8"))
    try:
        rust_routes = collect_rust_routes()
        worker_routes = collect_worker_routes()
        app_routes = collect_app_routes()
        app_unsupported = collect_app_unsupported()
    except (OSError, UnicodeDecodeError, SourceSyntaxError) as exc:
        print(f"FATAL: route source parse failure: {exc}")
        return 2

    if args.self_test:
        print("validate_api_surface self-test (positive controls)\n")
        return self_test(documented, rust_routes, worker_routes, app_routes, app_unsupported)

    # The self-test is a PRECONDITION of every run, not an opt-in mode: an
    # extractor that has gone blind reports a clean surface, and a clean
    # report from a blind instrument is the failure mode this gate exists to
    # prevent.
    if self_test(documented, rust_routes, worker_routes, app_routes, app_unsupported) != 0:
        return 2
    if app_unsupported:
        print("FAIL: unsupported public app pathname dispatch (fail-closed census).")
        for finding in app_unsupported:
            print(f"  - {finding}")
        return 2
    print()

    missing_route, missing_doc = compare(documented, rust_routes, worker_routes, app_routes)

    print("=" * 74)
    print("DOCUMENTED (openapi/corelink-v1.yaml) but NOT SERVED")
    print("=" * 74)
    if not missing_route:
        print("  (none)")
    for path in missing_route:
        note = LEDGER_MISSING_ROUTE.get(path)
        methods = ",".join(sorted(documented[path])) or "?"
        tag = f"  [declared: {note}]" if note and not args.strict else ""
        print(f"  MISSING_ROUTE  {methods:<16} {path}{tag}")

    print()
    print("=" * 74)
    print("SERVED but NOT DOCUMENTED (public /v1 surface)")
    print("=" * 74)
    if not missing_doc:
        print("  (none)")
    for path, where in missing_doc:
        note = LEDGER_MISSING_DOC.get(path)
        tag = f"  [declared: {note}]" if note and not args.strict else ""
        print(f"  MISSING_DOC    {path}{tag}")
        print(f"                 registered at {where[0]}")

    print()
    print(
        f"summary: {len(documented)} documented paths, "
        f"{len(rust_routes)} registered crate routes, "
        f"{len(worker_routes)} edge-terminated paths, "
        f"{len(app_routes)} app pathname routes — "
        f"{len(missing_route)} MISSING_ROUTE, {len(missing_doc)} MISSING_DOC"
    )

    if args.strict:
        total = len(missing_route) + len(missing_doc)
        print(f"\n--strict: ledger ignored. {total} raw divergence(s).")
        return 1 if total else 0

    undeclared_route = [p for p in missing_route if p not in LEDGER_MISSING_ROUTE]
    undeclared_doc = [p for p, _ in missing_doc if p not in LEDGER_MISSING_DOC]
    stale_route = [p for p in LEDGER_MISSING_ROUTE if p not in set(missing_route)]
    stale_doc = [p for p in LEDGER_MISSING_DOC if p not in {q for q, _ in missing_doc}]

    print()
    if undeclared_route or undeclared_doc:
        print("FAIL: undeclared divergence between the documented and served surface.")
        for p in undeclared_route:
            print(f"  - documented but not served: {p}")
        for p in undeclared_doc:
            print(f"  - served but not documented: {p}")
        print(
            "\nFix openapi/corelink-v1.yaml or the route. If the divergence is\n"
            "known and tracked, add it to the ledger in this script WITH its\n"
            "backlog id — an unexplained entry is a silenced gate."
        )
    if stale_route or stale_doc:
        print("FAIL: STALE_LEDGER — these no longer diverge; remove their entries.")
        for p in stale_route + stale_doc:
            print(f"  - {p}")

    if undeclared_route or undeclared_doc or stale_route or stale_doc:
        return 1

    print(
        f"OK: no undeclared divergence "
        f"({len(LEDGER_MISSING_ROUTE) + len(LEDGER_MISSING_DOC)} declared, all still present)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
