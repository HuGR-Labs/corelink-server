/** PAT authentication and D1 verification for the edge worker. */

import type { Env } from "./index_common.js";
import type { AuthResult } from "./index_auth_policy.js";
import { isValidPatSigningKeyHex } from "./lib/pat_signing_key.js";
import { isPatExpiryLive } from "./lib/pat_expiry.js";
import { verifyPatRowCached, type KvReader } from "./lib/pat_verify_cache.js";
import { isTenantSuspended } from "./lib/tenant_suspend_gate.js";
import { extractBasicAuthForAuthStage } from "./index_auth.js";
import { base64url, parsePat, verifyPatHmacMulti } from "./index_auth_pat.js";

export async function extractAuth(
  request: Request,
  env: Env,
  // pip/uv natively emit ONLY URL-embedded HTTP Basic (`https://hugr:<PAT>@host/…`
  // → `Authorization: Basic base64("hugr:<PAT>")`) and NEVER `Authorization:
  // Bearer` — so the pip adapter surface is unusable unless the Worker accepts
  // Basic. When `allowBasicAuth` is true (set ONLY for the `pip` adapter route by
  // the sole caller), a Basic credential is decoded and its PASSWORD is taken as
  // the PAT, then verified through the EXACT same HMAC + D1 path as a Bearer PAT
  // (username is an ignored label — `hugr`). Defaults to false so every other
  // route keeps rejecting non-Bearer schemes with `invalid_scheme` (no bypass:
  // native CAS/AC, browser, and all other adapters are unchanged).
  allowBasicAuth = false,
  // `ctx.waitUntil`, forwarded to the PAT-verify KV write-behind so it survives
  // the response (a bare fire-and-forget kv.put is cancelled → KV never warms).
  waitUntil?: (p: Promise<unknown>) => void,
): Promise<AuthResult> {
  // F18: PAT_SIGNING_KEY is the SOLE possession gate for the native plane (F3/F17).
  // Fail CLOSED and LOUD when it is absent or too short — never silently skip the
  // HMAC check. The 32-byte minimum mirrors the NIST 128-bit floor for symmetric
  // auth secrets. The caller maps this reason to HTTP 503 so operators are alerted.
  const signingKeyRaw = env.PAT_SIGNING_KEY;
  if (!signingKeyRaw || signingKeyRaw.length === 0) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_absent",
        severity: "CRITICAL",
        message: "PAT_SIGNING_KEY is unset — extractAuth failing closed (503). Provision the secret and redeploy.",
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }
  // WP-F2 (MED-1): the PRIMARY key gets the SAME full predicate as its
  // rotation siblings (shared helper — they cannot diverge again). The old
  // length-only gate let a 64+-char NON-hex key through: hexDecode() returned
  // null on every request, HMAC failed silently and ALL legitimate clients
  // took 401s with zero operational signal. Now: fail CLOSED + LOUD (503).
  if (!isValidPatSigningKeyHex(signingKeyRaw)) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_malformed",
        severity: "CRITICAL",
        message: `PAT_SIGNING_KEY is PRESENT but is not a valid signing key (need an even-length all-hex string of >= 64 chars / >= 32 bytes; got hex length ${signingKeyRaw.length}) — failing closed (503). Fix or unset the key and redeploy.`,
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }

  const authHeader = request.headers.get("authorization");
  if (authHeader === null || authHeader.length === 0) {
    return { ok: false, reason: "missing_authorization_header" };
  }

  const bearerPrefix = "Bearer ";
  const basicPrefix = "Basic ";
  let token: string;
  if (authHeader.startsWith(bearerPrefix)) {
    token = authHeader.slice(bearerPrefix.length).trim();
  } else if (allowBasicAuth && authHeader.startsWith(basicPrefix)) {
    // pip-only Basic path (see `allowBasicAuth` doc above). Decode the credential
    // and take the PASSWORD as the PAT. Malformed Basic (not base64, non-UTF8, no
    // `:`, or empty password) is rejected with the SAME 401 shape as an
    // unsupported scheme — never a bypass. From here the token flows through the
    // identical length/char/format/HMAC/D1 checks as a Bearer PAT.
    const basicPat = extractBasicAuthForAuthStage(authHeader.slice(basicPrefix.length).trim());
    if (basicPat === null) {
      return { ok: false, reason: "invalid_scheme" };
    }
    token = basicPat;
  } else {
    return { ok: false, reason: "invalid_scheme" };
  }
  if (token.length < 32 || token.length > 256) {
    return { ok: false, reason: "invalid_token_length" };
  }

  // Validate printable ASCII (0x21–0x7E) — no spaces, no control chars.
  // Scan all bytes before early-exit to avoid byte-position timing oracle.
  const enc = new TextEncoder();
  const tokenBytes = enc.encode(token);
  let invalid = 0;
  for (const b of tokenBytes) {
    invalid |= (b < 0x21 || b > 0x7e) ? 1 : 0;
  }
  if (invalid !== 0) {
    return { ok: false, reason: "invalid_token_chars" };
  }

  // Derive a safe log prefix (first 6 chars of base64url of SHA-256 of token).
  // Deterministic per token value but non-reversible.
  const hashBuf = await crypto.subtle.digest("SHA-256", tokenBytes);
  const hashArr = new Uint8Array(hashBuf);
  const tokenPrefix = base64url(hashArr).slice(0, 6);

  // ── Step 2: Parse canonical PAT format ───────────────────────────────────
  // corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
  // Total length: 95 (env=2: ci/ro) or 96 (env=3: pat).
  const parsed = parsePat(token);
  if (parsed === null) {
    // Not a CoreLink PAT format. Reject — we only accept canonical PATs.
    return { ok: false, reason: "invalid_pat_format" };
  }

  // ── Step 3: HMAC-SHA256 fast-fail (PAT_SIGNING_KEY — always required) ───
  // Verify the hmac_sig segment before touching D1. The key is guaranteed
  // non-empty and ≥ 32 decoded bytes by the guard at the top of extractAuth.
  // Pre-image: `<token_id>.<random_secret>` (the same preimage used by the
  // Rust verify crate).
  //
  // ── Possession model (security: H2 — explicit engineering DECISION) ───────
  // This HMAC-SHA256 check IS the sole cryptographic possession gate for the
  // native plane (F3/F17): a caller cannot present a token whose hmac_sig
  // verifies without holding the server signing key — forging a PAT requires
  // that key, not merely a stolen/guessed token_id. This is what makes the
  // scope (H1) and tenant_id we read from D1 trustworthy to forward.
  //
  // We deliberately do NOT additionally Argon2id-verify `random_secret` against
  // the stored `pat_hash` on EVERY request: Argon2id is intentionally expensive
  // (~tens-of-ms) and the Worker runs under a tight cpu_ms budget on the hot
  // cache path — per-request Argon2id would dominate latency for every CAS/AC
  // hit. The HMAC gate already binds possession to the signing key. Per-request
  // Argon2id is acceptable defence-in-depth to add LATER (amortised/cached per
  // token_id) ONLY if signing-key compromise becomes a credible concern — at
  // which point key rotation is the primary response. This is the decided
  // posture, not a TODO. Wiring adapter_pat::PatVerifier onto the native plane
  // as a container-side backstop is tracked as a TODO (F3 fix item 1).
  // Overlap key set: the current key plus any rotation siblings
  // (PAT_SIGNING_KEY_PREV / _NEW). A PAT minted under any of them
  // HMAC-verifies during the rotation overlap window so rotation is not
  // a fleet-wide auth outage.
  //
  // Finding #7 (HIGH) — symmetry with the container's fail-CLOSED stance:
  // a sibling that is ABSENT (unset / empty) is benign and simply skipped,
  // but a sibling that is PRESENT (env var set, non-empty) yet FAILS the
  // validity check (decodes to < 32 bytes / wrong length) is a LOUD FATAL
  // config error — never silently dropped. Silently dropping it would let a
  // one-character typo in a rotation sibling at deploy silently shrink the
  // overlap set (the container's `from_env` already refuses to mount in that
  // case), so we fail CLOSED (503) and alert the operator instead.
  const signingKeySet: string[] = [signingKeyRaw];
  for (const [name, sibling] of [
    ["PAT_SIGNING_KEY_PREV", env.PAT_SIGNING_KEY_PREV],
    ["PAT_SIGNING_KEY_NEW", env.PAT_SIGNING_KEY_NEW],
  ] as const) {
    // ABSENT (undefined / null / empty) → benign skip.
    if (typeof sibling !== "string" || sibling.length === 0) {
      continue;
    }
    // PRESENT but INVALID → LOUD fatal config error, fail CLOSED. Symmetric
    // with the container's `Some(None) => return None` refusal in
    // `adapter_pat::from_env`, which mirrors `hex::decode` + `>= 32 bytes`:
    // a valid signing key is an EVEN-length, all-hex string of >= 64 chars
    // (>= 32 bytes). This rejects ALL of finding #7's named malformations —
    // a short value, an odd-length hex string, and a non-hex typo — none of
    // which must be silently dropped (which would shrink the overlap set).
    // WP-F2: same SHARED predicate as the primary key (was inline here).
    const isValidHexKey = isValidPatSigningKeyHex(sibling);
    if (!isValidHexKey) {
      console.error(
        JSON.stringify({
          event: "pat_signing_key_sibling_malformed",
          severity: "CRITICAL",
          sibling: name,
          message: `${name} is PRESENT but is not a valid signing key (need an even-length all-hex string of >= 64 chars / >= 32 bytes; got hex length ${sibling.length}) — failing closed (503). Fix or unset the rotation sibling and redeploy.`,
        }),
      );
      return { ok: false, reason: "signing_key_not_configured" };
    }
    signingKeySet.push(sibling);
  }
  const hmacOk = await verifyPatHmacMulti(
    signingKeySet,
    parsed.hmacPreimage,
    parsed.hmacSigBytes,
  );
  if (!hmacOk) {
    return { ok: false, reason: "invalid_pat_hmac" };
  }

  // ── Step 4: PAT-row lookup by token_id (replica-read, cached) ────────────
  // Any token whose token_id is not in the D1 store → 401. This kills the
  // "any 32–256 char string accepted" vulnerability (P0-2). The D1 read
  // (`SELECT … FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL`) is the
  // per-request auth cost; trans-continental to the D1 primary it was ~0.5–0.7 s
  // (perf #99). ROOT FIX: route it through a D1 read-replication session
  // (`first-unconstrained` → nearest replica, ~tens of ms globally) with a
  // PRIMARY fallback for read-after-write freshness (a just-minted PAT that has
  // not yet replicated is re-checked on the primary — see readPatRow), so a new
  // token still authenticates immediately while a token revoked on the primary
  // is honored only for the ≤replication-lag window (sub-second; well within
  // ADR-0030's 60 s p99 revocation SLA). Still fronted by the per-isolate 5 s
  // single-flight cache (herd protection). Consulted ONLY here, AFTER the HMAC
  // possession proof; positive results only; D1 stays the source-of-truth.
  // See lib/pat_verify_cache.ts.
  // Feature-detect the Sessions API: on a runtime (or a test double) without
  // read replication, degrade gracefully to the primary handle rather than
  // crash. When present, `first-unconstrained` reads the nearest replica.
  const readSession =
    typeof env.CONFIG_DB.withSession === "function"
      ? env.CONFIG_DB.withSession("first-unconstrained")
      : env.CONFIG_DB;
  // L2: globally-replicated, per-colo KV cache — the latency fix for callers far
  // from the ENAM D1 primary (SAM has no D1 replica region). Feature-detected so
  // a build without the binding just skips L2. Reuses METADATA_KV (`patrow:` prefix).
  const kvBinding = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
  const verify = await verifyPatRowCached(readSession, parsed.tokenId, {
    primaryDb: env.CONFIG_DB,
    ...(kvBinding ? { kv: kvBinding } : {}),
    ...(waitUntil ? { waitUntil } : {}),
  });
  if (verify.kind === "error") {
    // D1 errors (network partition, DB unavailable) must not fail-open. Return a
    // distinct reason; the caller maps d1_lookup_error to 503 (transient,
    // retryable) — still fail-closed (access denied), NOT 401 "bad credentials" (H1).
    return { ok: false, reason: "d1_lookup_error" };
  }
  if (verify.kind === "not_found") {
    // token_id not in D1 (unknown or revoked) — reject.
    return { ok: false, reason: "pat_not_found" };
  }
  const row = verify.row;

  // ── Step 5: Expiry check ──────────────────────────────────────────────────
  // `expires_ms === 0` is the canonical "never expires" sentinel (mint:
  // internal_pat.rs:400) — the container SQL honors it (adapter_pat.rs:115:
  // `expires_ms = 0 OR expires_ms > now`). The edge MUST match, else a no-TTL
  // PAT works in the container but is dead-on-arrival here (split-brain, looks
  // like a forged token). Guard the sentinel.
  if (!isPatExpiryLive(row.expires_ms)) {
    return { ok: false, reason: "pat_expired" };
  }

  // ── Step 6: Tenant fast-suspend gate (go-live GAP G4) ─────────────────────
  // A valid, unexpired PAT is NOT sufficient if its tenant has been suspended
  // or erased (tenant_offboarding_state.state ∈ {suspended, erased}, migration
  // 0046). Without this gate a suspended/abusive tenant keeps full CAS/AC read
  // + write access until every one of its PATs is individually revoked. The
  // check is a three-tier read — L1 isolate → L2 KV (`tsusp:` on METADATA_KV) →
  // L3 D1 (the replica session), mirroring the pat read's L2 (ADR-0070) — so it
  // adds no uncached per-request far-D1 round-trip on the SAM hot path. It reads
  // through the same `first-unconstrained` replica session as the pat lookup
  // (perf #99) and, on a cold L1+L2, serves the rest of the colo's traffic from
  // the edge-local KV. A newly-suspended tenant is honored only for the bounded
  // ≤KV-TTL enforcement window (ADR-0070's ratified trade-off; PAT revoke stays
  // the immediate lever). Fail-OPEN on a D1 fault (availability), but a
  // KNOWN-suspended cached value still denies. The caller maps `tenant_suspended`
  // to 403 (fail-closed, distinct from the 401 bad-credential and 503 infra arms).
  if (
    await isTenantSuspended(readSession, row.tenant_id, {
      ...(kvBinding ? { kv: kvBinding } : {}),
      ...(waitUntil ? { waitUntil } : {}),
    })
  ) {
    return { ok: false, reason: "tenant_suspended" };
  }

  // Resolved tenant_id + scope from D1. The Worker forwards `scope` to the
  // container as the server-trusted `x-corelink-scope` header (H1) so scopes
  // written to D1 are actually ENFORCED downstream (previously they were stored
  // but never read here, leaving every PAT effectively unscoped). NULL scope on
  // older rows is normalised to "" so the container sees an explicit value.
  return {
    ok: true,
    tenantId: row.tenant_id,
    tokenPrefix,
    // ADR-0071: a FIND-ONLY PAT (`pat.find_only = 1`, migration 0093) stores the
    // CHECK-safe base `read-only` but is NARROWED here to the find-missing
    // capability ONLY — the Worker (sole `x-corelink-scope` authority) forwards
    // the literal `find-missing`, so the container grants `can_find_missing()`
    // and 403s CAS read/write. NULL/0 = normal PAT → forward the base scope.
    scope: row.find_only === 1 ? "find-missing" : (row.scope ?? ""),
    // WP5a: carry the narrowed runner-job marker (NULL on normal PATs). The
    // forward sites set the runner-job headers only when this is non-NULL.
    runnerJobAcKey: row.runner_job_ac_key ?? null,
    // Observability only (Server-Timing `auth` desc) — which tier served the row.
    patSource: verify.source,
  };
}
