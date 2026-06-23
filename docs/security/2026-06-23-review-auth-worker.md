# Go-Live Code Review — Auth/Identity + Worker Path (2026-06-23)

**Scope:** `worker/src/index.ts`, `worker/src/durable_object.ts`,
`worker/src/lib/{auth_rotate,runner_mint,internal_auth,session_exchange}.ts`,
`crates/corelink-container/src/routes/{auth_introspect,internal_pat,ratelimit_layer}.rs`,
`crates/corelink-container/src/{adapter_pat,native_pat_gate,scope,auth_tenant}.rs`.

**Method:** read-only, line-by-line. Each finding verified against the actual code
(no speculation). Focus: auth/scope bypass, forged-header smuggling, cross-tenant
via DO/container, F-020 stale-starting recovery, F-006 owner_tenant gate, internal-auth
key confusion, the DO env-forward completeness, rate-limit fail-open.

## Verdict

**No CRITICAL or HIGH findings. The recently-changed fixes (F-006, F-012, F-014,
F-015, F-016, F-017, F-020, F-022, the STRIPE_PRICE_ID_RUNNER_* forward) are all
correctly implemented and airtight against the attacks they target.** The auth/worker
path is GO for launch from this review's perspective. Findings below are LOW /
INFORMATIONAL hardening + doc-drift items; none block merge or launch.

| Severity | Count |
|----------|-------|
| Critical | 0 |
| High     | 0 |
| Medium   | 0 |
| Low      | 3 |
| Info     | 4 |

---

## Verified-correct (the recently-changed hot spots)

These were scrutinized hardest and are confirmed sound:

- **F-006 (auth_rotate owner_tenant mandatory)** — `auth_rotate.ts:210-216` makes
  `owner_tenant` a hard 400 when absent/empty, and `:254-261` enforces
  `ownerTenant === oldRow.tenant_id` BEFORE the mint. A compromised `pat_mint` key
  cannot rotate (mint a fresh credential for) a PAT it does not own. The 404 (unknown
  OR revoked) is checked before the 403 (wrong tenant), so the wire reveals nothing
  about existence vs ownership. Airtight.
- **F-012 (token-prefix strip)** — `x-corelink-token-prefix` IS in
  `CLIENT_TRUST_HEADERS` (`index.ts:465`) and is therefore structurally stripped on
  EVERY forward (OCI / billing-webhook / fabric / ingest / PAT / internal / onboarding /
  customer / fanout). Load-bearing because the container's customer `pat_gate_reject`
  trusts `x-corelink-token-prefix: clerk` to SKIP the Argon2id backstop — a client can
  no longer smuggle `clerk` to bypass it. Verified the PAT path re-sets it to
  `auth.tokenPrefix` (never "clerk").
- **F-014 (!isFanout region gate)** — `index.ts:2317-2321` recomputes the
  constant-time fan-out marker in the region-routing scope and `:2371` gates the
  regional re-route on `!isFanout`, so a request already on a regional Worker (which
  lacks `PROD_*` bindings) falls through to its local DO instead of 503-ing.
- **F-015 (primary-region stamp on local path)** — `primaryRegion` is hoisted
  (`index.ts:2330`) and stamped on the LOCAL DO forward (`:2514-2516`), so a client
  hitting a regional public host directly with a non-matching tenant reaches the
  container WITH a residency header → the container backstop can reject. Stripped
  from client input via `CLIENT_TRUST_HEADERS`.
- **F-020 (stale-starting recovery)** — `durable_object.ts:434-453`: a `"starting"`
  older than `STALE_STARTING_MS` (= `STARTUP_TIMEOUT_MS + 30s`, so a legit in-flight
  start is never pre-empted) is treated as stopped and restarted. `startingAt_ms` is
  stamped SYNCHRONOUSLY in the same microtask as the concurrent-start guard flip
  (`:496-500`, no intervening await), so the race is closed: a concurrent request sees
  `"starting"` and waits; a genuinely dead start self-heals. An absent `startingAt_ms`
  (pre-field state) reads as 0 → immediately stale → self-heals. Correct, no new race.
- **F-016/F-017/F-022 (ratelimit_layer.rs)** — OCI per-repo velocity gate keys on the
  path-parsed repo when the tenant header is absent (closing the unauth `/v2`+`/token`
  no-limit hole); F-017 tier ladder applies the per-tenant RPS rung once via a bounded
  `planned` set; F-022 uses the BOUNDED `NoOp*` sinks (not the unbounded `InMemory*`
  test sinks). Fail-open is confined to limiter-internal faults (mutex poison) and
  genuinely-non-billable sentinel traffic — documented and acceptable.
- **Internal-auth key confusion** — three independent secret families, no confusion:
  `/_internal/*` (per-consumer split via `resolveConsumerKey` + shared fallback),
  the runners FABRIC introspect (`FABRIC_INTROSPECT_AUTH_KEY` + `_HUGR`, a DEDICATED
  secret, container is sole authority), and `BILLING_INGEST_AUTH_KEY`. The mint-to-
  container call always uses the SHARED key (the container's mint gate expects it),
  re-read explicitly in runner_mint/auth_rotate. Constant-time compares with no length
  oracle on both TS (`constantTimeSecretEqual`) and Rust (`internal_auth_ok`) sides.
- **STRIPE_PRICE_ID_RUNNER_* forward (new)** — declared in `Env`
  (`index.ts:95-99`), forwarded by the DO (`durable_object.ts:563-567`), and read by
  the container's `build_runners_resolver` (`main.rs:108-112`). Complete end-to-end;
  no silent-no-op gap.
- **Native plane PAT possession (F3/F17)** — the worker comments still call this a
  "TODO", but the container-side `NativePatGate` IS now wired
  (`routes.rs:478-599`) onto CAS/AC/Bazel/Turbo/customer/users and fatal-on-missing
  in prod (`main.rs:253`). A leaked `PAT_SIGNING_KEY` forging an HMAC-valid token for
  an arbitrary tenant is now caught by the Argon2id + tenant-binding backstop. See
  INFO-1 (the worker comment is stale).

---

## LOW

### LOW-1 — `auth_rotate` is gated by the `pat_mint` consumer key, not `runner_mint`
**File:** `worker/src/lib/auth_rotate.ts:166` (`requireConsumerAuth(request, env, "pat_mint", …)`)

`auth_rotate` (clw key rotation) authenticates with the `pat_mint` consumer
(`CORELINK_PAT_MINT_AUTH_KEY`), the SAME key as the signup webhook's full-admin mint
surface — whereas the runner mint/revoke routes deliberately use a separate
`runner_mint` key (`internal_auth.ts:49-56` documents this least-privilege split). The
clw backend therefore holds the signup-grade mint key. This is internally consistent
(rotate genuinely needs the mint authority and reads/writes the same `pat` table) and
not exploitable on its own — a `pat_mint`-key holder rotating a PAT is still bounded by
the F-006 `owner_tenant` check. But it widens what a leaked clw key can reach (it can
exercise `/_internal/pat/mint` directly with any scope incl. `admin`). **Fix
(post-launch):** give clw-rotate its OWN consumer key, or document explicitly that the
clw backend is trusted at signup-grade. Severity LOW: the surfaces it reaches are all
already mint-authority surfaces; no NEW privilege.

### LOW-2 — `customer_v1` Clerk arm mints `read-write` for every dashboard caller
**File:** `worker/src/index.ts:2040` (`h.set("x-corelink-scope", "read-write")`)

The Clerk-session dashboard arm unconditionally stamps `x-corelink-scope: read-write`,
so any dashboard user (regardless of their org role) gets cache read+write scope on the
customer routes. This is the dashboard surface (overview/usage/billing/keys/team/audit),
not the data plane, and the container customer routes apply their own role/mint gates
(`mint_requests_write`, `classify_requested_scopes`), so it is not a data-plane
escalation. But a least-privilege "read-only dashboard viewer" role cannot be expressed
through this arm today — the scope is hardcoded. **Fix (post-launch):** derive the
forwarded scope from the Clerk session's org role rather than a constant. Severity LOW:
no cross-tenant or data-plane bypass; tenant is server-resolved from the verified
session.

### LOW-3 — `mintScopedPat` collapses container mint failures to a uniform 500, masking 429 throttle
**File:** `worker/src/lib/session_exchange.ts:507-515`

When the container's `/_internal/pat/mint` returns a non-200 (incl. its own
`429 too_many_requests` from `MintRateLimiter`/`MintInflightLimiter`), `mintScopedPat`
collapses it to a generic `500 INTERNAL_ERROR`. A legitimate caller being mint-rate-
limited at the container therefore sees a 500, not a 429 with retry semantics, and
cannot distinguish "retry shortly" from "server fault". The worker-side
`checkMintThrottle` already returns a proper 429 before this, so the container 429 is a
secondary backstop — but the status collapse is a (minor) availability/observability
gap. **Fix (post-launch):** preserve a 429 (with `Retry-After`) when the container
mint returns 429; keep the 500 collapse for genuine faults. Severity LOW: fail-CLOSED
(no token leaked), only a worse client signal.

---

## INFORMATIONAL

### INFO-1 — Stale worker comments claim native-plane Argon2id is an un-wired TODO
**File:** `worker/src/index.ts:838-843, 2499-2505`; `routes/internal_pat.rs:20-27`

The worker's `extractAuth` doc and the PAT-forward comment still say the native plane
"does NOT perform Argon2id re-verify … Wiring adapter_pat::PatVerifier onto the native
plane … is tracked as a TODO (F3 fix item 1)". That TODO is DONE: `NativePatGate` is
wired across CAS/AC/Bazel/Turbo/customer/users (`routes.rs:478-599`) and is fatal-on-
missing in prod (`main.rs:253`). The comments understate the actual posture and could
mislead a future reviewer into thinking the native plane rests solely on the HMAC gate.
Recommend updating the comments to reflect the wired backstop.

### INFO-2 — `/_internal/*` remains publicly reachable; sole gate is the shared secret
**File:** `worker/src/index.ts:674-684`; `routes/internal_pat.rs:7-18`

Documented (F4) and accepted: `/_internal/pat/mint` is reachable from the public
internet, gated only by the constant-time `CORELINK_INTERNAL_AUTH_KEY` (per-consumer)
compare. The per-consumer split + the container's mint rate/concurrency limiters
(`internal_pat.rs` `MintRateLimiter`/`MintInflightLimiter`) bound the blast radius, and
the gate is fail-CLOSED + checked before body parse. The recommended hardening
(Service-Binding-only, no public route) is still open and worth doing post-launch, but
the current posture is defensible. Informational only.

### INFO-3 — `native_pat_gate` 5s verify cache bounds revocation latency on the native plane
**File:** `crates/corelink-container/src/native_pat_gate.rs:55-68`

A PAT revoked mid-TTL keeps native CAS/AC/Bazel/Turbo access for up to 5s (cache hit
returns `Ok` without re-consulting D1). This is a deliberate, documented latency/
security trade (rt-nuclear verify C3) and tightly bounded; revocation on the Worker
hot path and the adapter D1 lookup are immediate. No change recommended; noting for
completeness.

### INFO-4 — DO env-forward verified complete for all container-read vars in scope
**File:** `worker/src/durable_object.ts:529-648`

Cross-checked the forward block against the container's `env::var` reads for the auth/
billing/runners surfaces: `CORELINK_INTERNAL_AUTH_KEY`, `PAT_SIGNING_KEY`,
`FABRIC_INTROSPECT_AUTH_KEY(+_HUGR)`, `BILLING_INGEST_AUTH_KEY`,
`STRIPE_PRICE_ID_RUNNER_*`, `STRIPE_*`, `ERASURE_*`, OCI key + legacy alias — all
forwarded. The per-consumer mint keys (`CORELINK_PAT_MINT_AUTH_KEY`, `_ADMIN_`,
`_ERASE_`, `_RUNNER_MINT_`) are NOT in the DO forward block, but that is CORRECT: the
runner/auth-rotate/token-exchange routes are handled AT the Worker (they never reach
the container's `/_internal/*` gate), and the container's `internal_pat`/`admin`
per-consumer resolution falls back to the shared key it DOES receive. No silent-no-op
gap found.

---

## Cross-tenant / forged-header summary

- DO ID is derived solely from `idFromName(resolvedTenantId)` (PAT-resolved or server
  constant); the container's `AuthTenant` extractor fail-CLOSES on sentinels and trusts
  ONLY `x-corelink-tenant-id`, which the Worker strips-then-sets on every forward.
- `CLIENT_TRUST_HEADERS` is comprehensive (x-admin-*, fanout-from, scope, tenant-id,
  internal-auth, storage-quota, client-ip/XFF, primary-region, token-prefix) and is
  applied on EVERY forward arm via `stripClientTrustHeaders` before any server `.set()`.
- The token-exchange cross-tenant rejection (`session.tenant !== audience → 403`,
  `session_exchange.ts:719`) is intact and is githugr's CRITICAL-#1 defense.
- The native `NativePatGate` binds a genuine PAT to the CLAIMED tenant (tenant-A PAT on
  tenant-B path → 401), closing the leaked-signing-key cross-tenant forge chain.

No path was found by which a client can smuggle a forged tenant-id, scope, internal-auth,
fan-out marker, or token-prefix past the Worker, nor obtain cross-tenant access via the
DO/container.
