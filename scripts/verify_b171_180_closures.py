#!/usr/bin/env python3
"""Executable, inverted closure gate for D03 findings B-171..B-180.

The gate checks the implementation boundary (not only a backlog packet), then
mutates each load-bearing clause in memory and requires that the mutation is
detected.  It is intentionally focal and has no network, Cargo, or CI calls.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ClosureError(ValueError):
    pass


_MAX_LEX_SOURCE_BYTES = 2_000_000
_MAX_LEX_TOKENS = 100_000


# (artifact, required clauses, mutation needle). Clauses are deliberately
# spread across executable wiring and storage behavior so comments alone cannot
# close an item.
CONTRACTS: dict[str, tuple[str, tuple[str, ...], str]] = {
    "B-171": (
        "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs",
        (r"tenant_prefix\(self\.tdk\.as_ref\(\),\s*tenant\)\?", r"R2S3Client::blob_key", r"derive_tenant_prefix_strict"),
        "tenant_prefix(self.tdk.as_ref(), tenant)?",
    ),
    "B-172": (
        "crates/corelink-adapter-host/src/oci/server/handlers.rs",
        (r"MAX_MANIFEST_BYTES", r"to_bytes\(body, MAX_MANIFEST_BYTES\)", r"ManifestOversized"),
        "to_bytes(body, MAX_MANIFEST_BYTES)",
    ),
    "B-173": (
        "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
        (r"OCI_MAX_OPEN_SESSIONS_PER_TENANT", r"OCI_MAX_INFLIGHT_BYTES", r"OCI_MAX_INFLIGHT_BYTES_PER_TENANT", r"last_active_ms", r"inflight_bytes", r"reap"),
        "self.inflight_bytes.fetch_sub",
    ),
    "B-174": (
        "scripts/d1-apply-fk-parent.sh",
        (r"0064_tenant_tier_max\.sql", r'run\s+--file\s+"\$TMP_SQL"', r"d1_migrations", r"INSERT INTO d1_migrations"),
        'run --file "$TMP_SQL" >/dev/null',
    ),
    "B-175": (
        "worker/src/index_special_routes.ts",
        (r"const ociTenant = ociRoutingTenantId\(request\)", r"resolveTenantResidency\(env\.CONFIG_DB, ociTenant", r"h\.set\(\"x-corelink-primary-region\", primaryRegion\)", r"regionalBinding\.fetch"),
        "const ociTenant = ociRoutingTenantId(request);",
    ),
    "B-176": (
        "crates/corelink-container/src/routes/dsr/attestation.rs",
        (r"signature_ed25519", r"canonical_payload_jcs", r"INSERT OR IGNORE", r"put_audit_object"),
        "INSERT OR IGNORE INTO erasure_attestations",
    ),
    "B-177": (
        "crates/corelink-container/src/routes/build.rs",
        (r"pub\s+fn\s+build_with_factory", r"ratelimit_layer::rate_limit_layer", r"RateLimitLayerState"),
        "ratelimit_layer::rate_limit_layer",
    ),
    "B-178": (
        "worker/src/lib/quota.ts",
        (r"export async function checkRequestQuota", r"requestQuotaEnabled", r"ON CONFLICT\(tenant_id, year_month\)", r"request_count"),
        "ON CONFLICT(tenant_id, year_month)",
    ),
    "B-179": (
        "worker/src/index_special_passthrough.ts",
        (r"route\.routeKind\s*===\s*\"oci_v2\"", r"ociStub\.fetch", r"x-corelink-client-ip", r"cf-connecting-ip"),
        "ociStub.fetch(ociReq)",
    ),
    "B-180": (
        "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
        (r"pub const OCI_TOKEN_KEY_ENV", r"pub const OCI_TOKEN_KEY_ENV_LEGACY", r"CORELINK_OCI_TOKEN_KEY", r"HUGR_OCI_TOKEN_KEY"),
        "OCI_TOKEN_KEY_ENV_LEGACY",
    ),
}

EXTRA_ARTIFACTS: dict[str, tuple[tuple[str, tuple[str, ...]], ...]] = {
    "B-171": (
        (
            "crates/corelink-container/src/storage/r2_kv.rs",
            (r"derive_prefix", r"self\.tdk\.as_ref\(\)\.ok_or_else\(non_derivable_tenant_err\)"),
        ),
    ),
    "B-173": (
        ("crates/corelink-adapter-host/src/oci/server/handlers.rs", (r"MAX_UPLOAD_REQUEST_BYTES", r"collect_upload_body", r"into_data_stream", r"BlobOversized")),
        ("crates/corelink-container/src/routes/oci.rs", (r'include!\("oci/b126_m2_impl_01\.rs"\)',)),
    ),
    "B-174": (
        ("scripts/apply-d1-migrations-prod.sh", (r'if ! WRANGLER_CMD="\$WRANGLER_CMD" "\$REPO_ROOT/scripts/d1-apply-fk-parent\.sh"', r"exit 1")),
        ("scripts/d-day-migrations-apply-prod.sh", (r'if ! WRANGLER_CMD="\$WRANGLER_CMD" "\$\{REPO_ROOT\}/scripts/d1-apply-fk-parent\.sh"', r"exit 1")),
        ("scripts/d1-migration-runner.sh", (r'if ! WRANGLER_CMD=wrangler "\$REPO_ROOT/scripts/d1-apply-fk-parent\.sh"', r"exit 1")),
    ),
    "B-175": (
        ("worker/src/region-map.ts", (r"PROVISIONED_MACROS", r'"wnam"', r'"enam"', r'"weur"', r'"apac"')),
    ),
    "B-176": (
        ("crates/corelink-container/src/routes/public_attestation.rs", (r"/v1/public/attestation", r"/v1/public/keys/erasure", r"signature_ed25519")),
    ),
    "B-179": (
        ("crates/corelink-container/src/routes/ratelimit_layer.rs", (r"pub async fn rate_limit_layer", r"run_oci_velocity_gate", r"oci_bucket_key", r"RateLimitDecision::Deny429")),
    ),
    "B-180": (
        ("worker/src/durable_object_start.ts", (r"CORELINK_OCI_TOKEN_KEY", r"HUGR_OCI_TOKEN_KEY")),
    ),
}


def _text(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ClosureError(f"missing/non-regular artifact: {relative}")
    return path.read_text(encoding="utf-8")


def _executable_text(relative: str, text: str) -> str:
    """Drop whole-line shell comments before checking executable wiring."""
    if not relative.endswith(".sh"):
        return text
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))


class _ShellToken:
    __slots__ = ("value", "quoted", "operator")

    def __init__(self, value: str, quoted: bool, operator: bool) -> None:
        self.value = value
        self.quoted = quoted
        self.operator = operator


def _lex_shell(text: str) -> list[_ShellToken] | None:
    """Lex the small shell subset needed by the migration guard.

    Quoted words remain one token, comments are discarded, and control
    separators are retained so a command in ``if false; then`` can be excluded
    structurally. Unterminated quotes/escapes fail closed.
    """
    if len(text) > _MAX_LEX_SOURCE_BYTES:
        return None
    tokens: list[_ShellToken] = []
    word: list[str] = []
    quoted = False
    quote: str | None = None
    escaped = False

    def flush() -> None:
        nonlocal word, quoted
        if word:
            tokens.append(_ShellToken("".join(word), quoted, False))
            word = []
            quoted = False

    i = 0
    while i < len(text):
        ch = text[i]
        if quote is not None:
            if ch == quote:
                quote = None
            elif ch == "\\" and quote == '"' and i + 1 < len(text):
                word.append(text[i + 1])
                i += 1
            else:
                word.append(ch)
            i += 1
            continue
        if escaped:
            if ch == "\n":
                escaped = False
                i += 1
                continue
            word.append(ch)
            quoted = True
            escaped = False
            i += 1
            continue
        if ch == "\\":
            escaped = True
            i += 1
            continue
        if ch in "'\"":
            quote = ch
            quoted = True
            i += 1
            continue
        if ch == "#" and not word:
            while i < len(text) and text[i] != "\n":
                i += 1
            continue
        if ch.isspace():
            flush()
            if ch == "\n":
                tokens.append(_ShellToken("\n", False, True))
            i += 1
            continue
        if text.startswith("&&", i) or text.startswith("||", i):
            flush()
            tokens.append(_ShellToken(text[i : i + 2], False, True))
            i += 2
            continue
        if ch in ";|(){}<>!":
            flush()
            tokens.append(_ShellToken(ch, False, True))
            i += 1
            continue
        word.append(ch)
        i += 1
        if len(tokens) > _MAX_LEX_TOKENS:
            return None
    if quote is not None or escaped:
        return None
    flush()
    return tokens


def _shell_condition_is_false(tokens: list[_ShellToken], start: int, end: int) -> bool:
    """Recognize only unambiguous constant-false shell conditions."""
    condition = [
        token
        for token in tokens[start:end]
        if not token.operator or token.value not in {";", "\n"}
    ]
    if any(token.quoted for token in condition):
        return False
    values = [token.value for token in condition]
    while values and values[0] in {"[[", "[", "("}:
        values.pop(0)
    while values and values[-1] in {"]],", "]", ")"}:
        values.pop()
    return values == ["false"] or values == ["0"]


def _shell_has_run_file(text: str) -> bool:
    """Find an active ``run --file \"$TMP_SQL\"`` command.

    The control-flow model is finite and conservative: it tracks shell
    separators and ``if/while`` blocks whose condition is literally false.
    Quoted bait, echo/printf words, and commands in statically dead bodies
    cannot satisfy this predicate.
    """
    tokens = _lex_shell(text)
    if tokens is None:
        return False
    blocks: list[tuple[str, bool, bool]] = []  # kind, false condition, in body
    command_start = True
    command_words = 0
    segment_values: list[str] = []
    short_circuit_dead = False
    i = 0
    while i < len(tokens):
        token = tokens[i]
        value = token.value
        dead = short_circuit_dead or any(is_false and in_body for _, is_false, in_body in blocks)
        if not token.operator and command_start and value in {"if", "while"} and not token.quoted:
            end_word = "then" if value == "if" else "do"
            j = i + 1
            while j < len(tokens) and not (
                not tokens[j].operator and not tokens[j].quoted and tokens[j].value == end_word
            ):
                j += 1
            is_false = j < len(tokens) and _shell_condition_is_false(tokens, i + 1, j)
            blocks.append((value, is_false, False))
            command_start = True
            command_words = 0
            segment_values = []
            short_circuit_dead = False
            i += 1
            continue
        if not token.operator and command_start and not token.quoted and value in {"then", "do"}:
            if blocks:
                kind, is_false, _ = blocks[-1]
                blocks[-1] = (kind, is_false, True)
            command_start = True
            command_words = 0
            segment_values = []
            short_circuit_dead = False
            i += 1
            continue
        if not token.operator and command_start and not token.quoted and value == "else":
            if blocks:
                kind, is_false, _ = blocks[-1]
                blocks[-1] = (kind, False if is_false else is_false, True)
            command_start = True
            command_words = 0
            segment_values = []
            short_circuit_dead = False
            i += 1
            continue
        if not token.operator and command_start and not token.quoted and value == "elif":
            if blocks:
                kind, _, _ = blocks[-1]
                blocks[-1] = (kind, False, False)
            command_start = True
            command_words = 0
            segment_values = []
            short_circuit_dead = False
            i += 1
            continue
        if not token.operator and command_start and not token.quoted and value in {"fi", "done"}:
            if blocks:
                blocks.pop()
            command_start = True
            command_words = 0
            segment_values = []
            short_circuit_dead = False
            i += 1
            continue
        if token.operator and value in {";", "\n", "&&", "||", "|", "{", "}"}:
            if value == "&&" and segment_values == ["false"]:
                short_circuit_dead = True
            elif value == "||" and segment_values == ["true"]:
                short_circuit_dead = True
            elif value not in {"&&", "||"}:
                short_circuit_dead = False
            command_start = True
            command_words = 0
            segment_values = []
            i += 1
            continue
        if not token.operator:
            if command_start and not token.quoted and value == "!":
                i += 1
                continue
            assignment = (
                command_start
                and not token.quoted
                and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", value) is not None
            )
            if command_start and not assignment:
                command_start = False
                command_words += 1
                segment_values.append(value)
            elif not command_start:
                command_words += 1
                segment_values.append(value)
            if (
                not dead
                and command_words == 1
                and not token.quoted
                and value == "run"
                and i + 2 < len(tokens)
                and not tokens[i + 1].operator
                and not tokens[i + 1].quoted
                and tokens[i + 1].value == "--file"
                and not tokens[i + 2].operator
                and tokens[i + 2].value == "$TMP_SQL"
            ):
                return True
        i += 1
    return False


class _TsToken:
    __slots__ = ("kind", "value")

    def __init__(self, kind: str, value: str) -> None:
        self.kind = kind
        self.value = value


def _lex_typescript(text: str) -> list[_TsToken] | None:
    """Lex TypeScript tokens while removing comments and retaining strings."""
    if len(text) > _MAX_LEX_SOURCE_BYTES:
        return None
    tokens: list[_TsToken] = []
    i = 0
    limit = len(text)
    while i < limit:
        ch = text[i]
        if ch.isspace():
            i += 1
            continue
        if text.startswith("//", i):
            newline = text.find("\n", i + 2)
            i = limit if newline < 0 else newline + 1
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            if end < 0:
                return None
            i = end + 2
            continue
        if ch in "'\"`":
            quote = ch
            i += 1
            value: list[str] = []
            closed = False
            while i < limit:
                current = text[i]
                if current == "\\" and i + 1 < limit:
                    value.append(text[i : i + 2])
                    i += 2
                    continue
                if current == quote:
                    closed = True
                    i += 1
                    break
                value.append(current)
                i += 1
            if not closed:
                return None
            tokens.append(_TsToken("string", "".join(value)))
            if len(tokens) > _MAX_LEX_TOKENS:
                return None
            continue
        if ch.isalpha() or ch in "_$":
            start = i
            i += 1
            while i < limit and (text[i].isalnum() or text[i] in "_$"):
                i += 1
            tokens.append(_TsToken("word", text[start:i]))
            if len(tokens) > _MAX_LEX_TOKENS:
                return None
            continue
        if ch.isdigit():
            start = i
            i += 1
            while i < limit and (text[i].isalnum() or text[i] in "._"):
                i += 1
            tokens.append(_TsToken("number", text[start:i]))
            if len(tokens) > _MAX_LEX_TOKENS:
                return None
            continue
        # Multi-byte operators are kept together where they matter to the
        # control-flow recognizer; all other punctuation is one token.
        matched = next((op for op in ("===", "!==", "=>", "&&", "||", "??") if text.startswith(op, i)), None)
        if matched is not None:
            tokens.append(_TsToken("punct", matched))
            i += len(matched)
        else:
            tokens.append(_TsToken("punct", ch))
            i += 1
        if len(tokens) > _MAX_LEX_TOKENS:
            return None
    return tokens


def _ts_pairs(tokens: list[_TsToken], opening: str, closing: str) -> dict[int, int] | None:
    stack: list[int] = []
    pairs: dict[int, int] = {}
    for index, token in enumerate(tokens):
        if token.value == opening:
            stack.append(index)
        elif token.value == closing:
            if not stack:
                return None
            pairs[stack.pop()] = index
    return None if stack else pairs


def _ts_balanced(tokens: list[_TsToken]) -> bool:
    """Reject mismatched delimiter nesting before building reachability."""
    matching = {")": "(", "]": "[", "}": "{",
    }
    openings = set(matching.values())
    stack: list[str] = []
    for token in tokens:
        if token.value in openings:
            stack.append(token.value)
        elif token.value in matching:
            if not stack or stack.pop() != matching[token.value]:
                return False
    return not stack


def _ts_unreachable_ranges(tokens: list[_TsToken]) -> list[tuple[int, int]] | None:
    """Return statically dead ``if(false)`` and post-return ranges."""
    if not _ts_balanced(tokens):
        return None
    parens = _ts_pairs(tokens, "(", ")")
    braces = _ts_pairs(tokens, "{", "}")
    if parens is None or braces is None:
        return None
    ranges: list[tuple[int, int]] = []
    for index, token in enumerate(tokens):
        if token.kind != "word" or token.value not in {"if", "while"}:
            continue
        if index + 1 >= len(tokens) or tokens[index + 1].value != "(":
            continue
        end_paren = parens.get(index + 1)
        if end_paren is None:
            return None
        condition = tokens[index + 2 : end_paren]
        is_false = len(condition) == 1 and condition[0].value in {"false", "0"}
        if not is_false or end_paren + 1 >= len(tokens):
            continue
        body = end_paren + 1
        if tokens[body].value == "{":
            end_body = braces.get(body)
            if end_body is None:
                return None
            ranges.append((body + 1, end_body))
        else:
            end_body = body
            while end_body < len(tokens) and tokens[end_body].value != ";":
                end_body += 1
            if end_body >= len(tokens):
                return None
            ranges.append((body, end_body + 1))

    # A bare return/throw ends the enclosing braced block.  A return carrying
    # an expression (including ``return db.prepare(...)``) remains reachable
    # until its terminating semicolon, which is exactly the desired behavior.
    enclosing: list[int | None] = [None] * len(tokens)
    stack: list[int] = []
    for index, token in enumerate(tokens):
        if stack:
            enclosing[index] = stack[-1]
        if token.value == "{":
            stack.append(index)
        elif token.value == "}" and stack:
            stack.pop()
    for index, token in enumerate(tokens):
        if token.kind != "word" or token.value not in {"return", "throw"}:
            continue
        block = enclosing[index]
        if block is None or block not in braces:
            continue
        depth = 0
        semicolon: int | None = None
        for cursor in range(index + 1, braces[block]):
            value = tokens[cursor].value
            if value == "{":
                depth += 1
            elif value == "}" and depth:
                depth -= 1
            elif value == ";" and depth == 0:
                semicolon = cursor
                break
        if semicolon is not None:
            ranges.append((semicolon + 1, braces[block]))
    return ranges


def _ts_in_ranges(index: int, ranges: list[tuple[int, int]]) -> bool:
    return any(start <= index < end for start, end in ranges)


def _ts_call_end(tokens: list[_TsToken], open_index: int, parens: dict[int, int]) -> int | None:
    return parens.get(open_index)


def _prepared_monthly_upsert_active(text: str) -> bool:
    """Require active prepared SQL plus a bound and executed statement."""
    tokens = _lex_typescript(text)
    if tokens is None:
        return False
    parens = _ts_pairs(tokens, "(", ")")
    ranges = _ts_unreachable_ranges(tokens)
    if parens is None or ranges is None:
        return False
    prepared = False
    for index in range(len(tokens) - 3):
        if (
            tokens[index].value != "."
            or tokens[index + 1].value != "prepare"
            or tokens[index + 2].value != "("
            or _ts_in_ranges(index + 1, ranges)
        ):
            continue
        close = _ts_call_end(tokens, index + 2, parens)
        if close is None:
            return False
        args = tokens[index + 3 : close]
        literals = [token.value for token in args if token.kind == "string"]
        if not literals or not literals[0].startswith("INSERT INTO monthly_request_counts"):
            continue
        if "ON CONFLICT(tenant_id, year_month)" not in "".join(literals):
            continue
        bind = close + 1
        if bind + 3 >= len(tokens) or tokens[bind].value != "." or tokens[bind + 1].value != "bind" or tokens[bind + 2].value != "(":
            continue
        bind_close = _ts_call_end(tokens, bind + 2, parens)
        if bind_close is None or bind_close == bind + 3 or _ts_in_ranges(bind + 1, ranges):
            continue
        prepared = True
        break
    if not prepared:
        return False
    # The returned bound statement must be used by a real executor.  Bind and
    # prepare alone can be an inert factory; this call-site check ties the
    # named statement to ``.first()`` (or another D1 execution method).
    executors = {"first", "run", "all", "raw"}
    for index in range(len(tokens) - 5):
        if tokens[index].value != "monthlyRequestCountStatement":
            continue
        if index + 1 >= len(tokens) or tokens[index + 1].value != "(":
            continue
        close = parens.get(index + 1)
        if close is None or _ts_in_ranges(index, ranges):
            continue
        if close + 2 < len(tokens) and tokens[close + 1].value == "." and tokens[close + 2].value in executors:
            call_open = close + 3
            if call_open < len(tokens) and tokens[call_open].value == "<":
                angle = call_open + 1
                while angle < len(tokens) and tokens[angle].value != ">":
                    angle += 1
                call_open = angle + 1
            if call_open < len(tokens) and tokens[call_open].value == "(":
                return True
    return False


def _without_comments(text: str) -> str:
    """Mask comments while retaining string literals used as SQL evidence."""
    out: list[str] = []
    i = 0
    quote: str | None = None
    block = False
    while i < len(text):
        if block:
            if text.startswith("*/", i):
                block = False
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        if quote:
            ch = text[i]
            out.append(ch)
            if ch == "\\" and i + 1 < len(text):
                out.append(text[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if text.startswith("/*", i):
            block = True
            out.extend("  ")
            i += 2
            continue
        if text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        ch = text[i]
        if ch in "'\"`":
            quote = ch
        out.append(ch)
        i += 1
    return "".join(out)


def _without_comments_and_strings(text: str) -> str:
    """Mask comments and literals so copied witnesses cannot close a gate."""
    out: list[str] = []
    i = 0
    quote: str | None = None
    block = False
    while i < len(text):
        if block:
            if text.startswith("*/", i):
                block = False
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        if quote:
            ch = text[i]
            out.append("\n" if ch == "\n" else " ")
            if ch == "\\" and i + 1 < len(text):
                out.append("\n" if text[i + 1] == "\n" else " ")
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if text.startswith("/*", i):
            block = True
            out.extend("  ")
            i += 2
            continue
        if text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        # Rust lifetimes are code, not character literals.
        if text[i] == "'" and i + 1 < len(text) and (text[i + 1].isalpha() or text[i + 1] == "_"):
            out.append(text[i])
            i += 1
            continue
        ch = text[i]
        if ch in "'\"`":
            quote = ch
            out.append(" ")
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def _check(identifier: str, root: Path, text_override: str | None = None) -> None:
    artifact, patterns, needle = CONTRACTS[identifier]
    text = _text(root, artifact) if text_override is None else text_override
    executable = _executable_text(artifact, text)
    pattern_view = _without_comments_and_strings(executable) if identifier in {"B-172", "B-177"} else executable
    missing = [p for p in patterns if re.search(p, pattern_view, re.IGNORECASE | re.DOTALL) is None]
    if missing:
        raise ClosureError(f"{identifier}: missing executable clause {missing[0]!r} in {artifact}")
    needle_view = executable
    if identifier in {"B-171", "B-172", "B-173", "B-175", "B-177", "B-179", "B-180"}:
        needle_view = _without_comments_and_strings(executable)
    if needle not in needle_view:
        raise ClosureError(f"{identifier}: load-bearing clause {needle!r} is absent in {artifact}")
    if identifier == "B-174" and not _shell_has_run_file(executable):
        raise ClosureError(f"{identifier}: run --file invocation is not active shell code")
    if identifier == "B-178" and not _prepared_monthly_upsert_active(executable):
        raise ClosureError(f"{identifier}: monthly counter UPSERT is not active prepared SQL")
    if identifier == "B-176":
        comment_free = _without_comments(executable)
        if re.search(
            r"(?m)^\s*let\s+att_sql\s*=\s*\"INSERT OR IGNORE INTO erasure_attestations\b",
            comment_free,
        ) is None:
            raise ClosureError(f"{identifier}: attestation SQL is not the active att_sql assignment")
    for extra_artifact, extra_patterns in EXTRA_ARTIFACTS.get(identifier, ()):
        extra = _text(root, extra_artifact)
        extra_executable = _executable_text(extra_artifact, extra)
        missing_extra = [
            p for p in extra_patterns
            if re.search(p, extra_executable, re.IGNORECASE | re.DOTALL) is None
        ]
        if missing_extra:
            raise ClosureError(f"{identifier}: missing executable clause {missing_extra[0]!r} in {extra_artifact}")


def _mutation_self_test(identifier: str, root: Path) -> None:
    artifact, _, needle = CONTRACTS[identifier]
    source = _text(root, artifact)
    if needle not in source:
        raise ClosureError(f"{identifier}: mutation needle is absent ({needle!r})")
    # Remove every occurrence: several implementation comments repeat the
    # contract, and mutating only the first (often a comment) would make the
    # mutation test decorative rather than load-bearing.
    mutated = source.replace(needle, "__B171_180_MUTATION_REMOVED__")
    try:
        _check(identifier, root, mutated)
    except ClosureError:
        return
    raise ClosureError(f"{identifier}: load-bearing mutation was not detected")


def _semantic_mutation_self_test(root: Path) -> None:
    """Keep the prior semantic mutants red, including inert string bait."""
    mutations: dict[str, tuple[str, str]] = {}
    handlers = _text(root, CONTRACTS["B-172"][0])
    mutations["B-172"] = (
        CONTRACTS["B-172"][0],
        handlers.replace(
            "let bytes = axum::body::to_bytes(body, MAX_MANIFEST_BYTES)",
            "let bytes = axum::body::Body::empty()",
            1,
        ),
    )
    migration = _text(root, CONTRACTS["B-174"][0])
    mutations["B-174"] = (
        CONTRACTS["B-174"][0],
        migration.replace(
            'run --file "$TMP_SQL" >/dev/null',
            ': # run --file "$TMP_SQL" >/dev/null',
            1,
        ),
    )
    mutations["B-174-string"] = (
        CONTRACTS["B-174"][0],
        migration.replace(
            'run --file "$TMP_SQL" >/dev/null',
            "printf '%s\\n' 'run --file \"$TMP_SQL\" >/dev/null'",
            1,
        ),
    )
    attestation = _text(root, CONTRACTS["B-176"][0])
    start = attestation.index('let att_sql = "')
    end = attestation.index('";', start) + 2
    mutations["B-176"] = (
        CONTRACTS["B-176"][0],
        attestation[:start]
        + 'let att_sql = "SELECT 1"; // INSERT OR IGNORE INTO erasure_attestations'
        + attestation[end:],
    )
    build = _text(root, CONTRACTS["B-177"][0])
    mutations["B-177"] = (
        CONTRACTS["B-177"][0],
        build.replace(
            "ratelimit_layer::rate_limit_layer,",
            "noop_layer, // ratelimit_layer::rate_limit_layer",
            1,
        ),
    )
    quota = _text(root, CONTRACTS["B-178"][0])
    statement_start = quota.index("export function monthlyRequestCountStatement")
    conflict = quota.index('"ON CONFLICT(tenant_id, year_month) " +', statement_start)
    conflict_clause = '"ON CONFLICT(tenant_id, year_month) " +'
    mutations["B-178"] = (
        CONTRACTS["B-178"][0],
        quota[:conflict] + '"ON CONFLICT(noop) " +' + quota[conflict + len(conflict_clause) :],
    )
    mutations["B-178-string"] = (
        CONTRACTS["B-178"][0],
        'const bait = \'.prepare("INSERT INTO monthly_request_counts ON CONFLICT(tenant_id, year_month)");\'\n'
        + quota[:conflict]
        + '"ON CONFLICT(noop) " +'
        + quota[conflict + len(conflict_clause) :],
    )
    for identifier, (artifact, mutated) in mutations.items():
        check_id = identifier.replace("-string", "")
        try:
            _check(check_id, root, mutated)
        except ClosureError:
            continue
        raise ClosureError(f"{identifier}: inert semantic mutation was accepted")


def verify(root: Path = ROOT, identifier: str | None = None) -> dict[str, int]:
    ids = [identifier] if identifier is not None else list(CONTRACTS)
    unknown = [item for item in ids if item not in CONTRACTS]
    if unknown:
        raise ClosureError(f"unknown closure id(s): {', '.join(unknown)}")
    for item in ids:
        _check(item, root)
        _mutation_self_test(item, root)
    if identifier is None:
        _semantic_mutation_self_test(root)
    return {"closed": len(ids), "mutations": len(ids)}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--id")
    parser.add_argument("--expect", choices=("done", "open"), default="done")
    args = parser.parse_args(argv)
    try:
        if args.expect == "open":
            raise ClosureError("closure verifier is inverted: open is not a closed result")
        report = verify(Path(args.root).resolve(), args.id)
    except (ClosureError, OSError, UnicodeDecodeError) as exc:
        print(f"B-171..B-180 closure gate: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"B-171..B-180 closure gate: PASS: {report['closed']} item(s), {report['mutations']} mutation(s) red")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
