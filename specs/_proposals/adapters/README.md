---
id: "PROPOSAL-2026-05-26-ADAPTER-CONTRACTS-INDEX"
type: "governance"
doc_status: "DEFASADO"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-05-26"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["proposal", "adapters", "wave-34", "package-managers", "index"]
references:
---

> **DEFASADO 2026-05-27 — never landed.** This proposal/draft was scoped but did not advance to implementation. Preserved as historical record; no current code references it. See `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.2 for the inventory triage decision.

# Adapter Contracts — Wave 34 (Adapter Campaign)

> **Authored:** 2026-05-26 (post Wave 33 Stage 2 in flight).
>
> **Trigger:** external evaluation revealed CoreLink GA delivers ~30% of the
> vision (CAS + REAPI + compliance); the missing 40-50% is package-manager
> adapters + public shared namespace + cross-tenant dedup. This dir holds
> the 5 adapter contracts that close the package-manager-adapter gap.
>
> **Authority:** these are PROPOSALS (`_proposals/`); promoted to canonical
> spec corpus only after orchestrator + Owner approval per adapter.

## Why these contracts exist as proposals first

Friend's evaluation 2026-05-26: "as specs estão absurdamente bloated". Empirical:
841 .md / 253K LOC in `specs/`, 3 mega-files >100KB, 10+ files 50-100KB. The
existing `specs/04_sprints/SXX/work_items/` average is ~75KB per WI — that's
the anti-pattern.

These adapter contracts target **a different bar**: lean, executable,
pre-digested, zero-ambiguity, agent-executes-without-decisions. The agent
that picks up a contract from this directory will need:

1. The contract itself (≤500 LOC structured markdown)
2. The upstream protocol spec it bridges (URL, single reference)
3. The CoreLink REAPI surface it consumes (`crates/corelink-reapi`)
4. The CoreLink CAS surface (`crates/corelink-cas`)
5. Standard charter (`/techlead` skill)

That's it. No "read 8 audit docs first" / no "decide between option A and B" /
no "design the trait interface". The contract IS the design.

## The 5 contracts

| # | File | Adapter | Upstream Spec | LOC Target | Status |
|---|---|---|---|---|---|
| 1 | `cargo.md` | cargo build cache | Cargo Book §16 | ~400 LOC | **REDRAFT v2** (re-dispatch pending) |
| 2 | `npm.md` | npm package cache | npm CLI registry API | ~400 LOC | **REDRAFT v2** (re-dispatch pending) |
| 3 | `pip.md` | pip wheel cache | PEP 503 (Simple Index) + PEP 691 (JSON) | ~400 LOC | **SEALED** (`98170fb3` — 57 tests; inline-ports) |
| 4 | `brew.md` | brew bottle cache | Homebrew bottle DSL + GitHub Packages OCI | ~350 LOC | **SEALED** (`d1a275f9` — 27 tests; inline-ports) |
| 5 | `oci.md` | OCI registry / Docker | OCI Distribution Spec v1.1 | ~500 LOC | **SEALED** (`3d3788dd` — 59 tests; inline-ports) |

## Architectural pattern — inline-ports (convergent agent decision, 2026-05-26)

**Status:** canonical for Wave 34 after **3-way independent convergence** (pip / brew / oci agents
each arrived at the same design without coordination on `wt/r-prep-w34-adapter-{pip,brew,oci}`).

**v1 packet error:** the original contract drafts referenced trait surfaces that do not exist
in the workspace (`corelink_cas::CasStore`, `corelink_auth::TenantResolver`,
`corelink_adapters_cloud::cf::kv::KvStore`). Cargo + npm agents HALTed pre-mutation per
"executa, não decide". Pip + brew + oci agents each independently chose to declare minimal
**adapter-local port traits** in `src/ports.rs` (parallel-safe; no umbrella mutation; isolated
adapter domain).

**Why inline-ports is SOTA (not a workaround):**

- Hexagonal architecture purity: adapter defines its own ports in its own domain language
  (`TenantId` + `Digest` Rust-native types; async uniform; TTL with timestamp for pip's KV).
- Workspace SPI (`CasReadHandler` + `CasWriteHandler` + `KvBackend` + `PatValidator`) is
  intentionally split-API / mixed sync-async — exposing it directly at the adapter boundary
  would leak workspace internals into the adapter modules.
- Each port surface is shaped by the adapter's specific needs (brew has no KV; oci has
  `ManifestKvStore` specific; pip's KV returns `(value, ts)` for freshness check).
- Production wiring (bridging adapter ports → workspace SPI) is the load-bearing translation
  tier — not debt, but the correct architectural layer.

**Canonical reference templates** (merged on main):

- `crates/corelink-adapter-pip/src/ports.rs` — async `CasStore` + `KvStore` + `TenantResolver`
  (most comprehensive; includes TTL-aware KV).
- `crates/corelink-adapter-brew/src/ports.rs` — async `CasStore` + `TenantResolver` (CAS-only;
  no KV needed).
- `crates/corelink-adapter-oci/src/ports.rs` — async `BlobStore` + `ManifestKvStore` +
  `TenantResolver` (with in-mem fakes for upload-session state).

**Wave 35 consolidation campaign:** extract `corelink-adapter-host` crate that provides
production bridges (one `impl pip::CasStore for CasHandlerBridge` per adapter, ~50 LOC each)
once all 5 adapters have stabilised. Pre-empted at Wave 34: lock the inline-ports pattern
NOW so cargo + npm land consistent with pip/brew/oci.

**Mandate for re-dispatched cargo + npm contracts:**

- Declare adapter-local `pub trait CasStore` / `pub trait TenantResolver` (and `KvStore` if
  needed) in `src/ports.rs` matching the pip/brew/oci shape (async, tenant-scoped,
  domain-native types, in-memory test fake colocated).
- Do NOT bind to workspace traits directly. Do NOT mutate umbrella `lib.rs` files.
- Document the inline-ports decision in the adapter's SEAL audit §3 (same template as
  `specs/_audits/2026-05-26-w34-adapter-{pip,brew,oci}.md`).
- Production bridges deferred to Wave 35 explicitly.

## Shape of each contract (template)

Every contract MUST contain these 10 sections, in this order:

1. **Headline + scope.** One paragraph: what this adapter does, who consumes it.
2. **Upstream protocol summary.** 5-10 lines, with single canonical URL.
3. **Mapping to CoreLink.** Table: upstream operation → CoreLink REAPI/CAS call.
4. **Crate structure.** Target crate name, file layout, dep graph (deps on other CoreLink umbrellas).
5. **Trait interface.** Public Rust traits + types the adapter exposes (sketched, not exhaustive).
6. **Auth + multi-tenancy.** How the tool's single-cache model maps to CoreLink's per-tenant prefix.
7. **Cache invalidation.** When does a cached artifact stop being valid?
8. **Tests.** Acceptance tests (smoke flow with real tool against CoreLink mock); property tests; adversarial tests.
9. **Acceptance criteria.** "Agent is done when..." checklist. No subjective items.
10. **Out of scope / explicit deferrals.** What this adapter doesn't do; what's a future iteration.

DCO sign-off + Co-Authored-By trailer at the bottom.

## Dispatch model

After Stage 2 SEAL, orchestrator dispatches **5 Sonnet agents in parallel**
(one per adapter), each with its contract as the dispatch packet. Worktrees
isolated; no shared mutation surfaces. Crate count goes from ~25 (post Stage
2) to ~30 (5 new adapter crates).

**Hard constraint:** each agent only consumes its own contract. No cross-
adapter references. If a contract is ambiguous, agent HALTs + escalates;
contract drafting is iterated on the orchestrator side.

## Charter alignment

- `#![forbid(unsafe_code)]` on every new crate.
- L2.10: every file ≤500 LOC HARD CAP (per-file, not per-crate; adapters
  may be 1000-2000 LOC distributed across files).
- `#[non_exhaustive]` on every public type.
- `SecretString` for credentials (npm tokens, brew github-pat, OCI bearer).
- Audit fail-CLOSED on every state mutation (CAS PUT, manifest write).
- Trait-abstraction-defer: pure-logic trait in crate; HTTPS adapter at
  binding boundary (`corelink-adapters-cloud` if applicable).
- TLA+ specs for any state machine in the adapter (e.g., OCI manifest
  finalization, npm tarball integrity verification).

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Adapter Contracts README.**
