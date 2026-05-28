# CoreLink E2E User-Journey Suite

## What this is

The **real ship gate** that was missing before the 2026-05-28 multi-model
production-readiness audit. The prior smoke gate tested only `/health` and skipped
every authenticated CAS operation — which is why the P0 breaks shipped undetected.

This suite simulates a **real paying customer** using only:
- The deployed HTTP API (configurable via `CORELINK_E2E_ENDPOINT`)
- The `corelink` CLI binary (via `std::process::Command`)
- An HTTP client (`reqwest` blocking) with a Bearer PAT
- The published OpenAPI contract for response-shape assertions

## BLACK-BOX rule (non-negotiable)

Tests MUST use **only** what a paying customer has. The following are **FORBIDDEN**:

| Forbidden | Why |
|-----------|-----|
| `corelink-*` internal crate imports | Would test internal state, not the public surface |
| `path = "../../crates/..."` in Cargo.toml | Zero internal-crate deps enforced by acceptance gate |
| Direct D1/R2/KV access (`wrangler d1 execute`, etc.) | Internal store access; not a user surface |
| Test-only backdoor endpoints | Bypass the surface under test |
| In-process handler calls | Not a real customer request |
| Reading internal state to assert | Only the API response is the oracle |

Acceptance gate: `grep -c 'path *= *"\.\..*crates' Cargo.toml` MUST print `0`.

## Current state: SHIP-GATE RED

All 7 journeys are **expected RED** against the current production deploy. This
is intentional — the suite was built to CATCH the existing P0s, not to pass despite
them. The gate will turn GREEN when the P0 remediation wave lands.

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CORELINK_E2E_ENDPOINT` | `http://localhost:8787` | Base URL of the API under test |
| `CORELINK_E2E_TOKEN` | (required) | Bearer PAT for tenant A |
| `CORELINK_E2E_TOKEN_TENANT_B` | (required for J5) | Bearer PAT for a DIFFERENT tenant |
| `CORELINK_E2E_BAZEL_TEST` | unset | Set to `1` to enable Bazel round-trip (journey 4) |
| `CORELINK_E2E_QUOTA_TEST` | unset | Set to `1` to enable quota hard-cap test (journey 7) |

## Running the suite

```bash
# Against local wrangler dev (default):
CORELINK_E2E_TOKEN="ct_live_..." cargo run -p e2e-user-journeys

# Against production:
CORELINK_E2E_ENDPOINT="https://api.corelink.humangr.com" \
  CORELINK_E2E_TOKEN="ct_live_..." \
  CORELINK_E2E_TOKEN_TENANT_B="ct_live_..." \
  cargo run -p e2e-user-journeys

# With Bazel (requires `bazel` on PATH):
CORELINK_E2E_BAZEL_TEST=1 \
  CORELINK_E2E_TOKEN="ct_live_..." \
  cargo run -p e2e-user-journeys
```

Exit code 1 if any journey fails; exit code 0 if all non-gated journeys pass.

## Journey → P0 map

| # | Journey | Status | P0 gated | P0 description |
|---|---------|--------|----------|----------------|
| J1 | Onboarding/ping — health + authenticated endpoint | **RED** | P0-6, P0-2 | Container health probe protocol mismatch → 503; auth middleware not wired |
| J2 | Auth rejection — absent/malformed/invalid → 401 | **RED** | P0-2 | Any 32-256 char string accepted; auth not in live path |
| J3 | Cache miss→hit — PUT blob, GET bytes match, 2nd GET = cache hit | **RED** | P0-1, P0-4, P0-7 | Composed router not bound; no R2 bindings; InMemory fakes ephemeral |
| J4 | Bazel round-trip — bazel-init + 2 builds → 2nd hits cache | **RED / GATED** | P0-1, P0-4, P0-6 | Same as J3 + health mismatch |
| J5 | Tenant isolation — tenant B cannot read tenant A's blob | **RED** | P0-3 | tenantId null; all traffic → single `_pending_auth` DO |
| J6 | Audit export + re-derive — N ops → chain integrity | **RED** | P0-1, P0-4 | Admin router not bound; D1 bindings missing → empty chain |
| J7 | Quota hard-cap — free-tier PUT until 429 | **GATED** | P1-5, P0-4 | Quota race; no R2 bindings → quota never tracked |

### P0 source

All P0 references are from:
`specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md`

| P0 | Summary |
|----|---------|
| P0-1 | Container serves only health + Stripe webhook; composed CAS/AC/Admin router discarded (`_composed_router`) |
| P0-2 | No PAT/auth validation on live request path; Worker checks token format only; DO never validates |
| P0-3 | No tenant isolation on live path; tenantId hardcoded null; all auth traffic → single `_pending_auth` DO |
| P0-4 | Container has no R2/D1/KV bindings; cannot read/write a blob even if routed |
| P0-6 | DO health probe sends HTTP/1.1 to gRPC-only (HTTP/2) port 50051; no tonic-web; container health fails → 503 on every auth request |
| P0-7 | Route handlers are InMemory fakes; real CF-R2/D1 adapters are `compile_error!()`-gated |
| P1-5 | Free-tier quota race; concurrent writes both pass check; no per-tenant concurrency cap |

## Architecture notes

- Language: Rust (binary crate, not a library)
- Dependencies: `reqwest` (blocking HTTP), `serde_json`, `uuid` (v4), `sha2`, `hex`
- No `tokio`/async — blocking is correct for a sequential journey runner
- Hermetic: each run generates unique content (UUID-keyed blobs); no shared state between runs
- Gated tests (Bazel, quota) are explicitly flagged `GATED` in output — not silently skipped
- Clean-up: Bazel journey creates/removes a temp directory; other journeys leave no persistent state
