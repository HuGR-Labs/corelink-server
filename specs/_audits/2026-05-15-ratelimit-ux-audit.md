---
id: "AUDIT-2026-05-15-RATELIMIT-UX"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep"
owner: "Gustavo Schneiter"
tags: ["audit", "rate-limit", "429", "ux", "customer-facing", "r-prep"]
---

# Audit — Rate-Limit 429 Customer-Facing UX

> **Scope.** End-to-end audit of what a CoreLink customer sees when
> their request is denied with HTTP 429 (the canonical 5-arm taxonomy:
> `tenant_quota` / `per_ip` / `per_pat` / `over_quota` /
> `global_circuit_open`). Covers `crates/corelink-rate-headers/`
> (header + body builder, today's surface), `crates/corelink-ratelimit/`
> (per-tenant token bucket + tier ladder), `crates/corelink-tier-selection/`
> (tier vocabulary for upgrade CTAs), and the production Tower middleware
> consumer surface that turns these into HTTP bytes.
>
> **Goal.** Verify the 429 surface is SOTA — RFC 9331 + RFC 6585 + RFC 9110
> stable headers, informative JSON body with retry guidance + tier
> upgrade CTA, customer documentation matching what we emit, sales FAQ
> for the same surface — then close every gap surfaced in this WI.

## 1. Surface as it stands (2026-05-15, pre-closure)

### 1.1 Headers emitted today

`corelink-rate-headers::RateLimitHeaderBuilder::build` composes the
following typed payload (`RateLimitHeaders`):

| Header | Source | Spec | Present |
|--------|--------|------|---------|
| `RateLimit: limit=N, remaining=M, reset=S` | `render_rate_limit` | RFC 9331 §2 IETF stable | **yes** |
| `RateLimit-Policy: <limit>;w=<window>, …` | `render_rate_limit_policy` | RFC 9331 §3 | **yes** |
| `Retry-After: <secs>` | `render_retry_after` | RFC 6585 §4 + RFC 9110 §10.2.3 | **yes** |
| `X-Rate-Limit-Type: <tenant_quota|per_ip|per_pat|over_quota|global_circuit_open>` | `render_x_rate_limit_type` | CoreLink-specific 5-arm taxonomy | **yes** |
| `X-RateLimit-Limit / -Remaining / -Reset` (legacy custom) | — | de-facto Bazel / Buck2 / Stripe / GitHub | **no — deliberate** (see §1.4) |
| `X-CoreLink-Tier: <free|solo|team|business|enterprise>` | — | informational vendor header | **MISSING** |
| `X-CoreLink-Quota-Reset-UTC: <RFC 3339 timestamp>` | — | absolute reset timestamp companion to `reset=N` seconds form | **MISSING** |
| `X-CoreLink-Tier-Upgrade-URL: <url>` | — | machine-readable upgrade pointer for SDK CTA | **MISSING** |

### 1.2 Body emitted today

There is **no canonical 429 body schema** in the rate-headers crate.
The Tower middleware in the production wiring (deferred per `trait-
abstraction-defer`) is expected to render bytes; the in-memory crate
ships the headers but no body builder.

This is the **P0 gap** this audit closes: customer SDKs cannot rely
on a stable JSON shape because there isn't one.

### 1.3 Per-endpoint quota by tier

Source: `crates/corelink-ratelimit/src/tier.rs` (`TIER_RATE_LADDER`)
+ `crates/corelink-eviction::Tier` (5-tier vocabulary FROZEN at the
data-model layer).

| Tier | Refill rate (RPS sustained) | Burst capacity (tokens) | Stripe Checkout |
|------|----------------------------:|------------------------:|-----------------|
| Free | 10 | 50 | n/a (instant activation) |
| Solo | 50 | 200 | required |
| Team | 200 | 1 000 | required |
| Business | 1 000 | 5 000 | required |
| Enterprise | 10 000 (negotiable per contract) | 50 000 | inquiry-form route (S-13) |

Notes on the tier vocabulary discrepancy:

- `corelink-ratelimit::tier` uses (Free / Solo / Team / Business /
  Enterprise) — the FROZEN rate-limit ladder.
- `corelink-tier-selection::tier` uses (Free / Starter / Team / Pro /
  Enterprise) — the FROZEN billing ladder.

Both are sealed in their respective contracts. The rate-limit ladder
is what customers consume; the tier-selection ladder is what they pay
on. The 429 body's `tier_upgrade_url` MUST point to the
tier-selection canonical pricing surface (`https://corelink.dev/pricing`)
and pass the **billing** tier vocabulary; the rate-limit `X-CoreLink-Tier`
header carries the **rate-limit** tier vocabulary. The customer doc
in §4 documents both vocabularies explicitly.

### 1.4 Why RFC 9331 (and **not** legacy `X-RateLimit-*`)

Per sprint contract §14.s08.5 + `corelink-rate-headers::lib.rs`
module-level docs lines 62–70:

> Legacy custom `X-RateLimit-*` vendor-specific headers vary across
> vendors (Bazel / Buck2 / Stripe / GitHub) so customer SDKs cannot
> reliably consume them without per-vendor adaptation. RFC 9331 IETF
> stable `RateLimit` + `RateLimit-Policy` is the canonical format
> these SDKs already consume; adopting it = customer SDK works without
> per-vendor adaptation.

This audit **PRESERVES** the canonical RFC 9331-only stance. The
`X-CoreLink-*` headers added in §2 are **informational vendor
additions**, not replacements for the IETF canonical pair.

## 2. Canonical 429 JSON body schema (P0 closure)

```jsonc
{
  "error": {
    "code": "rate_limit_exceeded",          // stable enum; one of:
                                            //   tenant_quota | per_ip | per_pat
                                            //   over_quota   | global_circuit_open
    "kind": "tenant_quota",                  // mirrors X-Rate-Limit-Type
    "message": "Request rate exceeded …",    // human-readable
    "retry_after_seconds": 5,                // mirrors Retry-After
    "tier": "free",                          // current rate-limit tier
    "tier_upgrade_url": "https://corelink.dev/pricing",
    "docs_url": "https://docs.corelink.dev/explanation/rate-limits",
    "request_id": "01HFXY…",                 // UUIDv7 for support escalation
    "limit": 10,                              // RFC 9331 limit
    "remaining": 0,                           // RFC 9331 remaining
    "reset_seconds": 5,                       // RFC 9331 reset
    "reset_utc": "2026-05-15T14:30:25Z"       // absolute reset (RFC 3339)
  }
}
```

Field-by-field rationale:

- `error.code` is a **stable string enum** for SDK pattern-matching;
  the 5 canonical values are pinned by `XRateLimitTypeKind::as_str`.
- `error.kind` is a redundant copy of `X-Rate-Limit-Type` so SDKs
  that parse only the body still have the discriminator.
- `error.message` is a human-readable English sentence; localisation
  is **deferred** (sprint contract §10 anti-scope; English at GA).
- `error.retry_after_seconds` mirrors the `Retry-After` header;
  SDKs SHOULD honour whichever they see first.
- `error.tier` is the customer's current rate-limit tier (lower-case
  snake_case; `corelink-ratelimit` vocabulary).
- `error.tier_upgrade_url` is a static `https://corelink.dev/pricing`
  pointer to the canonical pricing surface; opens in the browser when
  the SDK prints the message.
- `error.docs_url` points at `apps/docs/docs/explanation/rate-limits.mdx`
  (Diátaxis "explanation" quadrant). Customers click to learn the
  taxonomy + backoff guidance.
- `error.request_id` is the UUIDv7 the worker stamps on every request;
  surfaces in `request_id` log fields for support escalation.
- `error.limit / remaining / reset_seconds` duplicate the RFC 9331
  `RateLimit:` header so body-only consumers don't need to re-parse.
- `error.reset_utc` is an absolute RFC 3339 timestamp mirroring the
  new `X-CoreLink-Quota-Reset-UTC` header — robust against clock skew
  when the SDK retries far in the future.

## 3. Headers gap closures (P0)

| Header | Why | Status before | Status after |
|--------|-----|---------------|--------------|
| `X-CoreLink-Tier: <tier>` | SDK CTA + log enrichment | missing | **added** |
| `X-CoreLink-Quota-Reset-UTC: <RFC 3339>` | clock-skew-robust absolute reset | missing | **added** |
| `X-CoreLink-Tier-Upgrade-URL: <url>` | machine-readable upgrade CTA | missing | **added** |

The 5 canonical RFC 9331 / 6585 / 9110 headers already present
(`RateLimit`, `RateLimit-Policy`, `Retry-After`, `X-Rate-Limit-Type`)
are preserved unchanged.

## 4. Customer doc closure

`apps/docs/docs/explanation/rate-limits.mdx` (new) documents:

- The 5-arm taxonomy + which camada emits which arm.
- Per-tier rate-limit ladder table.
- Per-header semantics (RFC 9331 + RFC 6585 + CoreLink `X-*`).
- The canonical 429 JSON body schema.
- Backoff guidance (honour `Retry-After`; exponential fallback when
  absent; absolute `reset_utc` for long retries).
- When to upgrade tier + the `tier_upgrade_url` CTA.

## 5. Sales FAQ closure

`marketing/sales/RATE-LIMIT-FAQ.md` (new) — 9-question subset of
FAQ-MASTER focused on rate limits. Lives alongside the master so
Customer Success can hand it to a prospect during a pricing
conversation without exposing the entire 50-question dossier.

## 6. Test gap closures

- ≥ 5 new unit tests in `crates/corelink-rate-headers/src/headers.rs`
  asserting the new headers + body schema.
- 1 new property test in
  `crates/corelink-rate-headers/tests/prop_rate_headers.rs`
  pinning the invariant **"on every 429 the body carries `error.code`
  matching `X-Rate-Limit-Type` AND `retry_after_seconds` matching the
  `Retry-After` header AND `limit / remaining / reset_seconds` matching
  the `RateLimit:` header"** — i.e. the body is **always consistent**
  with the headers, regardless of input.

## 7. Cross-references

- e2e-resilience scenarios under `tests/e2e-resilience/tests/scenarios.rs`
  consume rate-limit responses; the new body schema is what those
  scenarios will assert against once the production Tower middleware
  ships (deferred per `trait-abstraction-defer`).
- `ROADMAP-TO-GA.md` Wave R-3 §3 R3-1 (`tests/e2e/signup-to-cas.rs`)
  will hit the 429 surface when a Free-tier signup bursts past 50
  tokens; that E2E test will assert against the canonical body shape
  introduced here.

## 8. Charter compliance

- `#![forbid(unsafe_code)]` preserved (no `unsafe` added).
- No `unwrap / expect / panic` outside test modules.
- Audit fail-CLOSED preserved (no audit-related code changed).
- Property test deterministic + reads `PROPTEST_CASES` env at runtime.

## 9. P0 / P1 / P2 prioritisation

| ID | Severity | Description | Closure |
|----|----------|-------------|---------|
| RL-UX-P0-1 | P0 | No canonical 429 body schema | **this WI** §2 |
| RL-UX-P0-2 | P0 | Missing `X-CoreLink-Tier` / `-Quota-Reset-UTC` / `-Tier-Upgrade-URL` | **this WI** §3 |
| RL-UX-P0-3 | P0 | No customer-facing rate-limit doc | **this WI** §4 |
| RL-UX-P0-4 | P0 | No sales FAQ subset for rate limits | **this WI** §5 |
| RL-UX-P1-1 | P1 | Body localisation (en-only at GA) | **deferred** S-22 i18n |
| RL-UX-P1-2 | P1 | SDK auto-honour of `tier_upgrade_url` (open-in-browser hint) | **deferred** S-21 SDK |
| RL-UX-P2-1 | P2 | RateLimit-Policy multi-camada disambiguator (tag policies per camada) | **deferred** S-22 |

Every P0 closed in this WI; P1 / P2 filed as follow-on tickets in
`specs/_audits/proptest-followup-tickets.md` parent file.
