#!/usr/bin/env python3
"""Closed-world TypeScript Worker pathname extraction for B-130."""

from __future__ import annotations

import re
import tempfile
from collections.abc import Iterator
from pathlib import Path

from validate_api_surface_support import SourceSyntaxError

REPO = Path(__file__).resolve().parent.parent
APPS = REPO / "apps"
APP_TYPESCRIPT_SUFFIXES = {".ts", ".tsx"}
NON_PUBLIC_PREFIXES = ("/_internal/", "/internal/", "/_health", "/health", "/metrics", "/__")
PROTOCOL_PREFIXES = (
    "/v2/", "/npm/", "/pypi/", "/simple/", "/bazel/", "/cargo/", "/oci/",
    "/artifacts/", "/cache/", "/cas/", "/ac/",
)


def configure_roots(root: Path) -> None:
    """Point the extractor at a temporary tree used by mutation tests."""
    global REPO, APPS
    REPO, APPS = root, root / "apps"


def is_test_path(path: Path) -> bool:
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
    if not APPS.is_dir():
        return
    for file in sorted(APPS.rglob("*")):
        if file.is_file() and file.suffix.casefold() in APP_TYPESCRIPT_SUFFIXES:
            yield file


def is_public(path: str) -> bool:
    if path.startswith(NON_PUBLIC_PREFIXES) or path.startswith(PROTOCOL_PREFIXES):
        return False
    return path.startswith("/v1/") or path == "/v1"


# Exact source/path dispositions for deployed app ingress that belongs to a
# machine-to-machine or provider contract, rather than customer OpenAPI. Keys
# include the source file so a newly added or moved customer dispatch cannot
# inherit a worker-wide or path-wide exclusion. The deployment and auth
# boundaries are documented beside each entry.
APP_NON_CUSTOMER_ROUTES: dict[tuple[str, str], str] = {
    # B-054: internal audit-witness API. The Worker requires its private bearer
    # token before dispatch (src/index.ts:421-423); callers are the audit chain,
    # and the DO's /append is reached through witness.internal, not public DNS.
    ("apps/audit-witness-worker/src/index.ts", "/v1/audit-chain/head-witness/compare-and-append"):
        "B-054 private audit-chain machine-to-machine API (bearer-authenticated)",
    ("apps/audit-witness-worker/src/index.ts", "/v1/audit-chain/head-witness/latest"):
        "B-054 private audit-chain machine-to-machine API (bearer-authenticated)",
    # B-072: synthetic drill service binding is disabled by default and runs
    # only in dev/staging; the callback route exists only on staging's
    # staging.corelink.humangr.com binding. See wrangler.toml and contract.ts.
    ("apps/synthetic-pager-worker/src/index.ts", "/v1/drills/synthetic_page"):
        "B-072 disabled-by-default dev/staging service-binding ingress",
    ("apps/synthetic-pager-worker/src/index.ts", "/v1/webhooks/pagerduty"):
        "B-072 staging-only PagerDuty provider callback (signature-authenticated)",
}


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
APP_EXPORTED_CONST = re.compile(
    rf"\bexport\s+const\s+(?P<name>{APP_IDENT})\s*=\s*(?P<value>{APP_QUOTED})"
)
APP_NAMED_IMPORT = re.compile(
    rf"\bimport\s*\{{(?P<names>[^}}]+)\}}\s*from\s*(?P<module>{APP_QUOTED})",
    re.DOTALL,
)
APP_AUDIT_WITNESS_REJECTION = re.compile(
    r'''if\s*\(\s*request\.method\s*!==\s*["']POST["']\s*\|\|\s*'''
    r'''url\.pathname\s*!==\s*["']/append["']\s*\)\s*'''
    r'''return\s+fail\(\s*["']NOT_FOUND["']\s*,\s*404\s*\)\s*;?'''
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
        constants = app_string_constants(src, file)
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


def app_string_constants(src: str, source_file: Path | None = None) -> dict[str, str]:
    """Static local string constants plus direct named imports from app TS.

    Imported values are followed only through a relative, in-app module edge
    and only to a directly exported string literal. Aliasing, re-exports,
    computed values, and package imports stay unresolved so public dispatches
    using them remain visible to the fail-closed census.
    """
    constants: dict[str, str] = {}
    for match in APP_CONST.finditer(src):
        value = app_unquote(match.group("value"))
        if value is not None:
            constants[match.group("name")] = value
    if source_file is None:
        return constants

    for imported in APP_NAMED_IMPORT.finditer(src):
        module = app_unquote(imported.group("module"))
        if module is None or not module.startswith("."):
            continue
        imported_source = resolve_app_import(source_file, module)
        if imported_source is None:
            continue
        try:
            imported_src = strip_ts_comments(
                imported_source.read_text(encoding="utf-8", errors="replace")
            )
        except OSError:
            continue
        exports = {
            match.group("name"): value
            for match in APP_EXPORTED_CONST.finditer(imported_src)
            if (value := app_unquote(match.group("value"))) is not None
        }
        for entry in imported.group("names").split(","):
            parts = re.fullmatch(
                rf"\s*({APP_IDENT})(?:\s+as\s+({APP_IDENT}))?\s*", entry
            )
            if parts is None:
                continue
            exported_name, local_name = parts.groups()
            value = exports.get(exported_name)
            if value is not None:
                constants[local_name or exported_name] = value
    return constants


def resolve_app_import(source_file: Path, module: str) -> Path | None:
    """Resolve one relative TS module within the apps tree."""
    base = (source_file.parent / module).resolve()
    if not base.is_relative_to(APPS.resolve()):
        return None
    candidates = [base] if base.suffix in APP_TYPESCRIPT_SUFFIXES else [
        base.with_suffix(".ts"),
        base.with_suffix(".tsx"),
        base / "index.ts",
        base / "index.tsx",
    ]
    return next(
        (candidate for candidate in candidates if candidate.is_file() and not is_test_path(candidate)),
        None,
    )


def app_customer_routes(routes: dict[str, set[str]]) -> dict[str, set[str]]:
    """Remove only exact evidence-backed non-customer app ingress sources."""
    customer: dict[str, set[str]] = {}
    for route, locations in routes.items():
        for location in locations:
            source = location.rsplit(":", 1)[0]
            if (source, route) not in APP_NON_CUSTOMER_ROUTES:
                customer.setdefault(route, set()).add(location)
    return customer


def app_negative_nonroute_spans(file: Path, src: str) -> list[tuple[int, int]]:
    """Consume the known audit-witness DO rejection guard without inventing a route.

    This guard rejects every request except POST /append and is not the public
    dispatch table. Its exact source, operator, path, method check, and 404
    response are pinned; edits to any of those details become census findings.
    """
    relative = str(file.relative_to(REPO))
    if relative != "apps/audit-witness-worker/src/index.ts":
        return []
    return [
        (match.start(), match.end())
        for match in APP_AUDIT_WITNESS_REJECTION.finditer(src)
    ]


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
        spans.extend(app_negative_nonroute_spans(file, src))
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
        constants = app_string_constants(src, file)
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
    active_routes = collect_app_routes()
    active_dispositions = {
        (location.rsplit(":", 1)[0], route)
        for route, locations in active_routes.items()
        for location in locations
    }
    for source, route in APP_NON_CUSTOMER_ROUTES:
        if (source, route) not in active_dispositions:
            findings.append(
                f"{source}: stale non-customer app route disposition {route}"
            )
    return sorted(set(findings))


# ==========================================================================
# Comparison
# ==========================================================================

def app_mutation_self_test(compare_fn) -> list[str]:
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
                "imported.ts": (
                    'import { IMPORTED_ROUTE as ROUTE } from "./contract";\n'
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
            contract = apps / "contract.ts"
            contract.write_text(
                'export const IMPORTED_ROUTE = "/v1/b130-imported" as const;\n',
                encoding="utf-8",
            )
            audit_witness = root / "apps/audit-witness-worker/src/index.ts"
            audit_witness.parent.mkdir(parents=True)
            audit_witness.write_text(
                'if (request.method !== "POST" || url.pathname !== "/append") '
                'return fail("NOT_FOUND", 404);\n'
                'if (request.method === "POST" && url.pathname === '
                '"/v1/audit-chain/head-witness/compare-and-append") return response;\n'
                'if (request.method === "POST" && url.pathname === '
                '"/v1/audit-chain/head-witness/latest") return response;\n',
                encoding="utf-8",
            )
            synthetic = root / "apps/synthetic-pager-worker/src/index.ts"
            synthetic.parent.mkdir(parents=True)
            synthetic.write_text(
                'import { SYNTHETIC_PAGE_PATH, PAGERDUTY_WEBHOOK_PATH } from "./contract";\n'
                'if (url.pathname === SYNTHETIC_PAGE_PATH) return response;\n'
                'if (url.pathname === PAGERDUTY_WEBHOOK_PATH) return response;\n',
                encoding="utf-8",
            )
            synthetic_contract = synthetic.parent / "contract.ts"
            synthetic_contract.write_text(
                'export const SYNTHETIC_PAGE_PATH = "/v1/drills/synthetic_page" as const;\n'
                'export const PAGERDUTY_WEBHOOK_PATH = "/v1/webhooks/pagerduty" as const;\n',
                encoding="utf-8",
            )
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
                "/v1/b130-imported",
                "/v1/b130-alias",
                "/v1/b130-tsx",
            }
            missing_route, missing_doc = compare_fn({}, {}, {}, routes)
            missing_doc_paths = {path for path, _ in missing_doc}
            if not expected_routes.issubset(routes):
                failures.append("static app mutations were not extracted")
            if "/v1/b130-mutation" not in missing_doc_paths:
                failures.append("strict comparison did not turn mutation into MISSING_DOC")
            if "/append" in routes:
                failures.append("negative audit-witness 404 guard was invented as a served route")
            if any(
                "unsupported app pathname token url.pathname" in finding
                and "audit-witness-worker" in finding
                for finding in unsupported
            ):
                failures.append("exact audit-witness rejection guard was not classified")

            private_audit_path = "/v1/audit-chain/head-witness/latest"
            _, private_missing_doc = compare_fn(
                {},
                {},
                {},
                {private_audit_path: {"apps/audit-witness-worker/src/index.ts:442"}},
            )
            if any(path == private_audit_path for path, _ in private_missing_doc):
                failures.append("exact B-054 machine route was reported as customer OpenAPI")
            adjacent_public = "/v1/audit-chain/head-witness/new"
            _, adjacent_missing_doc = compare_fn(
                {},
                {},
                {},
                {adjacent_public: {"apps/audit-witness-worker/src/index.ts:443"}},
            )
            if adjacent_public not in {path for path, _ in adjacent_missing_doc}:
                failures.append("adjacent audit-witness path inherited the private disposition")
            _, unrelated_same_path = compare_fn(
                {},
                {},
                {},
                {private_audit_path: {"apps/fixture-worker/src/index.ts:1"}},
            )
            if private_audit_path not in {path for path, _ in unrelated_same_path}:
                failures.append("private path classification hid an unrelated source")

            # The exact imported constant stays resolvable, while a changed
            # value remains a real public route and a missing module remains
            # an unresolved, fail-closed dispatch.
            contract.write_text(
                'export const IMPORTED_ROUTE = "/v1/b130-imported-changed" as const;\n',
                encoding="utf-8",
            )
            changed_routes = collect_app_routes()
            _, changed_missing_doc = compare_fn({}, {}, {}, changed_routes)
            if "/v1/b130-imported-changed" not in {path for path, _ in changed_missing_doc}:
                failures.append("changed imported route constant did not remain a public finding")
            contract.unlink()
            unresolved = collect_app_unsupported()
            if not any("<unresolved>" in finding and "imported.ts" in finding for finding in unresolved):
                failures.append("missing imported route module did not fail closed")

            audit_witness.write_text(
                'if (request.method !== "POST" || url.pathname !== "/append-mutated") '
                'return fail("NOT_FOUND", 404);\n',
                encoding="utf-8",
            )
            changed_guard = collect_app_unsupported()
            if not any(
                "unsupported app pathname token url.pathname" in finding
                and "audit-witness-worker" in finding
                for finding in changed_guard
            ):
                failures.append("changed negative pathname guard escaped the fail-closed census")
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
