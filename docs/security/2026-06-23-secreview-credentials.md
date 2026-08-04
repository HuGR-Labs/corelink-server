# GO-LIVE Security Review — Secrets / Credentials / Auth Posture

- **Date:** 2026-06-23
- **Reviewer:** automated security reviewer (read-only)
- **Commit:** `c1fa752079047c3a74c9a40d51d18d66ca7a6dfa` (main)
- **Scope:** PAT signing/verify, internal-auth key split, ERASURE_SALT_KEY,
  R2/Stripe creds, DO→container env-forward, header trust, repo secret hygiene.

## Verdict

**No committed live secrets found.** The credential / auth posture is strong and
defensible for launch: dedicated internal-auth keys are genuinely separated where
it matters (introspect / billing-ingest have NO shared fallback), the PAT planes
(HMAC native vs Argon2id adapter) are cleanly separated, every secret-bearing
state struct has a `Debug` redaction, header-trust strip is structural and
complete, and the predictable-salt fallback is fail-CLOSED in prod. The findings
below are **LOW / INFO** observations and documented design tradeoffs — none is a
launch blocker.

## Counts by severity

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 2 |
| INFO     | 4 |

---

## Findings

### LOW-1 — `pat_mint` / `admin` / `erase` / `runner_mint` consumer keys fall back to the shared `CORELINK_INTERNAL_AUTH_KEY`

- **Where:** `worker/src/lib/internal_auth.ts:157-196` (`resolveConsumerKey`);
  mirrored in `crates/corelink-container/src/routes/internal_pat.rs:698-707`
  (`resolve_internal_auth_key("CORELINK_PAT_MINT_AUTH_KEY").or_else(... shared)`).
- **Risk:** Until the operator provisions the dedicated per-consumer keys, all
  four consumer surfaces (signup PAT-mint, admin, erase, runner-mint) authenticate
  against the SAME `CORELINK_INTERNAL_AUTH_KEY`. A leak of that one shared secret
  unlocks every one of those surfaces — the least-privilege benefit the split was
  designed to give is only realized once the dedicated keys are actually set.
- **Assessment:** This is a **documented, intentional** flag-day-free rollout
  shape (internal_auth.ts:36-45, 140-153, 170-195) — the dedicated key is preferred
  when present and the shared key is the fallback **only when the dedicated key is
  UNSET**. A sub-floor (<32 char) dedicated key is **NOT** treated as absent: it is
  REFUSED fail-closed and logged, because falling through there would hand that
  consumer the broad shared key exactly when the operator was trying to isolate it.
  (This bullet asserted the opposite; corrected 2026-08-04 against the code.) **Crucially, the two
  highest-blast-radius dedicated keys do NOT fall back** (see INFO-1), so the
  fallback is confined to the worker-side `/internal/*` mint family.
- **Fix (launch hardening, not blocker):** Provision `CORELINK_PAT_MINT_AUTH_KEY`,
  `CORELINK_ADMIN_AUTH_KEY`, `CORELINK_ERASE_AUTH_KEY`, and
  `CORELINK_RUNNER_MINT_AUTH_KEY` as distinct ≥32-char secrets in prod so the
  shared-fallback path is never exercised. Track in the secrets matrix.

### LOW-2 — `handleRunnerMint` re-reads the shared key directly with a weaker length check

- **Where:** `worker/src/lib/runner_mint.ts:130-133`.
- **Detail:** After the `requireConsumerAuth(..., "runner_mint", ...)` gate passes,
  the handler re-reads `env.CORELINK_INTERNAL_AUTH_KEY` to present to the
  container's `/_internal/pat/mint` route, guarding only on `length === 0` (not the
  `>= 32` floor the gate enforces). A 1–31-char shared key would pass this second
  check yet would have been rejected by `resolveConsumerKey` — but only if a
  ≥32-char *dedicated* runner key existed, in which case the mint-to-container call
  would still present the short shared key. In practice the container's own gate
  (`internal_pat.rs` `< 32` floor) rejects it, so this fails closed downstream.
- **Risk:** None exploitable (container rejects sub-floor keys), but the
  inconsistency means a misconfigured short shared key surfaces as a confusing
  container-side 401/403 rather than a clean worker-side 403.
- **Fix:** Use the gate's `MIN_INTERNAL_AUTH_KEY_LEN` floor here too, or reuse the
  already-resolved key from the gate instead of re-reading the env.

### INFO-1 — Dedicated keys that are TRULY separated (no fallback) — verified

- `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `FABRIC_INTROSPECT_AUTH_KEY_HUGR`):
  `crates/corelink-container/src/routes/auth_introspect.rs:661-696` — read directly
  from env, ≥32-char floor, **no** fallback to `CORELINK_INTERNAL_AUTH_KEY`. A
  present-but-too-short HuGR key is ignored with a warn (not accepted).
- `BILLING_INGEST_AUTH_KEY`: `crates/corelink-container/src/routes/billing_ingest.rs:549-552`
  — dedicated, ≥32-char floor, **no** shared fallback (explicitly documented at
  lines 31-32, 538-539).
- These two highest-value internal surfaces (cross-tenant introspection authority;
  billing usage ingest) are correctly isolated from the shared key.

### INFO-2 — DO→container env-forward is complete and CI-gated; no predictable-fallback secret leaks through

- `worker/src/durable_object.ts:529-648` forwards every container-read secret,
  including the new `STRIPE_PRICE_ID_RUNNER_{STARTER,PRO,TEAM,SCALE,MAX}` (lines
  563-567), `ERASURE_SALT_KEY` (573), the Ed25519 attestation seed/key/region
  (580-582), `FABRIC_INTROSPECT_AUTH_KEY[_HUGR]`, `BILLING_INGEST_AUTH_KEY`, and
  the OCI legacy-alias `HUGR_OCI_TOKEN_KEY`.
- `scripts/check-env-contract.py` passes: **"32 container-read env var(s) scanned;
  all 32 present in the DO forward-list."** The set-but-not-delivered class of bug
  (the original ERASURE_SALT_KEY gap) is mechanically closed.
- **ERASURE_SALT_KEY predictable-fallback (the flagged concern) is handled
  correctly:** the salt is actually derived in the signup-worker
  (`apps/signup-worker/src/webhooks/clerk.ts:160-189` `deriveErasureSalt`), which
  **fails CLOSED in prod** (throws → 500 → Svix retry) when the key is absent; the
  deterministic non-secret `SHA-256("erasure-salt:"+dsrId)` fallback is reachable
  only in non-prod (`environment` not starting with `"prod"`). No predictable salt
  reaches production.

### INFO-3 — Header trust (`CLIENT_TRUST_HEADERS`) is structural and complete

- `worker/src/index.ts:427-477`. Every forgeable server-trust header is stripped
  on EVERY forward before the Worker sets its own verified value (delete-then-set):
  `x-admin-{scope,principal,tenant}`, `x-corelink-internal-auth`,
  `x-corelink-fanout-from`, `x-corelink-scope`, `x-corelink-tenant-id`,
  `x-corelink-storage-quota-bytes`, `x-forwarded-for`, `x-corelink-client-ip`,
  `x-corelink-primary-region`, `x-corelink-token-prefix` (the last one added in
  F-012 after the OCI/billing/fabric arms were found forwarding it raw).
- `x-corelink-route-kind` is intentionally unlisted (unconditionally `.set()` on
  every forward). The `_system` internal forward (`session_exchange.ts:490-493`)
  sets `x-corelink-internal-auth` to the resolved key AFTER the strip — correct.
- No forgeable trust header was found that escapes the structural strip.

### INFO-4 — PAT lifecycle, plane separation, constant-time, and rotation — all sound

- **Plane separation:** native HMAC plane verifies via `subtle::ConstantTimeEq`
  (`crates/corelink-pat/src/sig.rs:111`, `format.rs:119`); adapter plane uses
  Argon2id (`crates/corelink-pat/src/argon.rs`). The native_pat_gate wraps the
  shared `PatVerifier` with a fingerprint→tenant cache that fails CLOSED on D1
  faults (`native_pat_gate.rs:117`).
- **Rotation (F-006):** `verify_hmac_sig_multi` (`sig.rs:93-118`) folds over the
  full key set with NO early-return, OR-ing per-key `ct_eq` into a single `Choice`
  — so neither the matching-key identity (old vs new during rotation) nor the
  position leaks via timing. Empty key set → `InvalidPat` (fail-closed).
- **Mint:** `/_internal/pat/mint` is a pure function returning `token_plaintext`
  once; "NEVER log token_plaintext" is enforced and the success log explicitly
  omits it (`internal_pat.rs:390-395, 630`). Caller writes the D1 row.
- **Rotate/Revoke:** `worker/src/lib/auth_rotate.ts` mints-new-then-revokes-old
  (never a zero-valid-PAT window), validates `owner_tenant` against the row
  (REV-S2), refuses unmappable scopes (read-only) fail-CLOSED (422). Revoke is the
  single idempotent `UPDATE pat SET revoked_at_ms WHERE ... AND revoked_at_ms IS
  NULL`, tenant-scoped (REV-S2) to bound a leaked runner-mint key's blast radius.
- **Constant-time internal-auth compare:** both the TS gate
  (`internal_auth.ts:218-233` `constantTimeSecretEqual`) and the Rust gate
  (`internal_pat.rs` / `dsr.rs:127-146` `internal_auth_ok`) pad the provided value
  to the expected length, run ONE `timingSafeEqual`/`ct_eq`, then AND a single
  length-equality bit — no length oracle (CAA-360 #27).

### INFO — Secret hygiene / Debug redaction

- Every secret-bearing state struct redacts in `Debug`: `adapter_pat.rs:307`
  (signing keys), `storage.rs:72-73` (R2 access key id + secret), and
  `internal_auth_key` in `tier_select.rs:413`, `internal_pat.rs:362-363`,
  `cas_erase.rs:152`, `billing_ingest.rs:314`, `auth_introspect.rs:176`,
  `dsr.rs:115`.
- The only response-body/echo hits for `*_AUTH_KEY` / `signing_key` /
  `token_plaintext` are inside `#[cfg(test)]` modules (`TEST_AUTH_KEY` fixtures).
- `scripts/ops/stripe-setup-runners.sh:47` and `stripe-setup-tiers.sh` take the
  live key via env injection (`: "${STRIPE_LIVE_SECRET_KEY:?...}"`) — no hardcoded
  secret. `scripts/e2e-stripe-checkout.sh` references `sk_test_…`/`whsec_…` only as
  doc placeholders and reads them from env.

## Repo secret sweep (committed-secret check)

`git grep` over all tracked non-doc files for
`sk_live_/rk_live_/whsec_/pk_live_/AKIA…` + PEM private-key blocks returned only:

1. `crates/corelink-clerk/src/env_config.rs:316` — `"sk_live_redacted"` (test literal).
2. `tests/e2e-user-journeys/src/harness.rs:636` — all-`A` `whsec_…=` (test fixture).
3. `crates/corelink-dpa-acceptance/src/jwt.rs:17-18` — PEM header strings in a
   doc-comment (not a key).

`.env.local` (real `sk_test_…` keys) is gitignored and not tracked. `.gitleaks.toml`
+ baseline triage are in place as a CI gate.

**Conclusion: no committed live secrets.**
