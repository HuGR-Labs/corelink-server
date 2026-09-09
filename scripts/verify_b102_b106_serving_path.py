#!/usr/bin/env python3
"""Fail-closed source guard for the B-102/B-106 serving-path contract.

The production latency claims remain owner-only measurements.  This gate only
protects the already-landed bounded path from silently losing its cache,
single-flight, replica fallback, or write-behind wiring.
"""

from __future__ import annotations

from pathlib import Path


Token = tuple[str, str]


ROOT = Path(__file__).resolve().parents[1]
CACHE = "worker/src/lib/pat_verify_cache.ts"
FETCH = "worker/src/index_fetch.ts"
# Authentication was split out of index_auth.ts.  Keep both sides of the
# split in the census: the implementation carries the serving path while the
# facade must continue exporting it from the public module.
AUTH = "worker/src/index_auth_verify.ts"
AUTH_FACADE = "worker/src/index_auth.ts"
QUOTA = "worker/src/index_quota_impl.ts"
QUOTA_FACADE = "worker/src/index_quota_stage.ts"


class VerificationError(RuntimeError):
    pass


def read(path: str) -> str:
    try:
        return (ROOT / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"required serving-path source is unreadable: {path}: {exc}") from exc


def _ts_tokens(source: str) -> list[Token]:
    """Lex the TS subset used by the serving-path assertions.

    Comments and quoted/template literals are consumed as opaque regions, so
    a marker copied into documentation or a string cannot satisfy a source
    contract. Identifiers, numbers, and punctuation remain separate tokens,
    which permits exact active-code sequence checks without a build.
    """

    tokens: list[Token] = []
    index = 0
    length = len(source)
    while index < length:
        char = source[index]
        if char.isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = length if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            end = source.find("*/", index + 2)
            if end < 0:
                raise VerificationError("TypeScript source has an unterminated block comment")
            index = end + 2
            continue
        if char in {'"', "'", "`"}:
            quote = char
            start = index + 1
            index += 1
            escaped = False
            while index < length:
                current = source[index]
                if escaped:
                    escaped = False
                elif current == "\\":
                    escaped = True
                elif current == quote:
                    index += 1
                    break
                index += 1
            else:
                raise VerificationError("TypeScript source has an unterminated string literal")
            tokens.append(("string", source[start : index - 1]))
            continue
        if char.isalpha() or char in {"_", "$"}:
            end = index + 1
            while end < length and (source[end].isalnum() or source[end] in {"_", "$"}):
                end += 1
            tokens.append(("ident", source[index:end]))
            index = end
            continue
        if char.isdigit():
            end = index + 1
            while end < length and (source[end].isdigit() or source[end] == "_"):
                end += 1
            tokens.append(("number", source[index:end]))
            index = end
            continue
        tokens.append(("punct", char))
        index += 1
    return tokens


def _has_active_sequence(source_tokens: list[Token], marker: str) -> bool:
    expected = _ts_tokens(marker)
    if not expected:
        raise VerificationError("empty TypeScript source marker")
    width = len(expected)
    return any(source_tokens[index : index + width] == expected for index in range(len(source_tokens) - width + 1))


def _active_constant(source_tokens: list[Token], name: str) -> int | None:
    for index in range(len(source_tokens) - 4):
        prefix = source_tokens[index : index + 2]
        if prefix not in (
            [("ident", "const"), ("ident", name)],
            [("ident", "export"), ("ident", "const")],
        ):
            continue
        if prefix == [("ident", "export"), ("ident", "const")]:
            if index + 2 >= len(source_tokens) or source_tokens[index + 2] != ("ident", name):
                continue
            value_index = index + 4
        else:
            value_index = index + 3
        if value_index >= len(source_tokens) or source_tokens[value_index][0] != "number":
            continue
        if value_index + 1 >= len(source_tokens) or source_tokens[value_index + 1] != ("punct", ";"):
            continue
        return int(source_tokens[value_index][1].replace("_", ""))
    return None


def verify() -> None:
    cache = read(CACHE)
    fetch = read(FETCH)
    auth = read(AUTH)
    auth_facade = read(AUTH_FACADE)
    quota = read(QUOTA)
    quota_facade = read(QUOTA_FACADE)
    sources = {
        CACHE: cache,
        FETCH: fetch,
        AUTH: auth,
        AUTH_FACADE: auth_facade,
        QUOTA: quota,
        QUOTA_FACADE: quota_facade,
    }
    tokens = {path: _ts_tokens(source) for path, source in sources.items()}
    required = {
        CACHE: (
            "export async function verifyPatRowCached(",
            "const inflight = new Map<string, Promise<PatVerifyResult>>();",
            "inflight.set(tokenId, flight);",
            "inflight.delete(tokenId);",
            "export const PAT_VERIFY_CACHE_TTL_MS =",
            "const KV_PAT_ROW_TTL_S =",
            'const PAT_ROW_SQL = "SELECT tenant_id, expires_ms, scope, runner_job_ac_key, find_only FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL LIMIT 1";',
            "if (opts.waitUntil)",
            "opts.waitUntil(putPromise);",
        ),
        FETCH: (
            'import { resolveRequestId } from "./index_auth.js";',
            'import { authenticateRequest } from "./index_auth_stage.js";',
            'import { enforceQuota } from "./index_quota_stage.js";',
        ),
        AUTH: (
            'env.CONFIG_DB.withSession("first-unconstrained")',
            "verifyPatRowCached(readSession, parsed.tokenId, {",
            "primaryDb: env.CONFIG_DB",
            "...(kvBinding ? { kv: kvBinding } : {})",
            "...(waitUntil ? { waitUntil } : {})",
        ),
        AUTH_FACADE: (
            'export { extractAuth } from "./index_auth_verify.js";',
        ),
        QUOTA: (
            "const isStorageMutating = request.method === \"PUT\" || request.method === \"POST\";",
            "const asyncMeterEligible =",
            "meter && !isStorageMutating && !quotaTier.d1Error",
            "runQuotaBatch(env.CONFIG_DB, resolvedTenantId, quotaTier, {",
            "isMutating: isStorageMutating",
        ),
        QUOTA_FACADE: (
            'export { enforceQuota } from "./index_quota_impl.js";',
        ),
    }
    problems = [
        f"{path} is missing required serving-path marker: {marker}"
        for path, markers in required.items()
        for marker in markers
        if not _has_active_sequence(tokens[path], marker)
    ]
    if problems:
        raise VerificationError("\n".join(problems))
    for name, expected in (("PAT_VERIFY_CACHE_TTL_MS", 5_000), ("KV_PAT_ROW_TTL_S", 30)):
        value = _active_constant(tokens[CACHE], name)
        if value is None:
            raise VerificationError(f"{CACHE} has no numeric {name} constant")
        if value != expected:
            raise VerificationError(f"{CACHE} {name} must remain exactly {expected}")


def main() -> int:
    verify()
    print("verify_b102_b106_serving_path: OK (bounded source contract; production timings remain open)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        raise SystemExit(f"FAIL: {error}")
