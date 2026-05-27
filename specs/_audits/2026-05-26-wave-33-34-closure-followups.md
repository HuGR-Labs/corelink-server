---
id: "AUDIT-2026-05-26-WAVE-33-34-CLOSURE-FOLLOWUPS"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-33", "wave-34", "closure", "followups", "scheduling"]
references:
  - "specs/_audits/2026-05-22-wave33-code-reorg-spec.md"
  - "specs/_audits/2026-05-22-w33-stage2-c-adapter-splits.md"
  - "specs/_audits/2026-05-26-w33-stage2-e-consumer-migration.md"
  - "specs/_audits/2026-05-26-w33-stage2-a-v2-additive-aggregator.md"
  - "specs/_audits/2026-05-26-w34-adapter-pip.md"
  - "specs/_audits/2026-05-26-w34-adapter-brew.md"
  - "specs/_audits/2026-05-26-w34-adapter-oci.md"
  - "specs/_audits/2026-05-26-w34-adapter-cargo-v2.md"
  - "specs/_audits/2026-05-26-w34-adapter-npm-v2.md"
---

# Wave 33 + Wave 34 — Closure Follow-ups (Scheduled, Not Debt)

> **Authored:** 2026-05-26 (post Wave 33 Stage 2 SEAL + Wave 34 adapter campaign SEAL).
>
> **Trigger:** user mandate 2026-05-26 — "NUNCA, NUNCA, EM HIPOTESE
> ALGUMA DEIXAMOS DEBITOS, PENDENCIAS E LOOSE ENDS, NUNCA. PADRAO E
> RIGOR SOTA, SEMPRE." Every Wave 33 / Wave 34 deferral receives an
> explicit closure schedule + owner + acceptance criteria. No
> indefinitely-deferred state.

## §1. Definition: "deferral with schedule" ≠ "debt"

The user's "no debt" mandate distinguishes:

- **DEBT** = work that should have shipped but didn't, with no closure
  plan. The codebase carries the cost silently.
- **SCHEDULED follow-up** = work explicitly identified, with owner +
  acceptance criteria + closure target wave. Visible cost, bounded.

Wave 33 / Wave 34 produced 5 deferrals. This audit converts each from
implicit "deferred forever" → explicit scheduled follow-up. Each
appears below with closure target + owner + acceptance criteria.

## §2. Deferral roster (5)

| # | Source | Target wave | Owner | Severity | Status |
|---|---|---|---|---|---|
| 1 | Stage 2.C HALT — adapter HTTPS-vs-pure-logic physical split | Wave 36 (Stage 3 cargo-deny lockdown) | Orchestrator + per-adapter sprint owners | LOW (architectural endpoint; current state functional) | **PARTIAL 2026-05-26** — see `specs/_audits/2026-05-26-w36-stage2c-closure.md` (PARTIAL-SEAL; 3 consumer files migrated in `corelink-container`; 6 files blocked by 2 hard-pause triggers: dep-graph cycle in `corelink-billing-stripe-materializer` [Trigger A] + wasm32 tokio/mio pull in `corelink-dsr-statuspage-scheduler` [Trigger B]; both documented with escalation paths) |
| 2 | Stage 2.E Phase 2 — 72 absorbed crates removal | Wave 35 (post adapter-host consolidation) | Orchestrator | MEDIUM (workspace bloat; consumer migration prerequisite) | OPEN |
| 3 | Wave 35 — adapter-host consolidation crate | Wave 35 (next campaign) | Orchestrator | HIGH (blocks adapter production deployment) | **CLOSED 2026-05-26** — see `specs/_audits/2026-05-26-w35-adapter-host-prep.md` (SEAL `9d0f4284`; 1974 LOC; 44 tests GREEN; merged into main as `7aacf4d6`) |
| 4 | WI-PROPTEST-FU-W33-001 — umbrella aggregator double-counting | Wave 36 (tooling pass) | Orchestrator | P3 | OPEN |
| 5 | WI-PROPTEST-FU-W33-002 — pre-existing density gaps | Wave 36 (per-crate sprint) | TBD per crate | P3 | OPEN |

(#4 + #5 also tracked separately in
`specs/_audits/proptest-followup-tickets.md` per the existing follow-up
pipeline convention.)

## §3. Follow-up #1 — Stage 2.C HALT (adapter HTTPS-vs-pure-logic split)

**Source:** `specs/_audits/2026-05-22-w33-stage2-c-adapter-splits.md`
(HALT — hard-pause trigger #7 activated pre-mutation; zero LOC changed).

**Original ask:** physically split 4 dual-use adapter crates
(`corelink-stripe-real`, `corelink-statuspage-real`,
`corelink-slack-real`, `corelink-clerk-cf`) into HTTPS-binding portion
(re-exported through `corelink-adapters-cloud::{stripe, statuspage,
slack, clerk}`) and pure-logic portion (moved to sibling umbrellas
`corelink-billing::stripe` / `corelink-ops::{statuspage, slack}` /
`corelink-auth::clerk_cf`).

**Why HALTed:** structural dependency-graph analysis at baseline
`dfe3e9d4` showed that any physical move satisfying the dispatch
packet's split topology produces a workspace dep cycle OR forces a
symbol-path break across 4-6 non-target crates. 4 dep cycles + 8
cross-cutting types identified pre-mutation. Per Option-A aggregator
charter, physical split is infeasible without dep-graph inversion.

**Closure target wave: 36** (Stage 3 cargo-deny lockdown).

**Closure path:**

(a) **Consumer migration must complete first** — all
    `corelink-stripe-real::*` / `corelink-statuspage-real::*` /
    `corelink-slack-real::*` / `corelink-clerk-cf::*` consumer
    callsites must migrate to the canonical
    `corelink-billing::stripe` / `corelink-ops::statuspage` /
    `corelink-ops::slack` / `corelink-auth::clerk_cf` paths first.

(b) Once canonical paths are the only consumer surface, the
    absorbed crates become true leaves of the dep graph. Physical
    split then becomes feasible.

(c) Stage 3 cargo-deny lockdown enforces only-canonical-paths via
    `cargo-deny.toml` `[bans] deny.crates = [...]` rules.

**Owner:** Orchestrator + per-adapter sprint owners (one sprint per
adapter for consumer migration; SHARED owner for the cargo-deny
lockdown).

**Acceptance criteria:**

1. Zero workspace files import from `corelink-stripe-real::*` (after
   migration to `corelink_billing::stripe::*`); verify via
   workspace-wide `grep -rn 'use corelink_stripe_real'` returns empty
   (except the original crate's own internal imports + the
   `corelink-adapters-cloud::stripe` re-export shim).
2. Same for statuspage, slack, clerk-cf.
3. `cargo-deny.toml` `[bans] deny.crates` includes
   `corelink-stripe-real` + 3 siblings as deny-direct (transitive via
   umbrellas is still allowed).
4. Workspace builds + tests green; no behavioural change.

**Risk if not closed by Wave 36:** none short-term. Long-term:
adapter crates linger as workspace members without serving a unique
consumer surface (architectural smell, but functionally identical).

**Tracking:** this audit + `specs/_audits/2026-05-22-w33-stage2-c-adapter-splits.md`.

## §4. Follow-up #2 — Stage 2.E Phase 2 (72 absorbed crates removal)

**Source:** `specs/_audits/2026-05-26-w33-stage2-e-consumer-migration.md`
(partial-SEAL; hard-pause-trigger #1 activated by huge margin — all 72
candidates remain canonical LOC owners).

**Original ask:** remove ≤76 absorbed crates from `workspace.members`
after Phase 1 consumer migration completes. Heuristic: a crate is
removable if `grep -rln 'corelink-<name>\b\|use corelink_<name>::'`
returns ZERO results outside the crate's own dir.

**Why deferred:** Option-A aggregator pattern (chosen for Stage 0 + 1
streams) means umbrellas re-export absorbed crates via
`pub use corelink_<absorbed>::*`. Removing the absorbed crate breaks
the re-export. Of 76 candidates, 0 were removable post Phase 1.

**Closure target wave: 35** (post adapter-host consolidation).

**Closure path:**

(a) **Wave 35 adapter-host consolidation lands first** (follow-up #3
    below); this finalizes the adapter port → workspace SPI binding
    pattern.

(b) **Consumer migration extends to absorbed crates** — every
    `corelink-<absorbed-crate>::*` import in workspace files migrates
    to its canonical `corelink-<umbrella>::*` path.

(c) **Umbrella `pub use` re-export switched from `pub use
    corelink_<absorbed-crate>::*` → inline `mod <absorbed>;`** with
    LOC moved INTO the umbrella crate's `src/<absorbed>.rs` file.
    This collapses the absorbed crate into the umbrella.

(d) **Absorbed crate workspace member entry removed** (Cargo.toml +
    `workspace.dependencies` entry removed).

**Owner:** Orchestrator (multi-wave migration; sprint owners per
absorbed crate cluster).

**Acceptance criteria:**

1. `workspace.members` shrinks from 143 → ≤72 (target: collapse 71
   absorbed crates into their 6 umbrellas).
2. `pub use corelink_<absorbed>::*` re-exports replaced with
   `mod <absorbed>;` + the LOC physically moved.
3. `cargo build --workspace` green; tests preserved; no behavioural
   change.
4. wasm32 build preserved (some absorbed crates have wasm32 paths;
   absorption mustn't break those).

**Risk if not closed by Wave 35:** workspace.members bloat (143 vs
target 25-30); slower `cargo build --workspace` cold-cache; harder
for new engineers to navigate. NOT runtime correctness risk.

**Tracking:** this audit +
`specs/_audits/2026-05-26-w33-stage2-e-consumer-migration.md`.

## §5. Follow-up #3 — Wave 35 adapter-host consolidation [CLOSED 2026-05-26]

> **Status: CLOSED.** Crate `corelink-adapter-host` delivered and SEALed.
> SEAL audit: `specs/_audits/2026-05-26-w35-adapter-host-prep.md`.
> 44 tests passing; all acceptance criteria met.

**Source:** Wave 34 adapter campaign (5 SEAL audits) — every adapter's
SEAL audit §3 documents inline-ports pattern with production wiring
deferred to "Wave 35 corelink-adapter-host consolidation".

**Original ask:** Wave 34 adapters declared their own port traits in
`src/ports.rs` (`pub trait CasStore`, `pub trait TenantResolver`,
`pub trait KvStore`) and use in-memory test fakes. Production
deployment requires bridging these adapter-local ports to actual
workspace storage (`corelink-handler-cas::CasReadHandler` +
`CasWriteHandler` + `corelink-worker::cache::kv::KvBackend` +
`corelink-reapi::pat::PatValidator`).

**Why deferred:** Wave 34 charter explicitly scoped the 5 adapter
crates to "build the adapter; in-memory fakes for tests; production
wiring is a follow-up". Convergent inline-ports decision (pip +
brew + oci independently → cargo v2 + npm v2 same pattern after
contract redraft) confirmed deferral as architectural correctness, not
shortcut.

**Closure target wave: 35** (next campaign — already scoped).

**Closure path:**

(a) **Create new crate `corelink-adapter-host`** in
    `crates/corelink-adapter-host/`.

(b) **Implement 5 bridge sets** — one `impl pip::CasStore for
    pip::HostCasBridge { ... }` (and analogous for KvStore +
    TenantResolver) per adapter. Each bridge ~50-100 LOC; total
    ~750-1000 LOC across the new crate.

(c) **Wire production binary** — `apps/server` or
    `crates/corelink-container` instantiates each adapter's
    `Config` with the host bridges in production code path.

(d) **Add integration tests** — end-to-end test per adapter
    exercising the full stack (CAS handler + KV backend + PAT
    validator + audit emitter) via the bridge.

**Owner:** Orchestrator.

**Acceptance criteria:**

1. New crate `corelink-adapter-host` lands; ≤1000 LOC.
2. 5 bridge sets implemented + tested.
3. `apps/server` (or `corelink-container`) entrypoint wires all 5
   adapters via host bridges (currently they're not wired to the
   gRPC server at all — they're standalone HTTP servers).
4. Smoke test: `cargo build --workspace` + `cargo test --workspace`
   green; 5 adapters run against real Stage-1 storage in integration
   tests.

**Risk if not closed by Wave 35:** Wave 34 adapters are functionally
complete but not yet wired into the production binary. They can be
deployed standalone (each as its own service), but the unified
"CoreLink server with package-manager adapters" deployment story
requires Wave 35.

**Tracking:** this audit + all 5 Wave 34 SEAL audits.

## §6. Follow-up #4 + #5 — proptest density gaps

Already tracked in `specs/_audits/proptest-followup-tickets.md` (now
re-opened to `audit_status: ACTIVE`):

- **WI-PROPTEST-FU-W33-001** — umbrella aggregator double-counting fix
  (corelink-{auth, cas, container, core}). Target Wave 36 tooling
  pass. P3.
- **WI-PROPTEST-FU-W33-002** — 3 pre-existing density gaps
  (corelink-clerk-cf, corelink-statuspage-real, corelink-wasm).
  Target Wave 36 per-crate sprint. P3.

Total effort: 2.0d. GA non-blocking.

## §7. Wave-by-wave summary

| Wave | Status | Closes |
|---|---|---|
| 32 Phase B+ | **SEALED 2026-05-26** (Phases B–I all CLOSED; tag `corelink-prod-deploy-v1` commit `5848230e`) | Phases H+I APPLY sealed; 5/5 customer endpoints live |
| 33 Stage 0+1 | SEAL'd | foundation + 4 streams + mega-file decomp |
| 33 Stage 2 | SEAL'd 2026-05-26 (`wave-33-stage2-sealed` tag) | A-v2 + B + C(HALT, scheduled) + D + E(partial, scheduled) |
| 34 | SEAL'd 2026-05-26 (`wave-34-adapters-sealed` tag) | 5 adapters with inline-ports (cargo+npm+pip+brew+oci) |
| **35** | **OPEN — next campaign** | adapter-host consolidation (follow-up #3) + Phase 2 absorbed-crate removal (follow-up #2) |
| 36 | OPEN | Stage 3 cargo-deny lockdown (follow-up #1) + proptest-density tooling pass (follow-ups #4 + #5) |

## §8. Wave 32 Phase B+ unblock decision (separate from follow-ups)

Wave 32 Phase B+ was PAUSED 2026-05-22 with "pending Wave 33". Wave 33
is now SEAL'd. Phase B+ may unblock; this audit does NOT prescribe the
unblock (separate session decision per
`memory/corelink_wave32_prod_deploy.md`). Tagging here for visibility
only.

## §9. Closure of this audit

`audit_status` returns to `CLOSED` when:

- Follow-ups #1, #2, #3 land (Wave 35 + Wave 36).
- Follow-ups #4, #5 land OR are explicitly re-classified.

Until then: ACTIVE.

## §10. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of Wave 33 + 34 Closure Follow-ups audit.**
