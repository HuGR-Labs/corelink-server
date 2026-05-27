---
id: "AUDIT-2026-05-26-W36-PROPTEST-FU-002-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-36", "proptest", "fu-w33-002", "corelink-clerk-cf", "corelink-statuspage-real", "density", "seal"]
references:
  - "specs/_audits/proptest-followup-tickets.md"
  - "specs/_audits/2026-05-26-w36-proptest-wasm-seal.md"
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
---

# Wave 36 — clerk-cf + statuspage-real proptest density SEAL

## §1. Scope

Closes the remaining two-crate portion of **WI-PROPTEST-FU-W33-002**
per `specs/_audits/proptest-followup-tickets.md`. The corelink-wasm
portion was sealed earlier in this wave at
`2026-05-26-w36-proptest-wasm-seal.md` (commit `4bdf17e9`, 6 proptests);
this audit covers the residual:

- `corelink-clerk-cf` — Cloudflare Worker bindings + `ClerkHealthDo`
  Durable Object pure-logic surface.
- `corelink-statuspage-real` — Atlassian Statuspage Public-Metric
  client (DSR completion publisher).

Both crates had pre-existing property-test gaps despite hosting
load-bearing pure-logic surfaces (tenant-scope CT compare, DSR audit
roundtrip, rate-limit window enforcement, retry classification).

### Invariants covered — corelink-clerk-cf (`tests/prop_clerk_health_logic.rs`)

| ID | Invariant | Surface |
|----|-----------|---------|
| INV-CLERK-TENANT-SCOPE | `ParsedRoute::assert_tenant` rejects any URL tenant ≠ actor tenant; same-tenant always Ok | CTRL-PRIV-001 defense-in-depth (constant-time CT compare via `subtle::ConstantTimeEq`) |
| INV-CLERK-ROUNDTRIP | `upsert(note, now_ms)` → `get` returns byte-identical `correlation_id` + `note` + `created_at_ms`; `RecordResponse::from(rec)` preserves all three | State-store contract / wire-format mirror |
| INV-CLERK-SWEEP-BOUNDARY | `sweep` removes record iff `created_at_ms + ttl_ms < now_ms` (strict inequality; exact boundary survives) | TTL state machine (half-open Prometheus convention) |
| INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER | Failing audit hook MUST short-circuit every mutation across any random op sequence; state map stays empty | Audit-emit-BEFORE fail-CLOSED |

### Invariants covered — corelink-statuspage-real (`tests/prop_statuspage_invariants.rs`)

| ID | Invariant | Surface |
|----|-----------|---------|
| INV-STATUSPAGE-REDACT-NEVER-LEAK | `redact_api_key(k)` MUST NOT echo any prefix of `k` longer than the trailing 4 codepoints; length-bounded; canonical `"OAuth ***<tail>"` shape | CTRL-PRIV-001 credential redaction |
| INV-STATUSPAGE-WIRE-ROUNDTRIP | `serde_json` round-trip preserves every field of `DsrCompletionReport`; `to_metric_body` puts `window_end` as `timestamp` and `p95` as `value` | Audit envelope wire format (Atlassian Statuspage API v1) |
| INV-STATUSPAGE-RATELIMIT-NO-DRIFT | A `DenyBackoff` decision MUST NOT advance `last_allowed`; `jitter == retry_after / 8`; boundary at `t + 5min` re-allows + advances | 1-publish-per-5-min quota policy |
| INV-STATUSPAGE-RETRY-TOTAL-AND-MONOTONE | `RetryPolicy::decide` is total over every u16 status code (2xx → Success; 401/403 → GiveUpAuth; 429 + 5xx → Retry/GiveUp by attempt; other → GiveUp); `backoff_for` monotone non-decreasing up to `max_backoff` cap | Retry classification + exponential schedule |

## §2. Acceptance criteria

- [x] ≥3 new proptests per crate (delivered: **4 each**; **8 total**;
      plus 1 sanity test in clerk-cf re-export canary)
- [x] Each uses `proptest!` macro with `proptest_cases()` env-overridable
      helper (NO hard-coded `ProptestConfig::with_cases(N)`)
- [x] Each has rustdoc comment naming the invariant ID
- [x] No `#[ignore]` to bypass
- [x] No `matches!(..., Variant { .. })` anti-pattern (S-08 P1-1) —
      explicit field-bind / field-equality asserts used throughout
- [x] `#![forbid(unsafe_code)]` preserved on both crates + on both test
      files
- [x] `subtle::ConstantTimeEq` invariant (clerk-cf) preserved — proptest
      validates the cross-tenant CT-compare path drives the
      `TenantScope` error variant for ALL non-matching inputs
- [x] CTRL-CRED-001 / CTRL-PRIV-001 preserved — proptest pins the
      redactor against prefix-byte leakage
- [x] INV-AUDIT preserved on both crates (clerk-cf proptest exercises
      audit-emit-BEFORE fail-CLOSED across random op sequences)

## §3. Acceptance evidence

```
Crate: corelink-clerk-cf
  Tests: 35 → 40 (+5: 4 proptests + 1 sanity canary)
  Build:                                              GREEN
  Clippy --tests -D warnings:                         GREEN
  cargo test (default cases):                         40 passed; 0 failed
  PROPTEST_CASES=256 cargo test --release:            40 passed; 0 failed

Crate: corelink-statuspage-real
  Tests: 35 → 39 (+4 proptests)
  Build:                                              GREEN
  Clippy --tests -D warnings:                         GREEN
  cargo test (default cases):                         39 passed; 0 failed
  PROPTEST_CASES=256 cargo test --release:            39 passed; 0 failed
```

### Files touched

- `crates/corelink-clerk-cf/Cargo.toml` — added `proptest = { workspace = true }` to native-only `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`.
- `crates/corelink-clerk-cf/tests/prop_clerk_health_logic.rs` — NEW.
- `crates/corelink-statuspage-real/Cargo.toml` — added `proptest = { workspace = true }` to `[dev-dependencies]`.
- `crates/corelink-statuspage-real/tests/prop_statuspage_invariants.rs` — NEW.

### Placement rationale

Both proptest files live in `tests/` (integration-test target) because:

1. The pure-logic surfaces under test (`ClerkHealthLogic`,
   `ParsedRoute`, `DsrCompletionReport`, `RetryPolicy`,
   `StatuspageRateLimiter`, `redact_api_key`) are public via `pub use`
   in each crate's `lib.rs`. No private-helper bypass is needed.
2. The wasm32 build is unaffected — `proptest` is gated to the native
   target on clerk-cf (the crate compiles to `wasm32-unknown-unknown`
   in production; proptest needs a host thread scheduler).
3. Both test files re-assert `#![forbid(unsafe_code)]` at the top —
   defense-in-depth against accidental unsafe leaking through a
   build-dep cycle.

## §4. Charter compliance

- `proptest_cases()` helper (NOT `ProptestConfig::with_cases(N)` const).
- No `#[ignore]` bypass anywhere.
- No `matches!(..., Variant { .. })` — every error/decision assertion
  uses explicit pattern-bind + field-by-field equality (S-08 P1-1 lesson).
- `INV-AUDIT` preserved: clerk-cf proptest pins
  `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` directly; statuspage-real proptests
  cover the wire format + redaction surfaces that feed the audit envelope.
- `CTRL-CRED-001` preserved: clerk-cf session tokens are not weakened
  (proptests target the orthogonal tenant-scope + audit + TTL surfaces);
  statuspage-real `redact_api_key` proptest is the explicit
  credential-NEVER-LEAK pin.
- W36 Stage 2.C orthogonality: clerk-cf + statuspage-real were NOT
  absorbed by W35-P2 (Triggers A + B blocked the merge); these property
  tests add density without touching the W36 Stage 2.C migration zones.

## §5. Stress validation

`PROPTEST_CASES=256` release stress executed across BOTH crates;
all 8 new proptests passed at 256-case density on the first run. No
shrink, no `#[ignore]`, no masking.

## §6. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
