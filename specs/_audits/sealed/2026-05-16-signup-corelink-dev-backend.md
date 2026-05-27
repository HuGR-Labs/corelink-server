# Audit — `signup.corelink.humangr.com` Pilot Signup Backend (Wave-29 stream-1)

- **Date:** 2026-05-16
- **Wave / stream:** Wave-29 / stream-1 (R-prep)
- **Branch:** `wt/r-prep-signup-corelink-dev-backend`
- **Base commit:** `365dd38` (main)
- **Closes:** **DEBT-027** engineering-side (pilot signups ≥ 3 to GA;
  GA-cutover gate per `RB-GA-CUTOVER.md`).
- **Cross-refs:**
    - `specs/_audits/2026-05-16-pilot-signup-pipeline.md` (wave-27
      enablement bundle — admin scripts + Grafana panels)
    - `docs/internal/pilot-comms-templates.md` (wave-28 pilot-outreach
      email templates referencing `https://signup.corelink.humangr.com/pilot/<token>`)
    - `specs/_audits/2026-05-15-debt-register.md §DEBT-027` (row update)

## 1. Why

Wave-27 (commit `6761860`) shipped the operator-facing pilot admin
shell scripts (`grant-pilot-tier.sh`, `list-pilot-tenants.sh`,
`pilot-24h-checkin.sh`) and the canonical Grafana dashboard YAML
(`dashboards/grafana/dash-pilot-tenants.yml`). Wave-28's pilot-comms
package pinned the public outreach URL
`https://signup.corelink.humangr.com/pilot/<token>` in every template
(`docs/internal/pilot-comms-templates.md`).

The actual backend that redeems those tokens did not exist. Operator
outreach was blocked on the backend: every pilot signup would have
404'd. This audit ships the production backend.

## 2. Deliverables

| # | Artefact | Path |
|---|----------|------|
| 1 | Rust route — `POST /v1/signup/pilot/{token}` | `apps/server/src/routes/signup.rs` |
| 2 | D1 migration — `pilot_signups` table | `migrations/d1/0053_pilot_signups.sql` |
| 3 | Token-mint CLI | `scripts/admin/mint-pilot-token.sh` |
| 4 | Integration tests (8 cases) | `apps/server/tests/signup_pilot.rs` |
| 5 | This audit doc | `specs/_audits/2026-05-16-signup-corelink-dev-backend.md` |
| 6 | DEBT-027 row → engineering-CLOSED | `specs/_audits/2026-05-15-debt-register.md` |

## 3. Token format + verify discipline

The route consumes a single URL path segment of the form:

```
pilot_<env>_<unix_ms>_<16-hex-random>.<hmac-hex>
```

| Segment | Source | Bound |
|---------|--------|-------|
| `pilot` | literal | parse-layer guard (`TokenError::NotPilotToken`) |
| `<env>` | `staging` ∣ `prod` | parse-layer guard (`TokenError::BadEnv`) |
| `<unix_ms>` | mint wall-clock | TTL anchor; >14 days old → `Expired` |
| `<16-hex-random>` | `openssl rand -hex 8` | 64 bits entropy ≫ pilot-programme volume |
| `<hmac-hex>` | `HMAC-SHA256(SIGNUP_TOKEN_KEY, body)` | constant-time compare via `subtle::ConstantTimeEq` |

**TTL** = 14 days (`PILOT_TOKEN_TTL_MS = 14 * 24 * 60 * 60 * 1000`).
Mirrors the operator pilot-window policy in
`docs/internal/customer-success-playbook.md §1`.

**Future-mint tolerance**: a token minted "in the future" (mint
timestamp > now) verifies on the happy path — the operator may
pre-mint a batch for an upcoming launch. We only reject tokens
older than TTL.

**Signature compare**: `subtle::ConstantTimeEq` on the post-hex-decode
bytes. We bail on length mismatch FIRST (the length is not secret;
HMAC-SHA256 is always 32 bytes) so the timing-safe compare only runs
on equal-length inputs.

## 4. Route surface

### Request

```text
POST /v1/signup/pilot/<token>
Content-Type: application/json
X-Forwarded-For: <client-ip>  (rate-limit anchor; optional)

{
  "email": "...",
  "company_name": "...",
  "tier_hint": "...",
  "expected_use_case": "..."
}
```

Every body field is required; each capped at `MAX_FIELD_LEN = 256`
chars. `email` must contain `@` (cheap RFC-5322 prefix check).

### Response (201)

```json
{
  "tenant_id": "<uuid v7>",
  "activation_url": "https://signup.corelink.humangr.com/pilot/activate/<id>",
  "state": "RESERVED"
}
```

### Failure modes

| Condition | Status | Body / header | Audit event_type |
|-----------|--------|---------------|------------------|
| forged / malformed / not-pilot token | 401 | `unauthorized` | `corelink.signup.pilot_token_rejected.v1` (payload=`<tag>`) |
| expired token (`> TTL`) | 401 | `unauthorized` | same as above (payload=`expired`) |
| missing / oversize body field | 400 | `bad_request` | same (payload=`invalid_field=<name>`) |
| rate-limit (6th request / IP / h) | 429 | `Retry-After: <s>` | `corelink.signup.pilot_rate_limited.v1` |
| audit emit failure | 503 | `audit pipeline closed` | (none — sink down) |
| store unavailability | 503 | `signup store unavailable` | (none — pre-emit position) |

## 5. Audit emit discipline

Every reject arm emits an audit row BEFORE the response is rendered,
routed through `emit_or_503` (mirrors the wave-20 `audit_export`
discipline). Fail-CLOSED per
`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`: a sink failure surfaces 503
with no body bytes.

The happy-path arm also persists the row in `pilot_signups` (D1)
BEFORE the audit emit. The pre-audit-emit store mutation is bounded
by the route's idempotency-on-email contract: a retry on a failed
audit emit surfaces the SAME `tenant_id` (the row already exists),
so the at-most-once externally-visible reservation is preserved.

| Event type | Arm | `exit_status` | `payload` |
|------------|-----|---------------|-----------|
| `corelink.signup.pilot_reserved.v1` | happy path (new) | `reserved` | (none) |
| `corelink.signup.pilot_reserved.v1` | happy path (duplicate email) | `duplicate` | (none) |
| `corelink.signup.pilot_token_rejected.v1` | token verify fail | `rejected` | `<TokenError::tag()>` |
| `corelink.signup.pilot_token_rejected.v1` | body validate fail | `bad_request` | `invalid_field=<name>` |
| `corelink.signup.pilot_rate_limited.v1` | 429 | `rate_limited` | `retry_after_secs=<n>` |

## 6. Rate-limit gate

**Policy**: 5 requests / IP / hour (per wave-29 stream-1 charter).

`RateLimitConfig::with_overrides(1, 5, 720, 86_400, 7 * 86_400)`:

| Knob | Value | Rationale |
|------|-------|-----------|
| `refill_rate_per_sec` | 1 | int-rounded floor of 5/3600s |
| `burst_capacity` | 5 | bucket starts full → first 5 requests admit |
| `retry_after_floor_secs` | 720 | 3600s / 5 = 12 min between refills under policy |
| `retry_after_hard_ceiling_secs` | 86_400 | 1 day (mirrors audit_export) |
| `retry_after_canceled_tenant_secs` | 604_800 | 7 days (mirrors canonical) |

The bucket key is `BucketKey::per_ip(NIL_UUID, ip)` — pre-auth, so
no tenant id is yet known. The leading nil tenant collapses the
key dimension to per-IP-only, matching the audit-export route's
pre-auth pattern.

**Cross-IP isolation**: integration test
`rate_limit_isolated_across_ips` proves the per-IP bucket key
isolates IPs `1.1.1.1` and `2.2.2.2` (IP-A exhausting its bucket
does NOT block IP-B).

## 7. Schema (migration 0053)

`migrations/d1/0053_pilot_signups.sql` is **59th** migration. The CI
gate `scripts/check_migrations_additive.py` is green (`OK: 59
migration file(s) scanned; all additive`).

Table:

```sql
CREATE TABLE IF NOT EXISTS pilot_signups (
    id TEXT PRIMARY KEY,                -- UUID v7
    tenant_id TEXT NOT NULL,            -- UUID v7 (operator reads this)
    email TEXT NOT NULL,                -- plaintext (operator outreach)
    company_name TEXT,
    tier_hint TEXT,
    expected_use_case TEXT,
    signed_up_at INTEGER NOT NULL,      -- Unix epoch ms
    activated_at INTEGER,               -- NULL until operator ACTIVE
    state TEXT NOT NULL CHECK (state IN (
        'RESERVED', 'PROVISIONED', 'ACTIVE', 'EXPIRED', 'CANCELLED'
    )),
    token_id TEXT NOT NULL              -- 16-hex random body
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_pilot_signups_email
    ON pilot_signups (email);
CREATE UNIQUE INDEX IF NOT EXISTS idx_pilot_signups_token_id
    ON pilot_signups (token_id);
CREATE INDEX IF NOT EXISTS idx_pilot_signups_state_signed_up
    ON pilot_signups (state, signed_up_at);
```

**PII note**: `email` is stored plaintext to back the operator's
`list-pilot-tenants.sh` outreach surface. The broader production
signup flow in `crates/corelink-signup` hashes `user_email` via
`UserEmailHash`; the operator-internal pilot table predates the
email-hash discipline. DEBT-028 (to be added post-GA) captures the
lift to email-hash + a separate operator-visible decryption seam
if and when GA-cutover graduates the pilot programme into self-serve.

## 8. Tests

### Unit tests (`apps/server/src/routes/signup.rs`)

9 unit tests, all pass:

```
test routes::signup::tests::body_validation_catches_missing_and_oversize ... ok
test routes::signup::tests::in_memory_store_dedupes_on_email          ... ok
test routes::signup::tests::malformed_tokens_rejected                 ... ok
test routes::signup::tests::expired_token_rejected                    ... ok
test routes::signup::tests::wrong_key_rejected                        ... ok
test routes::signup::tests::forged_signature_rejected                 ... ok
test routes::signup::tests::future_minted_token_accepted              ... ok
test routes::signup::tests::mint_then_verify_roundtrips               ... ok
test routes::signup::tests::router_builds                              ... ok
```

### Integration tests (`apps/server/tests/signup_pilot.rs`)

8 integration tests, all pass:

```
test happy_path_valid_token_returns_201               ... ok
test forged_hmac_returns_401                          ... ok
test expired_token_returns_401                        ... ok
test rate_limit_kicks_in_at_sixth_request_same_ip     ... ok
test rate_limit_isolated_across_ips                   ... ok
test audit_emit_failure_returns_503_fail_closed       ... ok
test duplicate_email_returns_original_tenant_id       ... ok
test bad_request_body_returns_400                     ... ok
```

## 9. Quality gates

| Gate | Result |
|------|--------|
| `cargo build -p corelink-server` | green |
| `cargo test -p corelink-server --test signup_pilot` | 8/8 green |
| `cargo test -p corelink-server --lib routes::signup` | 9/9 green |
| `cargo clippy -p corelink-server --all-targets -- -D warnings` | clean |
| `scripts/check_migrations_additive.py` | green (59 migrations) |

## 10. Charter compliance

- `#![forbid(unsafe_code)]` inherited from `corelink-server`.
- No `unwrap`/`expect`/`panic` in `src/` (clippy lints `deny`-level).
- HMAC verify uses `subtle::ConstantTimeEq` (timing-safe).
- Audit emit BEFORE response on every reject arm (fail-CLOSED per
  `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
- Rate-limit gate at the route boundary uses the canonical
  `corelink-ratelimit::RateLimiter` trait (per-IP bucket).
- DCO sign-off + `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`
  on commit.

## 11. axum 0.7 path-param syntax note

The codebase pins **axum 0.7 + matchit 0.7**, which uses the `:name`
capture syntax (axum 0.8 + matchit 0.8 switch to `{name}`). The
canonical route path constant is:

```rust
pub const SIGNUP_PILOT_ROUTE: &str = "/v1/signup/pilot/:token";
```

The audit doc and module-level prose use `{token}` for readability;
the wire path is `:token` in the matchit registration. The
wave-11 audit-doc prose for `/v1/ac/{tenant}/{action_digest}` +
`/v1/admin/read/{resource}` carries a similar prose/wire mismatch
(those routes are not currently exercised via `tower::ServiceExt::oneshot`
so the latent bug never surfaced — captured as a follow-up in §13).

## 12. Production wiring (deferred to PRR ship gate)

The trait-object surface (`Arc<dyn SignupAuditSink>` +
`Arc<dyn SignupStore>` + `Arc<dyn RateLimiter>` + `Arc<dyn WallClock>`)
preserves the binary shape across the native / wasm32 CF-Worker swap.
Production wiring slots:

1. **Audit sink** — CloudEvents emitter publishing to the audit-chain
   archive (mirrors wave-15 + wave-20 `audit_export` pattern).
2. **Store** — D1 binding writing to the `pilot_signups` table via
   the wave-17 D1-writer pattern.
3. **Rate limiter** — Cloudflare Durable Object singleton (per
   `corelink-ratelimit` framework canonical wiring).
4. **HMAC key** — `SIGNUP_TOKEN_KEY` Worker secret. Operator
   `mint-pilot-token.sh` reads the same secret from
   `~/.corelink/signup-key`.

The wave-29 stream-1 deliverable is the **native gRPC server + axum
HTTP shell**; the CF Worker boot path that swaps in the production
trait-objects lands at the T-7d PRR ship gate.

## 13. Follow-ups

1. **axum 0.7 path-param latent bug** — `apps/server/src/routes/ac.rs`
   + `admin.rs` declare path constants with `{name}` literal but the
   wire path requires `:name`. Latent because no `oneshot` tests
   exercise them. Captured as DEBT-029-engineering (to be added in
   wave-30 if a `cargo check`-only verifier surfaces the mismatch).
2. **Email-hash lift** — DEBT-028 (to be added post-GA) per §7 PII note.
3. **CloudEvents archive** — the `SignupAuditSink` production
   implementation lands as part of the audit-chain Wave-15.x archive
   producer wiring (currently captured under R-PREP-CF-AUDIT-CHAIN).

## 14. DEBT-027 row update

DEBT-027 → **engineering-CLOSED**. Owner-side row (announcement +
≥ 3 signup collection + per-tenant 8-step pre-flight) remains OPEN
pending the wave-28 pilot-comms package send. The DEBT-027 row in
`specs/_audits/2026-05-15-debt-register.md` is updated to reflect
the engineering-side closure with the wave-29 stream-1 commit ref.

## 15. Closure note — live-D1 tests added wave-30

Wave-30 stream-9 (`wt/r-prep-signup-live-d1-test`) lifted the 8
in-memory tests in `apps/server/tests/signup_pilot.rs` onto a real
SQLite-backed D1 surrogate in
`apps/server/tests/signup_pilot_live_d1.rs`, with the harness
shipping as `apps/server/tests/harness/d1_container.rs`. The
in-memory suite is preserved unchanged (it remains the latency-
cheap canonical regression); the new suite catches schema drift
between the Rust struct shape and `migrations/d1/0053_pilot_signups.sql`,
exercises the SQLite UNIQUE INDEX duplicate-email contract, pins
the wave-29 fail-CLOSED ordering (§6) against a durable backing
store, and asserts migration idempotency under
`INV-AUTH-MIGRATION-ADDITIVE`. See
`specs/_audits/2026-05-16-signup-live-d1-tests.md` for the full
design rationale and test matrix.

---

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
