# CoreLink ⇄ hugit P2 — response + techlead WAVE PLAN

> **From:** CoreLink TechLead · **Date:** 2026-06-11 · **Status:** OPEN (execution)
> **In response to:** `hugit/docs/handoff/2026-06-11-corelink-p2-ceiling-request.md`
> (the 6 seams A–F) + the prior tenant (`2026-06-08`) + identity (`2026-06-09`) asks.
>
> Owner directive (2026-06-11): *close ALL of it, no exception — and there are
> more gaps than the six.* This doc is the cold-verified gap inventory + the
> disjoint, contract-frozen wave plan. The build runs **in parallel with the
> launch (DSR Wave 1)**, under strict non-interference, Mac-aware (no CI storms,
> no orbiting infinite CI — local incremental verify, merge with a documented
> reason per the repo's CI-starved doctrine).

## 0. Tense-discipline assessment (what exists vs not — cold-verified)

| Seam | CoreLink state today | Evidence |
|---|---|---|
| **A** AC HTTP transport | **~90% EXISTS** — `GET/PUT /v1/ac/:tenant/:action_digest`, PAT→tenant, 403 cross-tenant, idempotent PUT. **Missing only 409-on-divergent-body.** | `routes/ac.rs:61,214`; `corelink-handler-ac/src/{error,handler}.rs` |
| **B** CAS per-hash erase + 410 | **ABSENT as public API.** Machinery EXISTS (DSR R2 refcount-safe + D1 purge, tenant-wide internal). | `routes/cas.rs:179` (GET/PUT only); DSR `routes/dsr/` |
| **C** session→token exchange | **ABSENT.** Clerk verify is Worker-edge; container mint `/_internal/pat/mint` EXISTS. | `worker/src/lib/clerk_auth.ts`; `routes/internal_pat.rs:220` |
| **D** per-repo event-log DO | **ABSENT.** Only `CoreLinkServer` + `RolloutController` DOs. | `worker/src/{durable_object,rollout_controller}.ts` |
| **E** transparency log (Rekor) | **ABSENT.** Per-tenant audit-export w/ Merkle proofs exists; no public witness. | `routes/audit_export.rs` |
| **F** secrets broker lease | **ABSENT** + partly platform-constrained (CF secrets write-only). | CLAUDE.md; `worker` env model |

## 1. The gaps BELOW the six (the "more surprises" — cold-verified)

- **G0 — FOUNDATION (blocks every seam).** The hugit tenant is **not provisioned**:
  no `tenant` row, no `pat` row, no `pilot_signups` row. The operator endpoint
  `POST /v1/admin/pilots` (create non-Clerk pilot tenant + mint PAT atomically)
  is **design-ratified but NOT implemented** (PROPOSAL-2026-06-10 / the admin-pilot
  task). `/_internal/pat/mint` needs the prod secret `CORELINK_INTERNAL_AUTH_KEY`
  (owner-gated, task #46). *Every A–F smoke test sits on this.*
- **G1 — monthly-$-ceiling enforcement is a GAP.** Per-tenant **rate-limit EXISTS**
  and is fully wired (`ratelimit_buckets` + tier map: free→200 RPS/1000 burst),
  but there is **no per-month $ cap at the request boundary** (`tenant_quota` table
  absent; deferred to S-08 CAP-QUOTA-001). This is precisely the *preventive
  non-interference bound* hugit's §8 / X6/X10⑤ requires before keys are handed out.
- **Non-interference probes X6/X10/X11** ride A + the tenant + G1 (no new primitive).
- **Confirmed OUT of CoreLink scope** (matched the request's matrix): GitHub detect +
  App token (GitHub infra), runner fabric exec (corelink-runners, frozen contract
  `hugit-integration-contract.md`), cold trajectory-blob bytes (owner: commodity
  cold store, not CoreLink hot tenant). hugit's recorder verb (PS-1) is hugit-side.

## 2. WAVE PLAN — disjoint, contract-frozen, dependency-ordered

Baseline `@ d8fa331b` (branch `feat/dsr-account-deletion`). Orchestrator (me) owns
all route-registration / wiring scaffold (the shared files `routes.rs` / `main.rs` /
the Worker router) to keep WPs conflict-free (AP-1). Each WP: agent writes its own
module(s); I wire + cold-check + verify.

| WP | Seam | Owner-files (disjoint) | Size | Dep-on | Non-interference |
|---|---|---|---|---|---|
| **WP-FOUND-1** | G0 admin/pilots endpoint | `routes/admin_pilot.rs` (new) + D1 provisioning | M | task #46 secret (live only) | ISOLATED (internal-auth route) |
| **WP-FOUND-2** | G1 $-ceiling | `tenant_quota` migration + quota middleware | M | ADR (design first) | the cap itself — protects launch |
| **WP-A** | A — AC 409 | `corelink-handler-ac/src/{error,handler}.rs` + `routes/ac.rs` (409 map) | **S** | — | ISOLATED |
| **WP-C** | C — session exchange | `worker/src/lib/session_exchange.ts` + Worker route | **S** | reuses `/_internal/pat/mint` | ISOLATED |
| **WP-B** | B — CAS erase + 410 | `routes/cas_erase.rs` + `corelink-handler-cas-erase` (new crate) + `storage/r2_s3.rs` erase | M | **DSR Wave 1 + R2 CAS adapter (DSR incr-3)** | write-side, OFF the hot GET path; cold R2 bucket |
| **WP-D** | D — event-log DO | ADR first → `worker/src/event_log_do.ts` | M | **ADR-D decision** | ISOLATED |
| **WP-E** | E — transparency log | ADR first (integrate sigstore/Rekor vs build vs defer) | L | **ADR-E decision** | ISOLATED (async, post-hoc) |
| **WP-F** | F — secrets broker | ADR first (D1-encrypted-lease vs platform-defer) | L | **ADR-F decision** | TOUCHES (mitigate w/ TTL cache) |

### Sequencing (Mac-aware, launch-first)
1. **Mac-light NOW (judgment, zero compile):** ADR-D / ADR-E / ADR-F (the architecture
   calls) + the G1 $-ceiling design. Freeze A/B/C contracts. *(This wave runs during
   the CI storm; it needs no Mac.)*
2. **DSR Wave 1 finishes first** (already in flight; its verify is queued for Mac
   headroom). B rides it; do not build B before DSR's R2 CAS adapter lands.
3. **WP-A + WP-C** (cheap, isolated, S) — build + local-verify when Mac frees;
   batch their `cargo check` with the DSR verify (one compile covers handler-ac +
   container).
4. **WP-FOUND-1 + WP-FOUND-2** — the foundation; FOUND-1 live-activation is
   owner-gated (task #46 secret), code is buildable now.
5. **WP-B** after DSR lands; **WP-D/E/F** per their ratified ADRs.

### Non-interference caps (set per the hugit §8 ask)
- hugit tenant = **free tier** → 200 RPS / 1000 burst (existing `ratelimit_buckets`).
- **G1 $-ceiling** is the preventive monthly bound (symbolic $5 tripwire per the
  tenant doc; $0 dogfood). hugit storm is structurally bounded *before* it can test
  CoreLink's fairness layer.
- hugit's seams are AC / CAS-erase / auth — **none touch the launch route, billing
  money-path, or campaign-1/2 criticals.** B is write-side on the cold R2 bucket.

## 3. Architecture decisions I will make as techlead (ADRs — owner may veto)

- **ADR-D (event-log DO):** does CoreLink *host* hugit's per-repo append-only log,
  or is that hugit's domain with CoreLink providing only the DO primitive? Lean:
  provide a thin generic append-only DO primitive; hugit owns the chain semantics.
- **ADR-E (transparency log):** **integrate** a Rekor/sigstore public log (don't
  rebuild a transparency service); CoreLink submits the chain head + returns the
  inclusion handle. Build = thin submission adapter, not a log.
- **ADR-F (secrets broker):** CF secrets are write-only → a true "broker" is a
  **D1-encrypted-lease** design (KMS-derived, TTL, audit) OR deferred as out-of-
  platform with the flat-file PAT as the sanctioned interim. Lean: design the
  D1-lease, stage behind the foundation; flat-file PAT remains interim until then.

## 4. CI discipline (owner mandate: no stupid CI, no infinite-CI hang)
Local incremental `cargo check -p corelink-server` (and the fast TS suites) **only
when the Mac has headroom** (gate: 1-min load < 30 AND concurrent cargo ≤ 3 — the
os-error-2 race needs a quiescent shared target). Merge with a documented
local-verify reason per the CI-starved doctrine; never orbit CI. Sequence builds —
never a storm. The shared-target race is itself fixable by merging
`fix/ci-prebuilt-tool-installs` (flagged separately).

---

*Cross-refs: the request `hugit/docs/handoff/2026-06-11-corelink-p2-ceiling-request.md`;
the DSR work `ADR-S11-010` + `crates/corelink-container/src/routes/dsr/`; the admin
foundation PROPOSAL-2026-06-10 (admin-pilot tenant).*
