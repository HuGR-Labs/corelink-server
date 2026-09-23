#!/usr/bin/env python3
"""Fail-closed implementation contracts for B-215..B-223.

This is the inverted (positive) side of the B-101 guard.  It intentionally
checks executable scopes and ordering, not finding prose or comments.  The
``--self-test`` mutations prove that removing a load-bearing guard turns the
contract red without changing the checkout.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class ContractError(ValueError):
    """A source-specific B-215..B-223 contract is absent or weakened."""


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ContractError(f"missing or non-regular canonical artifact: {relative}")
    return path.read_text(encoding="utf-8")


def _require(text: str, marker: str, lane: str) -> None:
    if marker not in text:
        raise ContractError(f"{lane}: required executable marker missing: {marker}")


def _code(text: str) -> str:
    """Mask comments so copied prose cannot satisfy an executable contract."""
    out = list(text)
    quote: str | None = None
    block = False
    i = 0
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if block:
            if c == "*" and n == "/":
                out[i] = out[i + 1] = " "
                i += 2
                block = False
            elif c != "\n":
                out[i] = " "
                i += 1
            else:
                i += 1
            continue
        if quote:
            if c == "\\":
                i += 2
            elif c == quote:
                quote = None
                i += 1
            else:
                i += 1
            continue
        if c in "\"'`":
            quote = c
            i += 1
        elif c == "/" and n == "/":
            while i < len(text) and text[i] != "\n":
                out[i] = " "
                i += 1
        elif c == "/" and n == "*":
            out[i] = out[i + 1] = " "
            i += 2
            block = True
        else:
            i += 1
    return "".join(out)


def _code_without_literals(text: str) -> str:
    """Mask comments and JS/TS string literals while retaining code tokens."""
    out = list(text)
    quote: str | None = None
    block = False
    i = 0
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if block:
            if c == "*" and n == "/":
                out[i] = out[i + 1] = " "
                i += 2
                block = False
            else:
                if c != "\n":
                    out[i] = " "
                i += 1
            continue
        if quote:
            if c == "\\":
                out[i] = " "
                if i + 1 < len(text) and text[i + 1] != "\n":
                    out[i + 1] = " "
                i += 2
            elif c == quote:
                out[i] = " "
                quote = None
                i += 1
            else:
                if c != "\n":
                    out[i] = " "
                i += 1
            continue
        if c == "/" and n == "/":
            out[i] = out[i + 1] = " "
            i += 2
            while i < len(text) and text[i] != "\n":
                out[i] = " "
                i += 1
        elif c == "/" and n == "*":
            out[i] = out[i + 1] = " "
            i += 2
            block = True
        elif c in "\"'`":
            out[i] = " "
            quote = c
            i += 1
        else:
            i += 1
    return "".join(out)


def _js_tokens(text: str) -> list[tuple[str, str, int, int]]:
    """Return active JS/TS tokens, keeping each literal as one opaque token."""
    tokens: list[tuple[str, str, int, int]] = []
    operators = (
        "===", "!==", "...", "?.", "=>", "&&", "||", "==", "!=", "<=", ">=",
        "++", "--", "+=", "-=", "*=", "/=", "??",
    )
    i = 0
    while i < len(text):
        c = text[i]
        if c.isspace():
            i += 1
            continue
        if c == "/" and i + 1 < len(text) and text[i + 1] == "/":
            i += 2
            while i < len(text) and text[i] != "\n":
                i += 1
            continue
        if c == "/" and i + 1 < len(text) and text[i + 1] == "*":
            end = text.find("*/", i + 2)
            if end < 0:
                return tokens
            i = end + 2
            continue
        if c in "'\"`":
            quote = c
            start = i
            i += 1
            while i < len(text):
                if text[i] == "\\":
                    i += 2
                elif text[i] == quote:
                    i += 1
                    break
                else:
                    i += 1
            tokens.append(("literal", text[start:i], start, i))
            continue
        # A regex literal is also opaque.  This conservative rule is enough
        # for the lifecycle validator and prevents regex contents becoming
        # false identifier/punctuation matches.
        if c == "/":
            start = i
            i += 1
            character_class = False
            while i < len(text):
                if text[i] == "\\":
                    i += 2
                elif text[i] == "[":
                    character_class = True
                    i += 1
                elif text[i] == "]":
                    character_class = False
                    i += 1
                elif text[i] == "/" and not character_class:
                    i += 1
                    while i < len(text) and text[i].isalpha():
                        i += 1
                    break
                else:
                    i += 1
            tokens.append(("regex", text[start:i], start, i))
            continue
        if c.isalpha() or c in "_$":
            start = i
            i += 1
            while i < len(text) and (text[i].isalnum() or text[i] in "_$"):
                i += 1
            tokens.append(("word", text[start:i], start, i))
            continue
        if c.isdigit():
            start = i
            i += 1
            while i < len(text) and (text[i].isalnum() or text[i] in "._"):
                i += 1
            tokens.append(("number", text[start:i], start, i))
            continue
        operator = next((item for item in operators if text.startswith(item, i)), None)
        if operator:
            tokens.append(("operator", operator, i, i + len(operator)))
            i += len(operator)
            continue
        tokens.append(("punctuation", c, i, i + 1))
        i += 1
    return tokens


def _token_window(text: str, marker: str) -> tuple[int, int] | None:
    actual = _js_tokens(text)
    expected = [token[1] for token in _js_tokens(marker)]
    if not expected:
        return None
    values = [token[1] for token in actual]
    width = len(expected)
    for start in range(len(values) - width + 1):
        if values[start : start + width] == expected:
            return actual[start][2], actual[start + width - 1][3]
    return None


def _require_active(text: str, marker: str, lane: str) -> None:
    if _token_window(text, marker) is None:
        raise ContractError(f"{lane}: required active marker missing: {marker}")


def _function(text: str, signature: str, lane: str) -> str:
    tokens = _js_tokens(text)
    expected = [token[1] for token in _js_tokens(signature)]
    values = [token[1] for token in tokens]
    start_token = next(
        (index for index in range(len(values) - len(expected) + 1)
         if values[index : index + len(expected)] == expected),
        None,
    )
    if start_token is None:
        raise ContractError(f"{lane}: function signature missing: {signature}")
    start = tokens[start_token][2]
    opening_token = next(
        (index for index in range(start_token, len(tokens)) if tokens[index][1] == "("),
        None,
    )
    if opening_token is None:
        raise ContractError(f"{lane}: function has no parameter list: {signature}")
    depth = 0
    closing_token = None
    for index in range(opening_token, len(tokens)):
        if tokens[index][1] == "(":
            depth += 1
        elif tokens[index][1] == ")":
            depth -= 1
            if depth == 0:
                closing_token = index
                break
    if closing_token is None:
        raise ContractError(f"{lane}: function has no closing parameter list: {signature}")
    brace_token = next(
        (index for index in range(closing_token + 1, len(tokens)) if tokens[index][1] == "{"),
        None,
    )
    if brace_token is None:
        raise ContractError(f"{lane}: function has no body: {signature}")
    depth = 0
    for index in range(brace_token, len(tokens)):
        if tokens[index][1] == "{":
            depth += 1
        elif tokens[index][1] == "}":
            depth -= 1
            if depth == 0:
                return text[start : tokens[index][3]]
    raise ContractError(f"{lane}: unterminated function: {signature}")


def _prepared_sql(text: str, lane: str) -> str:
    """Extract the literal-only argument passed to the claim's prepare call."""
    tokens = _js_tokens(text)
    values = [token[1] for token in tokens]
    for index in range(len(tokens) - 2):
        if values[index : index + 3] != [".", "prepare", "("]:
            continue
        depth = 1
        end = None
        for cursor in range(index + 3, len(tokens)):
            if tokens[cursor][1] == "(":
                depth += 1
            elif tokens[cursor][1] == ")":
                depth -= 1
                if depth == 0:
                    end = cursor
                    break
        if end is None:
            continue
        argument = tokens[index + 3 : end]
        if argument and argument[-1][1] == ",":
            argument = argument[:-1]  # permit a trailing call-site comma
        if not argument or any(
            (token[0] != "literal" and token[1] != "+")
            or (token[0] == "literal" and not token[1].startswith('"'))
            for token in argument
        ):
            continue
        pieces = []
        for token in argument:
            if token[0] != "literal":
                continue
            raw = token[1]
            pieces.append(raw[1:-1].replace("\\\"", '"').replace("\\\\", "\\"))
        return "".join(pieces)
    raise ContractError(f"{lane}: claim must pass a literal-only SQL argument to prepare")


def _block_after(text: str, marker: str, lane: str) -> str:
    """Return the balanced executable block beginning at ``marker``."""
    start = text.find(marker)
    if start < 0:
        raise ContractError(f"{lane}: executable block marker missing: {marker}")
    brace = text.find("{", start + len(marker))
    if brace < 0:
        raise ContractError(f"{lane}: executable block has no body: {marker}")
    depth = 0
    for index in range(brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[brace : index + 1]
    raise ContractError(f"{lane}: unterminated executable block: {marker}")


def check_b215(root: Path) -> None:
    lane = "B-215"
    text = _read(root, "apps/signup-worker/src/webhooks/clerk.ts")
    identity = _read(root, "apps/signup-worker/src/webhooks/clerk_identity.ts")
    region = _function(identity, "export function regionFromColo(", lane)
    _require_active(region, 'if (AFR.has(c)) return "afr";', lane)
    _require_active(region, 'if (SAM.has(c)) return "sam";', lane)
    provision = _function(text, "export async function autoProvisionFromClerkEvent(", lane)
    _require_active(provision, "if (!isProvisionedMacro(region))", lane)
    _require_active(provision, "throw new UnprovisionedRegionError(region)", lane)
    guard = _token_window(provision, "if (!isProvisionedMacro(region))")
    write = _token_window(provision, "input.api.createTenant(")
    if guard is None or write is None or guard[0] > write[0]:
        raise ContractError(f"{lane}: residency rejection must precede tenant write")


def check_b216(root: Path) -> None:
    lane = "B-216"
    consumer = _code(_read(root, "apps/signup-worker/src/webhooks/dsr_consumer.ts"))
    normalizer = _function(consumer, "function normalizedDlqRequeueCount(", lane)
    _require(normalizer, "if (value === undefined) return 0;", lane)
    _require(normalizer, "return MAX_DLQ_REQUEUES;", lane)
    body = _function(consumer, "export async function handleErasureDlqBatch(", lane)
    for marker in (
        "_dlq_requeue",
        "const canRequeue = priorRequeues < MAX_DLQ_REQUEUES && !!env.DSR_QUEUE",
        "normalizedDlqRequeueCount(body._dlq_requeue)",
        "event_id: eventId",
        "exhausted: true",
        "requeue_count:",
        "event: DLQ_EVENT_NAME",
        'severity: "critical"',
        "await env.DSR_QUEUE!.send",
        "m.ack();",
        "m.retry();",
    ):
        _require(body, marker, lane)
    _require(consumer, "PAGERDUTY_ROUTING_KEY", lane)
    _require(consumer, 'const DLQ_EVENT_NAME = "dsr.erasure.dead_letter"', lane)
    _require(consumer, 'error: "http_rejected" | "transport_error"', lane)
    _require(consumer, 'return { status: "failed", error: "transport_error" };', lane)
    paging = _function(consumer, "async function pageDlqEvent(", lane)
    _require(paging, "response.status !== 202", lane)
    _require(body, 'paging.status !== "delivered"', lane)
    _require(body, 'paging.status === "failed" ? paging.error : "route_not_configured"', lane)
    _require(body, 'action: exhausted ? "delivery_exhausted" : "retry_paging"', lane)
    _require(body, 'requeue_error: "transport_error"', lane)
    for marker in (
        "const recovery = env.DSR_DLQ_RECOVERY",
        "if (!store || !recovery)",
        "await recovery.capture(",
    ):
        _require(body, marker, lane)
    capture_at = body.index("await recovery.capture(")
    if capture_at > body.index("existing = await store.find(") or capture_at > body.index("m.ack();"):
        raise ContractError(f"{lane}: recovery capture must precede receipt reads and terminal ACKs")
    index = _code(_read(root, "apps/signup-worker/src/index.ts"))
    _require(index, 'batch.queue === "corelink-dsr-erasure-dlq"', lane)
    _require(index, "await handleErasureDlqBatch(", lane)
    redrive = _code(_read(root, "apps/signup-worker/src/webhooks/dsr_dlq_redrive.ts"))
    route = _function(redrive, "export async function handleDsrDlqRedrive(", lane)
    for marker in (
        "DSR_DLQ_REDRIVE_AUTH_KEY",
        "constantTimeEqual(presented, expected)",
        "Object.keys(record).length !== 1",
        "await store.claim(eventId, actorRef, approvalRef, nowMs)",
        "deriveErasureSalt(envelope.dsr_id, env.ERASURE_SALT_KEY, env.ENVIRONMENT)",
        "tenant_id: envelope.tenant_id",
        "subject_id: envelope.tenant_id",
        "_dlq_requeue: MAX_REQUEUE_COUNT",
        "await store.prepareDispatch(eventId, Date.now())",
        "await env.DSR_QUEUE.send(message)",
        "await store.ambiguous(eventId, Date.now())",
        'return json(410, { error: "receipt_expired" })',
    ):
        _require(route, marker, lane)
    if route.index("await store.prepareDispatch(eventId, Date.now())") > route.index("await env.DSR_QUEUE.send(message)"):
        raise ContractError(f"{lane}: durable dispatch fence must precede Queue.send")
    _require(redrive, "INSERT OR IGNORE INTO dsr_dlq_redrive_envelopes", lane)
    _require(redrive, "redrive_state = 'claimed'", lane)
    _require(redrive, "claim_expires_at_ms > ?2", lane)
    _require(redrive, 'await this.transition(eventId, "submitted", nowMs);', lane)
    _require(redrive, "redrive_state = 'ambiguous'", lane)
    _require(redrive, "DELETE FROM dsr_dlq_redrive_envelopes", lane)
    _require(redrive, "DELETE FROM dsr_dlq_redrive_audit", lane)
    migration = _read(root, "migrations/d1/0144_dsr_dlq_redrive_authority.sql")
    for marker in (
        "CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_envelopes",
        "CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_audit",
        "CREATE TRIGGER IF NOT EXISTS trg_dsr_dlq_redrive_audit_transition",
        "'captured', 'ready', 'closed', 'claimed', 'submitted', 'ambiguous'",
        "requeue_count IN (0, 1)",
        "transition IN ('claimed', 'submitted', 'ambiguous')",
    ):
        _require(migration, marker, lane)
    for forbidden in ("recovery_payload_json", "clerk_user_id", "PAGERDUTY_ROUTING_KEY"):
        if forbidden in redrive or forbidden in migration:
            raise ContractError(f"{lane}: forbidden recovery persistence marker present: {forbidden}")
    _require(index, 'url.pathname === "/internal/dsr/dlq/redrive"', lane)
    _require(index, "runDsrDlqRedriveCleanup", lane)
    redrive_test = _read(root, "apps/signup-worker/tests/dsr_dlq_redrive.test.ts")
    for marker in (
        "shared-secret substitute",
        "substitute a tenant",
        "submits exactly once",
        "returns expired",
        "ambiguous outcome",
        "stale claim ambiguous",
        "already claimed receipt",
        "claim lease expires",
        "bounded envelope fields",
    ):
        _require(redrive_test, marker, lane)
    consumer_test = _read(root, "apps/signup-worker/tests/dsr_consumer.test.ts")
    _require(consumer_test, "no durable recovery envelope authority", lane)


def check_b217(root: Path) -> None:
    lane = "B-217"
    text = _read(root, "apps/signup-worker/src/webhooks/clerk.ts")
    erasure = _read(root, "apps/signup-worker/src/webhooks/clerk_erasure.ts")
    validator = _function(erasure, "export function isValidClerkUserId(", lane)
    _require_active(validator, "value: unknown", lane)
    _require_active(validator, "/^user_[A-Za-z0-9_-]{1,128}$/", lane)
    body = _function(text, "export async function handleClerkWebhook(", lane)
    _require_active(body, "!isValidClerkUserId(parsed.data?.id)", lane)
    _require_active(body, 'new Response("invalid_clerk_user_id", { status: 400 })', lane)
    check = _token_window(body, "!isValidClerkUserId(parsed.data?.id)")
    dispatch = _token_window(body, "return handleUserDeleted(")
    if check is None or dispatch is None or check[0] > dispatch[0]:
        raise ContractError(f"{lane}: lifecycle id validation must precede dispatch")


def check_b218(root: Path) -> None:
    lane = "B-218"
    clerk = _code_without_literals(_read(root, "apps/signup-worker/src/webhooks/clerk.ts"))
    erasure = _code_without_literals(_read(root, "apps/signup-worker/src/webhooks/clerk_erasure.ts"))
    auth = _code_without_literals(_read(root, "worker/src/lib/internal_auth.ts"))
    _require(erasure, "export const MIN_INTERNAL_AUTH_KEY_LEN = 32;", lane)
    _require(clerk, "MIN_INTERNAL_AUTH_KEY_LEN", lane)
    _require(auth, "const MIN_INTERNAL_AUTH_KEY_LEN = 32;", lane)
    _require(clerk, "preflightAuthKey.length < MIN_INTERNAL_AUTH_KEY_LEN", lane)
    _require(clerk, "internalAuthKey.length < MIN_INTERNAL_AUTH_KEY_LEN", lane)
    _require(auth, "specific.length >= MIN_INTERNAL_AUTH_KEY_LEN", lane)
    _require(auth, "shared && shared.length >= MIN_INTERNAL_AUTH_KEY_LEN", lane)
    if "length < 16" in clerk or "length < 16" in auth:
        raise ContractError(f"{lane}: obsolete 16-character floor remains")


def check_b219(root: Path) -> None:
    lane = "B-219"
    text = _code(_read(root, "crates/corelink-erasure-attestation/src/verify.rs"))
    body = _function(text, "pub fn verify_attestation_signature(", lane)
    for marker in (
        "serde_jcs::to_string(&attestation.payload)",
        "recomputed != attestation.canonical_payload_jcs",
        "payload does not match signed canonical bytes",
        ".verify(attestation.canonical_payload_jcs.as_bytes(), &signature)",
    ):
        _require(body, marker, lane)
    compare = body.find("recomputed != attestation.canonical_payload_jcs")
    signature = body.find(".verify(attestation.canonical_payload_jcs.as_bytes(), &signature)")
    if compare < 0 or signature < 0 or compare > signature:
        raise ContractError(f"{lane}: payload binding must precede signature verification")


def check_b220(root: Path) -> None:
    lane = "B-220"
    text = _code(_read(root, "crates/corelink-container/src/auth_tenant.rs"))
    body = _block_after(
        text,
        "impl<S: Send + Sync> FromRequestParts<S> for AuthTenant",
        lane,
    )
    _require(body, "is_reserved_sentinel(raw)", lane)
    _require(body, "is_canonical_tenant_id(raw)", lane)
    _require(body, "Ok(AuthTenant(raw.to_owned()))", lane)
    # This extractor receives an already authenticated Worker result.  It must
    # not invent a second secret comparison or transform the tenant key: the
    # exact raw value is the behavioral zero-padding invariant.
    forbidden = ("timing_safe", "ct_eq", "constant_time", "pad16(", "zero_pad(")
    present = [marker for marker in forbidden if marker in body]
    if present:
        raise ContractError(f"{lane}: extractor contains forbidden secret/padding behavior: {present[0]}")


def check_b221(root: Path) -> None:
    lane = "B-221"
    text = _code(_read(root, "worker/src/lib/session_exchange.ts"))
    body = _function(text, "export async function checkMintThrottle(", lane)
    for marker in (
        "windowStartMs: number",
        "const _inMemoryMintCounts = new Map<string, InMemoryMintState>()",
        "_inMemoryMintCounts.delete(principalId);",
        "previous.windowStartMs < MINT_THROTTLE_WINDOW_MS",
        "const MAX_IN_MEMORY_MINT_ENTRIES = 50_000;",
        "_inMemoryMintCounts.keys().next().value",
    ):
        _require(text if marker.startswith(("windowStart", "const _in", "const MAX_")) else body, marker, lane)
    d1_reset = body.find("_inMemoryMintCounts.delete(principalId);")
    row_branch = body.find("if (row === null)")
    over_limit = body.find("if (row.count > maxPerWindow)")
    if min(d1_reset, row_branch, over_limit) < 0 or not (d1_reset < row_branch < over_limit):
        raise ContractError(f"{lane}: healthy D1 must clear local state before all branches")


def check_b222(root: Path) -> None:
    lane = "B-222"
    text = _read(root, "apps/signup-worker/src/webhooks/clerk.ts")
    erasure = _read(root, "apps/signup-worker/src/webhooks/clerk_erasure.ts")
    claim = _function(erasure, "export async function claimClerkProvision(", lane)
    sql = _prepared_sql(claim, lane)
    for marker in (
        "INSERT INTO clerk_provisioning_lock",
        "ON CONFLICT(clerk_user_id) DO UPDATE SET",
        "state = 'in_progress'",
        "state = 'complete'",
    ):
        if marker not in sql:
            raise ContractError(f"{lane}: prepared SQL is missing required clause: {marker}")
    _require_active(claim, "db.prepare(", lane)
    _require_active(claim, ".bind(clerkUserId, nowMs + CLERK_PROVISION_LEASE_MS).run(", lane)
    _require_active(text, "await claimClerkProvision(env.CONFIG_DB, event.data.id)", lane)
    _require_active(text, "await completeClerkProvision(env.CONFIG_DB, event.data.id)", lane)
    migration = _read(root, "migrations/d1/0113_clerk_provisioning_lock.sql")
    _require(migration, "PRIMARY KEY", lane)
    _require(migration, "lease_until_ms", lane)


def check_b223(root: Path) -> None:
    lane = "B-223"
    shared = _code(_read(root, "crates/corelink-adapter-host/src/upstream_ssrf.rs"))
    _require(shared, "pub fn ssrf_safe_redirect_policy(max: usize)", lane)
    for marker in (
        "host_is_internal_ip(host)",
        "attempt.stop()",
        "v6.to_ipv4().is_some_and(ipv4_is_internal)",
        "a == 100 && (b & 0xc0) == 0x40",
    ):
        _require(shared, marker, lane)
    for relative in (
        "crates/corelink-adapter-host/src/npm/upstream.rs",
        "crates/corelink-adapter-host/src/pip/upstream.rs",
        "crates/corelink-adapter-host/src/brew/upstream.rs",
    ):
        text = _code(_read(root, relative))
        _require(text, "ssrf_safe_redirect_policy(", lane)
    tests = _read(root, "crates/corelink-adapter-host/src/upstream_ssrf.rs")
    _require(tests, "guarded_client_follows_public_3xx_then_blocks_mapped_internal_hop", lane)


CHECKS = {
    "B-215": check_b215,
    "B-216": check_b216,
    "B-217": check_b217,
    "B-218": check_b218,
    "B-219": check_b219,
    "B-220": check_b220,
    "B-221": check_b221,
    "B-222": check_b222,
    "B-223": check_b223,
}


def verify(root: Path = ROOT, lane: str | None = None) -> dict[str, str]:
    selected = (lane,) if lane else tuple(CHECKS)
    for item in selected:
        if item not in CHECKS:
            raise ContractError(f"unknown lane: {item}")
        CHECKS[item](root)
    return {item: "pass" for item in selected}


def self_test(root: Path = ROOT) -> None:
    """Remove one load-bearing marker per lane, including bait variants."""
    mutations = {
        "B-215": ("if (AFR.has(c)) return \"afr\";", "if (AFR.has(c)) return \"enam\";", "apps/signup-worker/src/webhooks/clerk_identity.ts"),
        "B-216": ('const DLQ_EVENT_NAME = "dsr.erasure.dead_letter"', 'const DLQ_EVENT_NAME = "dsr.erasure.removed"', "apps/signup-worker/src/webhooks/dsr_consumer.ts"),
        "B-217": ("!isValidClerkUserId(parsed.data?.id)", "false", "apps/signup-worker/src/webhooks/clerk.ts"),
        "B-218": ("export const MIN_INTERNAL_AUTH_KEY_LEN = 32;", "export const MIN_INTERNAL_AUTH_KEY_LEN = 16;", "apps/signup-worker/src/webhooks/clerk_erasure.ts"),
        "B-219": ("recomputed != attestation.canonical_payload_jcs", "recomputed == attestation.canonical_payload_jcs", "crates/corelink-erasure-attestation/src/verify.rs"),
        "B-220": ("Ok(AuthTenant(raw.to_owned()))", "Ok(AuthTenant(pad16(raw)))", "crates/corelink-container/src/auth_tenant.rs"),
        "B-221": ("const MAX_IN_MEMORY_MINT_ENTRIES = 50_000;", "const REMOVED_MINT_ENTRIES = 50_000;", "worker/src/lib/session_exchange.ts"),
        "B-222": ("ON CONFLICT(clerk_user_id) DO UPDATE SET", "ON CONFLICT_REMOVED", "apps/signup-worker/src/webhooks/clerk_erasure.ts"),
        "B-223": ("ssrf_safe_redirect_policy(", "removed_redirect_policy(", "crates/corelink-adapter-host/src/npm/upstream.rs"),
    }
    paths_by_lane = {
        "B-215": ["apps/signup-worker/src/webhooks/clerk.ts", "apps/signup-worker/src/webhooks/clerk_identity.ts"],
        "B-216": [
            "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            "apps/signup-worker/src/webhooks/dsr_dlq_redrive.ts",
            "apps/signup-worker/src/index.ts",
            "migrations/d1/0144_dsr_dlq_redrive_authority.sql",
            "apps/signup-worker/tests/dsr_dlq_redrive.test.ts",
            "apps/signup-worker/tests/dsr_consumer.test.ts",
        ],
        "B-217": ["apps/signup-worker/src/webhooks/clerk.ts", "apps/signup-worker/src/webhooks/clerk_erasure.ts"],
        "B-218": ["apps/signup-worker/src/webhooks/clerk.ts", "apps/signup-worker/src/webhooks/clerk_erasure.ts", "worker/src/lib/internal_auth.ts"],
        "B-219": ["crates/corelink-erasure-attestation/src/verify.rs"],
        "B-220": ["crates/corelink-container/src/auth_tenant.rs"],
        "B-221": ["worker/src/lib/session_exchange.ts"],
        "B-222": ["apps/signup-worker/src/webhooks/clerk.ts", "apps/signup-worker/src/webhooks/clerk_erasure.ts", "migrations/d1/0113_clerk_provisioning_lock.sql"],
        "B-223": ["crates/corelink-adapter-host/src/upstream_ssrf.rs", "crates/corelink-adapter-host/src/npm/upstream.rs", "crates/corelink-adapter-host/src/pip/upstream.rs", "crates/corelink-adapter-host/src/brew/upstream.rs"],
    }
    cases = [(lane, lane, *spec, "") for lane, spec in mutations.items()]
    cases.extend(
        (
            label,
            lane,
            needle,
            replacement,
            relative,
            suffix,
        )
        for label, lane, needle, replacement, relative, suffix in (
            (
                "B-215-comment-bait",
                "B-215",
                'if (AFR.has(c)) return "afr";',
                'if (AFR.has(c)) return "enam";',
                "apps/signup-worker/src/webhooks/clerk_identity.ts",
                '\n// if (AFR.has(c)) return "afr";\n',
            ),
            (
                "B-215-string-bait",
                "B-215",
                'if (AFR.has(c)) return "afr";',
                'if (AFR.has(c)) return "enam";',
                "apps/signup-worker/src/webhooks/clerk_identity.ts",
                '\nconst B215_STRING_BAIT = \'if (AFR.has(c)) return "afr";\';\n',
            ),
            (
                "B-217-comment-bait",
                "B-217",
                "!isValidClerkUserId(parsed.data?.id)",
                "false",
                "apps/signup-worker/src/webhooks/clerk.ts",
                "\n// !isValidClerkUserId(parsed.data?.id)\n",
            ),
            (
                "B-217-string-bait",
                "B-217",
                "!isValidClerkUserId(parsed.data?.id)",
                "false",
                "apps/signup-worker/src/webhooks/clerk.ts",
                '\nconst B217_STRING_BAIT = "!isValidClerkUserId(parsed.data?.id)";\n',
            ),
            (
                "B-218-comment-bait",
                "B-218",
                "export const MIN_INTERNAL_AUTH_KEY_LEN = 32;",
                "export const MIN_INTERNAL_AUTH_KEY_LEN = 16;",
                "apps/signup-worker/src/webhooks/clerk_erasure.ts",
                "\n// export const MIN_INTERNAL_AUTH_KEY_LEN = 32;\n",
            ),
            (
                "B-218-string-bait",
                "B-218",
                "export const MIN_INTERNAL_AUTH_KEY_LEN = 32;",
                "export const MIN_INTERNAL_AUTH_KEY_LEN = 16;",
                "apps/signup-worker/src/webhooks/clerk_erasure.ts",
                '\nconst B218_STRING_BAIT = "export const MIN_INTERNAL_AUTH_KEY_LEN = 32;";\n',
            ),
            (
                "B-222-comment-bait",
                "B-222",
                "ON CONFLICT(clerk_user_id) DO UPDATE SET",
                "ON CONFLICT_REMOVED",
                "apps/signup-worker/src/webhooks/clerk_erasure.ts",
                "\n// ON CONFLICT(clerk_user_id) DO UPDATE SET\n",
            ),
            (
                "B-222-string-bait",
                "B-222",
                "ON CONFLICT(clerk_user_id) DO UPDATE SET",
                "ON CONFLICT_REMOVED",
                "apps/signup-worker/src/webhooks/clerk_erasure.ts",
                '\nconst B222_STRING_BAIT = "ON CONFLICT(clerk_user_id) DO UPDATE SET";\n',
            ),
        )
    )
    # Explicit no-op mutations exercise the control/data edges, rather than
    # merely changing a nearby spelling.  Keep these separate from the
    # comment/string suffix cases so the fixture remains syntactically local.
    cases.extend(
        (
            label,
            lane,
            needle,
            replacement,
            relative,
            "",
        )
        for label, lane, needle, replacement, relative in (
            (
                "B-215-no-op",
                "B-215",
                "throw new UnprovisionedRegionError(region);",
                "void region;",
                "apps/signup-worker/src/webhooks/clerk.ts",
            ),
            (
                "B-217-no-op",
                "B-217",
                "!isValidClerkUserId(parsed.data?.id)",
                "false",
                "apps/signup-worker/src/webhooks/clerk.ts",
            ),
            (
                "B-222-no-op",
                "B-222",
                ".prepare(",
                ".noop(",
                "apps/signup-worker/src/webhooks/clerk_erasure.ts",
            ),
            (
                "B-216-normalizer-removal",
                "B-216",
                "function normalizedDlqRequeueCount(",
                "function removedDlqRequeueCount(",
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
            (
                "B-216-malformed-marker-open",
                "B-216",
                "return MAX_DLQ_REQUEUES;",
                "return 0;",
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
            (
                "B-216-paging-error-leak",
                "B-216",
                'return { status: "failed", error: "transport_error" };',
                'return { status: "failed", error: String("upstream") };',
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
            (
                "B-216-accept-any-2xx",
                "B-216",
                "response.status !== 202",
                "!response.ok",
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
            (
                "B-216-missing-route-open",
                "B-216",
                'paging.status !== "delivered"',
                'paging.status === "failed"',
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
            (
                "B-216-requeue-error-leak",
                "B-216",
                'requeue_error: "transport_error"',
                "requeue_error: String(err)",
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
            (
                "B-216-redrive-dedicated-key-removal",
                "B-216",
                "DSR_DLQ_REDRIVE_AUTH_KEY",
                "DSR_DLQ_SHARED_AUTH_KEY",
                "apps/signup-worker/src/webhooks/dsr_dlq_redrive.ts",
            ),
            (
                "B-216-redrive-atomic-claim-removal",
                "B-216",
                "redrive_state = 'claimed'",
                "redrive_state = 'pending'",
                "apps/signup-worker/src/webhooks/dsr_dlq_redrive.ts",
            ),
            (
                "B-216-redrive-dispatch-fence-removal",
                "B-216",
                "await store.prepareDispatch(eventId, Date.now())",
                "await store.dispatchRemoved(eventId, Date.now())",
                "apps/signup-worker/src/webhooks/dsr_dlq_redrive.ts",
            ),
            (
                "B-216-recovery-capture-removal",
                "B-216",
                "await recovery.capture(",
                "await recovery.captureRemoved(",
                "apps/signup-worker/src/webhooks/dsr_consumer.ts",
            ),
        )
    )
    in_function_baits = (
        (
            "B-215-in-function-string-bait",
            "B-215",
            'if (AFR.has(c)) return "afr";',
            'if (AFR.has(c)) return "enam";',
            "apps/signup-worker/src/webhooks/clerk_identity.ts",
            'if (AFR.has(c)) return "enam";',
            '  const B215_STRING_BAIT = \'if (AFR.has(c)) return "afr";\';\n',
        ),
        (
            "B-217-in-function-string-bait",
            "B-217",
            "!isValidClerkUserId(parsed.data?.id)",
            "false",
            "apps/signup-worker/src/webhooks/clerk.ts",
            '  if (request.method !== "POST") {',
            '  const B217_STRING_BAIT = "!isValidClerkUserId(parsed.data?.id)";\n',
        ),
        (
            "B-222-in-function-string-bait",
            "B-222",
            "ON CONFLICT(clerk_user_id) DO UPDATE SET",
            "ON CONFLICT_REMOVED",
            "apps/signup-worker/src/webhooks/clerk_erasure.ts",
            "    .run();",
            '  const B222_STRING_BAIT = "ON CONFLICT(clerk_user_id) DO UPDATE SET";\n',
        ),
    )
    with tempfile.TemporaryDirectory(prefix="b215-b223-contract-") as tmp:
        temp_root = Path(tmp)
        for label, lane, needle, replacement, relative, suffix in cases:
            paths = paths_by_lane[lane]
            for item in paths:
                destination = temp_root / item
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(_read(root, item), encoding="utf-8")
            destination = temp_root / relative
            mutated = destination.read_text(encoding="utf-8").replace(needle, replacement, 1)
            destination.write_text(mutated + suffix, encoding="utf-8")
            try:
                CHECKS[lane](temp_root)
            except ContractError:
                continue
            raise ContractError(f"{label}: adversarial mutation did not turn the contract red")
        for label, lane, needle, replacement, relative, anchor, bait in in_function_baits:
            paths = paths_by_lane[lane]
            for item in paths:
                destination = temp_root / item
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(_read(root, item), encoding="utf-8")
            destination = temp_root / relative
            original = destination.read_text(encoding="utf-8")
            mutation_at = original.find(needle)
            if mutation_at < 0:
                raise ContractError(f"{label}: mutation marker missing: {needle}")
            mutated = original[:mutation_at] + replacement + original[mutation_at + len(needle) :]
            anchor_at = mutated.find(anchor)
            if anchor_at < 0:
                raise ContractError(f"{label}: in-function insertion anchor missing: {anchor}")
            line_end = mutated.find("\n", anchor_at)
            if line_end < 0:
                raise ContractError(f"{label}: in-function insertion anchor has no line ending")
            mutated = mutated[: line_end + 1] + bait + mutated[line_end + 1 :]
            destination.write_text(mutated, encoding="utf-8")
            try:
                CHECKS[lane](temp_root)
            except ContractError:
                continue
            raise ContractError(f"{label}: adversarial mutation did not turn the contract red")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--id", choices=tuple(CHECKS))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = verify(ROOT, args.id)
        if args.self_test:
            self_test(ROOT)
    except (ContractError, OSError, UnicodeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    print("PASS: " + ", ".join(result))
    if args.self_test:
        print("PASS: adversarial mutations rejected for every lane")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
