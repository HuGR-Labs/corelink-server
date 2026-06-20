# CoreLink E2E User-Journey Matrix — comprehensive coverage plan

> **Goal (owner, 2026-06-20):** simulate *everything* real users (multiple personas) do in
> CoreLink — every feature, every path, every possibility — and validate it **end-to-end, exactly
> as the user experiences it** (black-box: deployed HTTP API + PAT + CLI only; no internal crate
> imports, no mocks, no reading internal state to assert).
>
> This expands the existing 7-journey ship gate (`src/main.rs`, written pre-launch with now-fixed P0
> assumptions + stale URL contracts) into a full **persona × surface × path** matrix grounded in the
> real route map (see the surface inventory in the discovery report).

## 0. Black-box rules (inherited, inviolable)
- Only what a paying customer has: the deployed API (`CORELINK_E2E_ENDPOINT`), a Bearer PAT, the `corelink` CLI, the published contract.
- FORBIDDEN: any `corelink-*` crate import, direct D1/R2/KV access, mocking the SUT, reading internal state to assert.
- Every journey returns `Pass | Fail(reason) | Gated(reason)` — **gated, never silently skipped**.

## 1. Harness / environment strategy (where these run safely)
| Tier | Target | Tenants | Runs | Destructive OK? |
|---|---|---|---|---|
| **L0 local** | `wrangler dev` container @ `localhost:8787` + seeded D1 (`scripts/family-e2e-tier-seed.sql`) | the 6 per-tier fixtures | every journey incl. quota-cap + erase | ✅ ephemeral |
| **L1 prod-safe** | `https://corelink-api.humangr.com` | dedicated e2e test tenants (NOT real customers) | non-destructive journeys (read/write small blobs, isolation, auth, dashboard) | ❌ no quota-exhaust / no erase |
| **L2 prod-destructive** | prod | a throwaway e2e tenant explicitly provisioned for it | quota-cap, DSR erase — **owner-gated, one tenant** | ⚠️ only on the throwaway tenant |

Env vars (extend the existing set):
`CORELINK_E2E_ENDPOINT`, `CORELINK_E2E_TENANT` (the tenant UUID under test), and a **token map** per persona:
`CORELINK_E2E_PAT_RW`, `_PAT_RO`, `_PAT_ADMIN`, `_PAT_REVOKED`, `_PAT_EXPIRED`, `_PAT_TENANT_B`, plus `_PAT_FREE / _PAT_SOLO / _PAT_PRO / _PAT_ENTERPRISE / _PAT_PASTDUE`. Each journey **Gates** (not fails) when its required token isn't set.

## 2. Personas (the "different / multiple user types")
| ID | Persona | Tier | Scope | Sub-state | What they prove |
|---|---|---|---|---|---|
| P1 | Fresh free-tier signup | free | read-write | active | onboarding → first cache use works |
| P2 | Solo paid | solo | read-write | active | paid tier served + quota headroom |
| P3 | Pro paid | pro | read-write | active | higher tier limits |
| P4 | Enterprise | enterprise | admin | active | unlimited + admin surfaces |
| P5 | CI consumer | any | **read-only** | active | read works, **write → 403** |
| P6 | Team owner | any | **admin** | active | keys/team/audit admin surfaces |
| P7 | Past-due / pending | any | read-write | **past_due / pending** | **denied** (402/503) — no service without active sub |
| P8 | Revoked-PAT holder | any | (revoked) | — | **401** on every surface |
| P9 | Expired-PAT holder | any | (expired) | — | **401** on every surface |
| P10 | Cross-tenant attacker | tenant B | read-write | active | **cannot** read/write tenant A's data (404/403, never A's bytes) |
| P11 | Header-forgery attacker | any | read-only | active | injected `x-corelink-tenant-id` / `x-corelink-scope` are **stripped** by the Worker (no priv-esc) |
| P12 | Legal-hold tenant | any | admin | active | erasure request **preserved** (not erased) under legal hold |

## 3. Surfaces (every feature) × required scope
S1 Identity (`/v1/users/me`) · S2 Native CAS (`/v1/cas/:tenant/:hash` + batch) · S3 Native AC (`/v1/ac/:tenant/:digest`) · S4 Bazel REAPI v2 · S5 Turbo v8 · S6 cargo/sccache · S7 npm · S8 pip · S9 brew · S10 oci (two-leg token) · S11 Customer dashboard (overview/usage/billing/keys/team/audit) · S12 PAT lifecycle (mint/list/revoke/rotate) · S13 Billing/tier (tier-select → checkout → active → enforce; dunning recover; cancel) · S14 Quota $-ceiling (402 at limit; upgrade raises) · S15 DSR erasure (erase → 410 Gone tombstone, batch too) · S16 Introspection (runners fabric) · S17 Audit export + offline chain re-derive.

## 4. Path taxonomy (the "all paths and possibilities" per cell)
- **Happy** — the documented success.
- **Edge** — empty body, oversize (>cap → 413), malformed digest, idempotent re-PUT, concurrent writers, cache miss→hit, batch framing, unicode/long names.
- **Adversarial** — wrong scope (RO→write 403), cross-tenant (P10), forged headers (P11), revoked/expired PAT (P8/P9), over-quota (P7/S14), tombstoned-read (S15 → 410), replay.

## 5. The coverage matrix (journey IDs)
Format `J-<surface>-<path>`; each cell lists the personas it runs for. ✅ = must-pass, 🔒 = must-deny, ⊙ = gated by env/flag.

| Surface | Happy | Edge | Adversarial |
|---|---|---|---|
| S1 Identity | ✅P1-P4 | malformed PAT→401 | 🔒P8 revoked, 🔒P9 expired, 🔒 no-auth→401 |
| S2 CAS | ✅P1,P2 PUT→GET bytes match; 2nd GET hit | empty/oversize-413/idempotent/batch | 🔒P5 RO-write→403, 🔒P10 cross-tenant, 🔒P11 forged-header, 🔒S15 tombstoned→410 |
| S3 AC | ✅P2 update→read | divergent-body guard | 🔒P5 RO-write→403, 🔒P10 |
| S4 Bazel ⊙ | ✅ 2 builds→2nd cache-hit | findMissingBlobs | 🔒P10 instance isolation |
| S5 Turbo ⊙ | ✅ artifact PUT/GET + events | events fairness | 🔒P5 RO, 🔒P10 |
| S6 cargo | ✅ store/fetch | double-verify perf | 🔒P5 RO-write, 🔒P10, quota-charge |
| S7 npm | ✅ metadata + tarball (SHA512) | integrity-mismatch reject | 🔒P5 RO, 🔒P10, 🔒 quota-no-header→fail-closed |
| S8 pip | ✅ simple-index + download | — | 🔒P5, 🔒P10, 🔒 quota-fail-closed |
| S9 brew | ✅ bottle (public dedup) | upstream timeout | 🔒P10 private isolation |
| S10 oci ⊙ | ✅ token→pull/push | downscoped bearer | 🔒P5 RO-push, 🔒P10 |
| S11 Dashboard | ✅P1-P4 overview/usage/billing | empty periods | 🔒P5 (admin-only bits), 🔒P10 |
| S12 PAT lifecycle | ✅ mint→list→use→revoke→401 | rotate; scope subset | 🔒P5 mint-admin→deny, 🔒P10 revoke-others (backward-compat warn) |
| S13 Billing/tier ⊙ | ✅ tier-select→checkout URL; active→served | dunning past_due→recover→active | 🔒P7 pending→denied, cancel→deny |
| S14 Quota ⊙🔒 | — | — | 🔒P7/free PUT until 402; upgrade raises ceiling |
| S15 DSR erase ⊙ | ✅ erase→subsequent GET 410 | batch-read tombstone→410/503 | 🔒P12 legal-hold→preserved |
| S16 Introspect | ✅ valid PAT→{valid,tenant,plan} | bad token→{valid:false} | 🔒 wrong internal key→401/403 |
| S17 Audit | ✅P6 export→chain re-derives | pagination | 🔒P5 non-admin→403, 🔒P10 tenant-scoped |

## 6. Module structure (build target — expand the single binary)
```
tests/e2e-user-journeys/
  src/
    main.rs            # runner: env → persona/token map → dispatch all journeys → table + ship verdict
    harness.rs         # Config, token map, JourneyResult, HTTP helpers, blob/digest, assert helpers
    personas.rs        # persona → (tenant, token, expected-allow/deny) resolution
    journeys/
      identity.rs      # S1
      cas.rs           # S2 (+ batch)
      ac.rs            # S3
      bazel.rs         # S4 (gated)
      turbo.rs         # S5 (gated)
      adapters.rs      # S6-S10 (cargo/npm/pip/brew/oci)
      dashboard.rs     # S11
      pat_lifecycle.rs # S12
      billing.rs       # S13 (gated)
      quota.rs         # S14 (gated, destructive)
      dsr.rs           # S15 (gated, destructive)
      introspect.rs    # S16
      audit.rs         # S17
```
Disjoint files → parallel build with zero merge conflict. `main.rs`/`harness.rs` are the shared contract (frozen first, then fan out).

## 7. Build plan (TL fan-out)
1. **Freeze the contract** (`harness.rs` + `JourneyResult` + persona/token map + the real URL templates) — one authored module, no agent invents URLs.
2. **Fan out** one agent per `journeys/*.rs` (disjoint), each implementing its surface's Happy+Edge+Adversarial cells against the frozen harness, returning a card.
3. **TL review** each diff cold (security-sensitive: the deny-assertions must actually deny).
4. **Integrate** + run against L0 (local seeded) first → fix reds → then L1 (prod-safe tenants).
5. **Wire** the seed + a run script (`scripts/e2e-journeys-run.sh`) + document the env/token provisioning.

## 8. Status
- [x] Surface map + persona model + harness strategy (this doc).
- [ ] Contract freeze (harness.rs).
- [ ] Fan-out build of journeys/*.
- [ ] L0 local green → L1 prod-safe green.
