# CoreLink user-simulation suites — SURFACE / PROTOCOL coverage gap map

> **Scope:** read-only audit (2026-06-23) of what our user-simulation suites
> *actually exercise* vs what the product *actually supports*. Maps every gap.
> No fixes. Brutally honest: where coverage is claimed-but-synthetic or
> claimed-but-absent, it is flagged as such — under-claiming is the only safe error.

## Suites audited (what each one really is)

| Suite | Path | Black-box? | What it really drives | Runs by default? |
|---|---|---|---|---|
| **Journey suite** | `tests/e2e-user-journeys/` | ✅ HTTP+PAT, no crate imports | **synthetic HTTP** (`reqwest`) shaped like each client. NOT real CLIs. | All journeys **GATE** unless `CORELINK_E2E_PAT_*` env set → default run is all-gated/GREEN-by-vacuum |
| **Real-client moat** | `scripts/e2e-real-client/` | ✅ real binaries | the only suite driving **actual** `docker`/`cargo+sccache`/`brew` + curl for native/bazel/turbo | GATES if no `CLERK_*` secret; many probes GATE if tool absent |
| **Signup flow** | `tests/e2e-signup-flow/` | ❌ in-memory | `corelink_tier_selection` crate, in-process ledgers; real-Stripe path is `#[ignore]` | unit-ish; real path ignored |
| **Pilot onboarding** | `tests/e2e-pilot-onboarding/` | ❌ **internal** | imports the harness that reads **R2 directly** (`cas_blob_count`, `batch_upload_blobs`) — violates black-box | internal integration |

**Headline:** only TWO suites are real-user-facing (journey = synthetic-HTTP, real-client = real-CLI). Pilot/signup are internal/in-memory and must NOT be counted as "real user" coverage.

---

## S2 — Native CAS/AC

Product ops (`routes/cas.rs`, `routes/ac.rs`, `routes/cas_erase.rs`):
`GET/PUT /v1/cas/:t/:hash` · `POST /v1/cas/:t/batch` (write) · `POST .../batch-read` · `POST .../batch-exists` · `GET /v1/cas/:t` (list, D-8) · `GET/PUT /v1/ac/:t/:digest` · `GET /v1/ac/:t` (refs list, D-7) · erase. Digest algo = **blake3 (verified)**.

| Operation | Real-client | Journey (synthetic-HTTP) | Verdict |
|---|---|---|---|
| CAS PUT→GET round-trip | ✅ (b3sum-gated) | ✅ miss→hit | **covered** |
| CAS cross-tenant isolation | ✅ | ✅ | covered |
| **CAS `POST /batch` (bulk write)** | ABSENT | ABSENT (helper `url_cas_batch` exists, unused) | **GAP** |
| **CAS `batch-exists` (bulk HEAD)** | ABSENT | ABSENT (helper exists, unused) | **GAP** |
| CAS `batch-read` | ABSENT | only via DSR erased-read (negative) | **GAP (no positive)** |
| **CAS list `GET /v1/cas/:t` (D-8)** | ABSENT | ABSENT | **GAP** |
| **AC refs list `GET /v1/ac/:t` (D-7)** | ABSENT | ABSENT | **GAP** |
| AC PUT→GET | ✅ | ✅ + divergent-body guard | covered |
| CAS erase→410 | n/a | ✅ (gated, destructive) | covered (gated) |

## S4 — Bazel REAPI v2

Product ops (`routes/bazel_v2.rs`): CAS read/write blob · **AC read/write** (`/blobs/ac/:hash/:size`) · **`POST /findMissingBlobs`** (cap 4096). Digest = **sha256**.

| Operation | Real-client | Journey | Verdict |
|---|---|---|---|
| CAS blob upload→read (sha256) | ✅ curl PUT/GET | (CLI round-trip, gated on `bazel`) | covered (sha256 here) |
| 2nd-build cache HIT | GATED (needs bazel CLI) | GATED | **effectively absent** (bazel rarely present) |
| **Bazel AC read/write** | ABSENT | ABSENT | **GAP** |
| **`findMissingBlobs`** | ABSENT | ABSENT (matrix lists it; never built) | **GAP** |
| instance/tenant isolation | ABSENT | ABSENT | **GAP** |

## S5 — Turbo v8

Product ops (`routes/turbo_v8.rs`): `GET/PUT /v8/artifacts/:hash?teamId` · `POST /events` · `POST /status`. teamId required (missing→400).

| Operation | Real-client | Journey | Verdict |
|---|---|---|---|
| artifact PUT→GET | ✅ curl | ✅ (but **`turbo_storage_finding_gate`** — known prod 500 gates it!) | **degraded/gated** |
| events accept-drop | ABSENT | ✅ | covered (synthetic) |
| status handshake | ABSENT | ✅ | covered (synthetic) |
| RO-write deny / cross-tenant | ABSENT | ✅ | covered (synthetic) |
| **missing-teamId → 400** | ABSENT | ABSENT | **GAP** |
| real `turbo` CLI | ABSENT | ABSENT | **GAP (no real client ever)** |

## S10 — OCI registry

Product ops (`oci/server/dispatch.rs`): `/token` GET+POST two-leg · `GET /v2/` · manifest PUT/GET/HEAD · blob GET/HEAD · **blob uploads: open session (POST), monolithic (PUT ?digest), chunked (PATCH)** · `GET /v2/:name/tags/list` · `_catalog` (disabled→401).

| Operation | Real-client | Journey | Verdict |
|---|---|---|---|
| `/token` two-leg (GET+POST, scopes, challenge) | partial (docker login) | ✅ deep (J1–J9) | covered (synthetic deep) |
| monolithic blob push + manifest + pull | ✅ docker push/pull | ✅ | covered |
| **chunked PATCH upload** | ABSENT (docker may not chunk tiny) | ABSENT (monolithic only) | **GAP** |
| **`tags/list`** | ABSENT | ABSENT | **GAP** |
| `_catalog` disabled→401 | ABSENT | ABSENT | **GAP** |
| RO-push deny | ABSENT | ABSENT (matrix 🔒 not built) | **GAP** |
| cross-tenant repo isolation | ABSENT | ABSENT | **GAP** |

## S6–S9 — cargo / npm / pip / brew

| Surface | Real-client | Journey | Verdict |
|---|---|---|---|
| cargo store/fetch + .sccache_check + RO-deny + x-tenant | ✅ (sccache TLS-gated on Mac) | ✅ rich | **best-covered adapter** |
| **npm tarball PUT/GET + SHA512 integrity** | ABSENT | only anon-deny + public-metadata-200 | **GAP (no real npm, no integrity)** |
| **pip simple-index + file download** | ABSENT | only anon-deny + public-index-200 | **GAP (no real pip)** |
| brew bottle fetch | curl auth-shape only (no install) | single-tenant 200 (no-502 regression) | partial |

## THE MOAT (CLAUDE.md network-effect) — the most important gap

| Moat behavior | Where it should be proven | Status |
|---|---|---|
| `_public` poison-resistance (tenant can't overwrite shared digest) | `abuse.rs::cache_poison_public_shared_rejected` | ✅ **negative only** |
| public-dedup path returns 200 not 502 (npm/pip/brew) | `adapters.rs` public-fetch | ✅ (single-tenant liveness) |
| **POSITIVE cross-team HIT: team A PUTs a public dep → team B GETs = HIT** | NOWHERE | ❌ **ABSENT in every suite** |
| public-vs-private isolation (B can't read A's *private* via shared key) | isolation tests cover private; not the public/private *boundary* | partial |

The single defining claim of the product — "more customers → fuller cache → faster+cheaper for everyone" — has **zero positive test**. brew/npm/pip dedup to `PUBLIC_NAMESPACE` in code, but no test proves tenant B gets a HIT on tenant A's public bytes.

## Digest-function coverage

| Algo | Surface | Driven by a real address? |
|---|---|---|
| blake3 | native CAS | ✅ real-client (b3sum) + journey | 
| sha256 | Bazel REAPI | ✅ real-client curl + journey edge | 
| sha256 (opaque) | AC / turbo / cargo keys | ✅ (opaque, not verified) |
| **SHA1 (npm tarball `dist.shasum`)** | npm | ❌ never driven (no real tarball fetch) |
| **SHA512 (npm)** | npm (matrix S7) | ❌ never driven |

---

## TOP missing-coverage items (ranked, with WHY it matters to a real customer)

1. **POSITIVE cross-team `_public` dedup HIT — completely untested.**
   *Why:* this IS the moat and the margin story. If team A's `lodash`/`alpine`/a public crate does not actually serve team B a HIT, the network-effect, the COGS win, and the whole expansion thesis are unproven. Today we only prove the *negative* (can't poison). A regression that silently per-tenant-isolates public bytes would pass every test and quietly kill the margin.

2. **Bazel `findMissingBlobs` + Bazel AC read/write — never exercised by anything.**
   *Why:* `findMissingBlobs` is the op every real `bazel build` hits FIRST on every build; AC is what makes a build a cache HIT instead of a re-run. We test a raw blob PUT/GET but not the two ops that make Bazel actually fast for a customer. The CLI round-trip that would cover them GATES (bazel binary absent) in practice.

3. **CAS batch plane (`batch` write, `batch-exists`) — helpers exist, no journey calls them.**
   *Why:* batch is the hot path the CLAUDE.md latency memo built specifically to fix the 3–7s CAS blocker (bulk PUT + budget-lease). It is the path real high-throughput clients (hugit, CI) will hammer. Zero real-user coverage on the very ops added for performance/correctness. `batch-read` is touched only negatively (DSR).

4. **No real package-manager client for npm / pip / turbo (and brew never installs).**
   *Why:* the suites prove anon-deny and a public 200, but never that a real `npm install` / `pip install` / `turbo` round-trips, and never the npm tarball **integrity** (SHA1/SHA512) reject path. These are the surfaces a self-serve SMB actually points their tooling at on day one; synthetic-HTTP "looks like npm" has repeatedly passed while real CLIs failed (per memory: OCI/docker/cargo/brew "never real-client-tested; curl passes while real CLIs fail").

5. **Turbo artifact round-trip is GATED on a known prod-500, and no enumeration/list/tags ops anywhere.**
   *Why:* the journey suite *self-disables* the Turbo happy path via `turbo_storage_finding_gate` (a known live 500) — so a paying Turbo customer's core flow is currently un-asserted, not just untested. Plus OCI `tags/list`, CAS list (D-8), AC refs list (D-7), and missing-teamId→400 — all real client behaviors / D-7/D-8 contracts — have no coverage at all.

### Secondary gaps (real but lower blast radius)
- OCI chunked **PATCH** upload (large-layer path), `_catalog`→401, OCI RO-push deny, OCI cross-tenant repo isolation.
- pilot-onboarding + signup suites are **not** black-box (R2-direct / in-memory) — must not be counted toward real-user coverage; real-Stripe path is `#[ignore]`.
- Journey suite default-runs **all-gated → GREEN by vacuum** unless the `CORELINK_E2E_PAT_*` token map + tenant are provisioned; "green" here can mean "ran nothing."
- npm/brew public bytes stored per-tenant-namespaced for npm (dedup is a *code TODO*), so even the dedup *mechanism* differs by adapter and is untested per-adapter.
