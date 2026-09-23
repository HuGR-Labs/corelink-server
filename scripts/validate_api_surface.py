#!/usr/bin/env python3
"""Compare the DOCUMENTED API surface against the SERVED one (B-121).

The documented surface is `openapi/corelink-v1.yaml` — the hand-written spec
that `scripts/gen-api-reference.py` turns into the MDX pages under
`apps/docs/docs/reference/api/endpoints/` (one per operation; 35 at the time of
writing — the generator derives the count, this comment does not).  The pages
are propagation, not source: regenerating them fixes nothing.  The spec is what
drifts.

The generator does NOT maintain the translated mirrors under
`apps/docs/i18n/*/docusaurus-plugin-content-docs/current/reference/api/`.  Those
are MT-stub copies and they went on publishing 45 pages for endpoints that had
already been deleted from EN.  This gate does not see them either; deleting a
path means deleting its translated pages and index rows by hand.

The served surface is `crates/`, `worker/src`, and the Worker entrypoints under
`apps/**`.  Three registration sites:

  * axum `.route(<path-expr>, ...)` in `crates/` — the path expression may be
    a string literal, may sit on the line AFTER `.route(`, and is frequently a
    `const` (`AUDIT_EXPORT_ROUTE`, `TURBO_GET_ROUTE`, `ROUTE_EVENT_COUNT`, …);
  * exact path comparisons in the Worker's `matchRoute` table
    (`worker/src/index.ts`) — `path === "/api/health"` and friends, which are
    served AT THE EDGE and never reach the container.

  * exact `url.pathname === "/..."` dispatch in sibling Workers under
    `apps/**`, including a direct named import of an exported static string
    from a relative app TypeScript module (the source file is retained for an
    actionable finding).

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

  MISSING_ROUTE  a documented path that nothing serves — a published lie
  MISSING_DOC    a served public path that nothing documents          (B-117)

The two are NOT treated alike.  MISSING_ROUTE has NO ledger: any occurrence
fails the gate, and a re-populated `LEDGER_MISSING_ROUTE` fails it too.  (It
used to be ledgerable, and 14 entries kept this gate green while 60 reference
pages shipped for endpoints that 404 — see the comment on that dict.)

MISSING_DOC keeps its ledger, each entry pinned to its backlog id.  The gate
fails on anything NOT in that ledger, and equally on a ledger entry that no
longer diverges (STALE_LEDGER) — so a fix cannot land while leaving its excuse
behind.  `--strict` ignores the ledger entirely and reports the raw truth.

Neither the ledger nor this docstring is the oracle for the counts: every
number the gate prints is derived from the lists it just built.

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

from validate_api_surface_support import SourceSyntaxError
from validate_api_surface_app import (
    APP_CASE,
    APP_COMPARE,
    APP_COMPARE_REVERSED,
    APP_COMPARE_REVERSED_TERM,
    APP_STARTS_WITH,
    APP_SWITCH,
    app_mutation_self_test,
    app_customer_routes,
    collect_app_routes,
    collect_app_unsupported,
    _live_pathname_tokens,
    strip_ts_comments,
)

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
# MISSING_ROUTE IS NOT LEDGERABLE.  This dict exists only so the gate can
# refuse a non-empty one; it must stay empty.
#
# The two directions are not symmetric.  MISSING_DOC (served, undocumented) is
# a gap in our writing: a customer who never reads about the endpoint is not
# harmed by it.  MISSING_ROUTE (documented, not served) is a PUBLISHED LIE —
# `scripts/gen-api-reference.py` turns every documented path into a reference
# page with copy-pasteable curl/Rust/Python/Go/JS snippets aimed at
# `https://corelink-api.humangr.com<path>`, in EN plus three translated
# mirrors.  A caller who follows it authenticates and then 404s.
#
# That direction was ledgered anyway: 14 entries carrying B-116/119/120/121
# kept the gate GREEN across every one of them, and 15 EN + 45 translated
# reference pages shipped for endpoints that do not exist.  A declared
# exception is meant to be a rare, tracked pause; a whole customer-facing
# failure class parked behind four backlog ids is the gate being talked out of
# its own verdict.
#
# So the exception mechanism is removed for this direction.  A path that is
# documented and not served must be fixed in the spec in the SAME change —
# repointed at what is actually served, or deleted.  If a genuinely
# unimplemented endpoint must be published ahead of its route, that is a
# deliberate product decision and needs a human waiver, not a dict entry.
LEDGER_MISSING_ROUTE: dict[str, str] = {}

LEDGER_MISSING_DOC: dict[str, str] = {
    # --- the rest of the same undocumented customer portal ------------------
    "/v1/customer/audit": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/billing": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/billing/portal": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/overview": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/runners/allowlist": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/runners/entitlement": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/runners/runs": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/team": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/team/{user_id}": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/usage": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/workspaces": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/workspaces/{workspace_id}": "B-117 (same customer-portal family; never documented)",
    "/v1/customer/workspaces/{workspace_id}/pin": "B-117 (same customer-portal family; never documented)",
    # --- twins of the path divergences above --------------------------------
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
# ``index_special_routes.ts`` dispatches the customer team/DSR endpoints via
# ``route.pathSuffix``; both forms are live exact-path Worker decisions.
WORKER_EXACT = re.compile(r'(?:path|pathSuffix)\s*===\s*"(/[^"]*)"')


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
    app_routes = app_customer_routes(app_routes or {})
    # Direction A (documented -> served) counts BOTH sources: a path the
    # Worker terminates at the edge (/api/health) is served even though no
    # crate registers it.
    served_all = sorted(set(rust_routes) | set(worker_routes) | set(app_routes))
    missing_route = [
        p for p in sorted(documented) if not any(covers(p, s) for s in served_all)
    ]

    # Direction B (served -> documented) counts crate registrations and exact
    # customer app pathname dispatch after private/provider source
    # dispositions. A Worker `path === "/v1/onboarding"` arm is a
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
    mutation_failures = app_mutation_self_test(compare)
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

    # MISSING_ROUTE is not ledgerable — see the comment on LEDGER_MISSING_ROUTE.
    # Refuse a re-populated dict rather than honouring it: otherwise the next
    # author reopens the exception with one line and the gate goes quiet again.
    if LEDGER_MISSING_ROUTE:
        print()
        print(
            "FAIL: LEDGER_MISSING_ROUTE is non-empty. That direction is a\n"
            "PUBLISHED LIE (a reference page with working snippets for an\n"
            "endpoint that 404s) and carries no exception mechanism. Fix the\n"
            "spec — repoint the path at what is served, or delete it."
        )
        for p, why in sorted(LEDGER_MISSING_ROUTE.items()):
            print(f"  - {p}  [{why}]")
        return 1

    undeclared_route = list(missing_route)
    undeclared_doc = [p for p, _ in missing_doc if p not in LEDGER_MISSING_DOC]
    stale_route: list[str] = []
    stale_doc = [p for p in LEDGER_MISSING_DOC if p not in {q for q, _ in missing_doc}]

    print()
    if undeclared_route or undeclared_doc:
        print("FAIL: divergence between the documented and served surface.")
        for p in undeclared_route:
            print(f"  - documented but not served: {p}")
        for p in undeclared_doc:
            print(f"  - served but not documented: {p}")
        print(
            "\nDocumented-but-not-served has NO ledger: fix openapi/corelink-v1.yaml\n"
            "in this change — repoint the path at the route that is actually\n"
            "registered, or remove it (and regenerate the reference pages with\n"
            "`python3 scripts/gen-api-reference.py`, including the i18n mirrors\n"
            "under apps/docs/i18n/*/…/reference/api/, which the generator does\n"
            "NOT maintain).\n"
            "Served-but-not-documented may be declared: add it to\n"
            "LEDGER_MISSING_DOC WITH its backlog id — an unexplained entry is a\n"
            "silenced gate."
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
