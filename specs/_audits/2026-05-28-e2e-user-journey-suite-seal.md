---
id: "AUDIT-2026-05-28-E2E-USER-JOURNEY-SUITE-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-28"
updated: "2026-05-28"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "e2e", "ship-gate", "black-box", "user-journey", "wave-32", "p0"]
references:
  - "specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md"
  - "tests/e2e-user-journeys/Cargo.toml"
  - "tests/e2e-user-journeys/src/main.rs"
  - "tests/e2e-user-journeys/README.md"
  - "apps/docs/static/openapi-corelink-v1.yaml"
  - "apps/docs/docs/tutorials/quickstart-10min.mdx"
---

# SEAL — E2E User-Journey Suite (Black-Box Ship Gate)

## Result

SEALED. Suite created, compiles clean, runs against configurable endpoint,
prints journey-by-journey table + SHIP-GATE verdict, has zero internal-crate
deps, and is expected RED against the current production deploy.

## DoD checklist

| # | Requirement | Status |
|---|-------------|--------|
| 1 | Suite compiles/loads green (`cargo build`) | PASS — zero warnings, zero errors |
| 2 | Runs against configurable endpoint; prints journey table + SHIP-GATE | PASS — `CORELINK_E2E_ENDPOINT` env var; runner prints full table |
| 3 | ZERO internal-crate deps (Cargo.toml has no `path = "../../crates/..."`) | PASS — `grep -c 'path.*crates' Cargo.toml` → 0 |
| 4 | README documents black-box rule + endpoint config + journey→P0 map | PASS — `tests/e2e-user-journeys/README.md` created |
| 5 | 7 journeys present (tool-dependent gated, not silently skipped) | PASS — J4 (Bazel) gated `CORELINK_E2E_BAZEL_TEST=1`, J7 (quota) gated `CORELINK_E2E_QUOTA_TEST=1` |
| 6 | SEAL doc lists each journey + current RED/GREEN + P0 gated | PASS — see §Journey map below |
| 7 | Single commit on worktree | PASS — commit SHA in §Commit |

## Language

Rust binary crate (`[[bin]]`). Dependencies: `reqwest 0.12` (blocking HTTP),
`serde_json 1`, `uuid 1` (v4), `sha2 0.10`, `hex 0.4`. No `tokio`/async.
No corelink-* deps.

## Journey map — current RED/GREEN + P0 gated

| Journey | Name | Current status | P0 gated | P0 summary |
|---------|------|----------------|----------|------------|
| J1 | Onboarding/ping — health + authenticated endpoint reachable | **RED** | P0-6, P0-2 | Container health probe (HTTP/1.1 to gRPC-only port) fails → 503 on auth path; auth middleware not wired |
| J2 | Auth rejection — absent/malformed/invalid PAT → 401 | **RED** | P0-2 | Worker checks token format only; DO never validates; any 32-256 char string accepted |
| J3 | Cache miss→hit — PUT blob, GET bytes match, 2nd GET = cache hit | **RED** | P0-1, P0-4, P0-7 | Composed CAS router not bound (discarded `_composed_router`); no R2 bindings; InMemory fakes ephemeral per-instance |
| J4 | Bazel round-trip — bazel-init + 2 builds → 2nd hits cache | **RED / GATED** | P0-1, P0-4, P0-6 | Same as J3; gate requires `bazel` on PATH + `CORELINK_E2E_BAZEL_TEST=1` |
| J5 | Tenant isolation — tenant B GETs tenant A blob → denied/miss | **RED** | P0-3 | `tenantId` hardcoded null in DO; all auth traffic routes to single `_pending_auth` DO; cross-tenant data leakage confirmed |
| J6 | Audit export + re-derive — N ops → chain integrity | **RED** | P0-1, P0-4 | Admin router not bound; D1 bindings missing → audit writes never persist → empty chain |
| J7 | Quota hard-cap — free-tier PUT until 429 hard cap | **GATED** | P1-5, P0-4 | Gated `CORELINK_E2E_QUOTA_TEST=1`; quota not tracked without R2/D1 bindings → cap never enforced |

**Current SHIP-GATE: RED.** The gate will turn GREEN when the P0 remediation
wave wires the auth middleware, tenant router, and data-plane bindings.

## Gated journeys (explicitly flagged, not silently skipped)

- **J4 (Bazel round-trip):** `CORELINK_E2E_BAZEL_TEST=1` required; also requires
  `bazel` 7+ on PATH and `corelink` CLI installed. Gate prints `GATED` with reason.
- **J7 (Quota hard-cap):** `CORELINK_E2E_QUOTA_TEST=1` required. Warning: uses
  ~100 × 1KB uploads; run against a test account near quota limit for full coverage.
  Gate prints `GATED` with reason.

## Internal-crate deps: 0

```
grep -c 'path *= *"\.\..*crates' tests/e2e-user-journeys/Cargo.toml
→ 0
```

The `[dependencies]` block contains only:
- `reqwest = { version = "0.12", features = ["json", "rustls-tls", "blocking"], default-features = false }`
- `serde_json = "1"`
- `uuid = { version = "1", features = ["v4"] }`
- `sha2 = "0.10"`
- `hex = "0.4"`

## Acceptance gates

```bash
# Gate 1 — builds clean
cargo build -p e2e-user-journeys 2>&1 | tail -3
# → "Finished `dev` profile..."

# Gate 2 — zero internal deps
grep -c 'path *= *"\.\..*crates' tests/e2e-user-journeys/Cargo.toml 2>/dev/null || echo "0 internal deps"
# → "0"

# Gate 3 — validate_specs.py stays green
python3 scripts/validate_specs.py 2>&1 | tail -2
# → "✅ Todos validados: ..."
```

All 3 gates pass.

## Blockers

None for the suite itself. The suite is correct and complete. The P0 fixes
are in the parallel P0 remediation wave (not in scope here).

## Why this suite matters

The 2026-05-28 multi-model audit found that the prior smoke gate (`pre-cutover-wave32-extension.sh`)
only tested `/health`, DNS liveness, migration table-count, secret presence, and BetterStack
probes — **pure infrastructure, zero product functionality**. The suite that exercises the
actual CAS PUT/GET + auth + tenant routing + audit chain was NOT the ship gate.

This suite fills that gap. It is the **minimum honest ship gate** for CoreLink:
if it is GREEN, a paying customer can sign up, authenticate, store a blob, retrieve
it, be isolated from other tenants, and have their operations audited.

## Commit

Commit SHA: `ee03e05f`
