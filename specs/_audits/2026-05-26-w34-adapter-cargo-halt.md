---
id: "AUDIT-2026-05-26-W34-ADAPTER-CARGO-HALT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "CLOSED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
closed: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: "specs/_audits/2026-05-26-w34-adapter-cargo-v2.md"
tags: ["audit", "wave-34", "adapter", "cargo", "halt", "hard-pause", "trigger-4"]
references:
  - "specs/_proposals/adapters/cargo.md"
  - "specs/_audits/2026-05-22-w33-stage2-c-adapter-splits.md"
  - "specs/_audits/2026-05-22-w33-stream-a-data-path.md"
  - "specs/_audits/2026-05-26-w34-adapter-cargo-v2.md"
---

**Title:** Wave 34 Adapter Campaign — Cargo (sccache-compatible) — HALT (preserved historical record)

**Status:** HALT (hard-pause trigger #4 activated pre-mutation) → SUPERSEDED by `cargo-v2` v2 inline-ports redispatch (SEAL `23a44535`).

**Original branch:** `wt/r-prep-w34-adapter-cargo` (now deleted; HALT commit `21eb0dbb` cherry-picked into this audit doc only — the branch itself produced zero adapter code).

**Baseline:** `a1c49678` (post wave-33 stage 2.A-v2 SEAL) — agent self-corrected from stale baseline `99269ed0`, avoiding 2.D-v1's 7-commit-loss mode.

---

# §1. Scope (as dispatched)

Wave 34 dispatch packet `specs/_proposals/adapters/cargo.md` (authored 2026-05-26)
instructs the agent to land a brand-new crate `crates/corelink-adapter-cargo/`
bridging sccache's HTTP storage wire to CoreLink's CAS, auth, and audit
chokepoints. Estimated end-state ~1050 LOC across 9–10 files, all ≤500 LOC per
L2.10, with ≥10 tests across smoke / property / adversarial tiers.

The contract names three upstream trait surfaces the new crate must consume
verbatim per its §4 dep-graph and §5 trait-interface block:

| Contract reference | Purpose |
|---|---|
| `corelink_cas::CasStore` — `get(tenant_id, Digest) -> bytes`, `put(tenant_id, Digest, bytes)`, `exists(tenant_id, Digest)` | CAS storage backend the adapter calls per HTTP method |
| `corelink_auth::TenantResolver` | Resolves `Authorization: Bearer hugr-pat_…` → `TenantId` |
| `corelink_audit::ports::AuditEmitter::emit(...)` | Fail-CLOSED audit before each state mutation |

The contract further claims test-side fakes `InMemoryCasStore`,
`InMemoryTenantResolver`, `InMemoryAuditEmitter` "already exist" in the
respective crates (§5 trailing paragraph).

# §2. Self-correction logged (worktree baseline)

`HEAD` on entry was `99269ed02f8e356dcff86a6c5abe560d752af172` — the *exact*
stale SHA the dispatch packet's Step-0 warning calls out as the mode that cost
2.D-v1 seven commits. Per the mandatory protocol:

```
git fetch origin
git checkout -B wt/r-prep-w34-adapter-cargo a1c49678
git rev-parse HEAD   # → a1c49678231e32ac88cd63470b0d422f9eb7a90a ✓
```

Self-correction succeeded. No work was performed on the stale baseline.

# §3. Hard pause trigger #4 — activated **pre-mutation**

Per dispatch packet hard pause triggers list (§"Hard pause triggers" in this
agent's contract):

> 4. `corelink-cas`/`corelink-auth`/`corelink-audit` trait surfaces don't
>    actually contain the methods the contract references (verify pre-impl)

Pre-impl verification performed by direct grep + Read against the workspace at
baseline `a1c49678`:

| Contract symbol | Codebase reality at `a1c49678` |
|---|---|
| `corelink_cas::CasStore` (trait) | **Does not exist.** No trait named `CasStore` anywhere in `crates/`. The aggregator crate `corelink-cas` (per `crates/corelink-cas/src/lib.rs`) is an Option-A re-export façade over `corelink-chunker`, `corelink-dedup`, `corelink-edge`, … and `corelink_worker::storage::r2`. The closest analogue is `corelink_worker::storage::blob_store::BlobStore` with `put_verified(&Self::Ctx, &VerifiedBody)` + `get(&Self::Ctx, &Digest)` — different method names, different signatures (`VerifiedBody` not raw `Bytes` on the write path; `Ctx` is an associated type not a `TenantId`), no `exists()` method at all. |
| `corelink_cas::CasStore::get/put/exists(tenant_id, Digest)` | Method names + signatures are fictional. The real write path is hash-verified (type-driven via `VerifiedBody`) — a sccache-bridge cannot legally call it with opaque `Bytes` without first wrapping through `corelink-hash`'s verify pipeline. |
| `corelink_auth::TenantResolver` (trait) | **Does not exist.** `grep -rn "pub trait TenantResolver"` over `crates/` returns zero hits. The PAT surface (`corelink-pat/src/verify.rs`) exposes free functions `verify_with_hash(...)` + `verify_hmac_only(...)`, not a `TenantResolver` trait. No equivalent "bearer-header → TenantId" abstraction exists at this baseline. |
| `corelink_audit::ports::AuditEmitter` (trait) | **Exists** at `crates/corelink-audit/src/ports.rs:172`, but the contract's pseudo-code implies async semantics — the real trait is `fn emit(&self, event: AuditEvent) -> Result<(), AuditEmitError>` (sync, for wasm32 compat per the same file's `## Choice of sync over async` doc-block). `InMemoryAuditEmitter` does exist at line 194 ✓. |
| `InMemoryCasStore` | **Does not exist.** The R2 path has `InMemoryR2` (a `R2Backend` fake, not a `CasStore` fake). |
| `InMemoryTenantResolver` | **Does not exist** (the trait it would implement doesn't exist either). |
| `InMemoryAuditEmitter` | **Exists** ✓ |

Two of three core upstream surfaces are fictional; one of three is real but
shape-mismatched against the contract pseudo-code. Two of three claimed test
fakes are fictional.

# §4. Why this is a HALT, not a "translate-and-proceed"

The contract is 160 LOC of pre-digested executable spec authored *as* the
dispatch packet per the user mandate "contratos sota, mastigado, sem
ambiguidade, agent só executa, não decide". Faced with three missing /
mis-shaped trait surfaces, an honest agent has exactly two paths:

(a) **Invent the missing traits.** Land `pub trait CasStore` in `corelink-cas`,
    `pub trait TenantResolver` in `corelink-auth`, plus `InMemoryCasStore` +
    `InMemoryTenantResolver` fakes. This is **architectural decision-making**
    crossing crate boundaries, explicitly excluded from the agent's remit by
    the mandate above ("não decide"). It also touches files outside the new
    crate, violating the parallel-safety assumption in Section 0.6 of the
    dispatch packet (sibling adapter agents would conflict on
    `corelink-cas/src/lib.rs`, `corelink-auth/src/lib.rs`).

(b) **Rewrite the contract to bind directly to the real surfaces** —
    `BlobStore` (with `VerifiedBody` write path + per-impl `Ctx`),
    free-function PAT verify, sync `AuditEmitter`. This is **substantive
    contract rewrite**, again outside the agent's remit and outside the
    parallel-safety contract with the 4 sibling adapter agents (a rewritten
    cargo contract would change the precedent the npm/pip/brew/oci contracts
    inherit).

Both paths cross the line from "execute" to "decide". Per Hard Pause Trigger
#4's explicit guidance — repeated verbatim in the agent contract:

> HALT + escalate. Stream A1 + 2.C precedents: partial-SEAL with documented
> deferral is acceptable.

This audit is that partial-SEAL with documented deferral.

# §5. What was NOT touched (parallel-safety preserved)

- `Cargo.toml [workspace.members]` — **untouched**; sibling adapter agents are
  free to add their own entries without UNION conflict with this branch.
- `Cargo.toml [workspace.dependencies]` — **untouched**.
- `Cargo.lock` — **untouched**.
- No new crate directory created; no `crates/corelink-adapter-cargo/` exists.
- No `src/`, no `Cargo.toml` for the new crate, no test files.
- No changes to `corelink-cas`, `corelink-auth`, `corelink-audit`, or any other
  workspace crate.

The only file written by this run is the present audit at
`specs/_audits/2026-05-26-w34-adapter-cargo.md`. Tree state remains a strict
superset of baseline `a1c49678` by one additive markdown file.

# §6. Gates run (informational; not a SEAL)

Not run — no implementation exists to gate. The build/clippy/wasm gates are
trivially "no-op-green" because the workspace is unchanged from `a1c49678`.

Spec-side gates (`validate_specs.py`, `validate_references.py`) should pass
because the only addition is a new audit file under an existing audit
directory.

# §7. Acceptance checklist (per contract §9)

| Item | Status |
|---|---|
| `cargo build -p corelink-adapter-cargo`: green | **N/A** — crate does not exist (deferred per §3) |
| `cargo test -p corelink-adapter-cargo`: ≥10 tests passing | **N/A** — deferred |
| `cargo clippy -p corelink-adapter-cargo --all-targets -- -D warnings`: clean | **N/A** — deferred |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`: green | **PRESERVED** — workspace unchanged |
| Every public type has `#[non_exhaustive]` | **N/A** — deferred |
| `#![forbid(unsafe_code)]` at crate root | **N/A** — deferred |
| Zero `unwrap()/expect()/panic!()` in `src/` | **N/A** — deferred |
| Audit emit before every state-mutation | **N/A** — deferred |
| `SecretString` + constant-time compare | **N/A** — deferred |
| L2.10: no `.rs` file >500 LOC | **N/A** — no `.rs` files written |
| SEAL audit at `specs/_audits/2026-05-2X-w34-adapter-cargo.md` per template | ✓ this file |
| Smoke test §8 row 1 reaches "cache hits: 3/3" | **N/A** — deferred |
| DCO + Co-Authored-By trailer on every commit | ✓ on the SEAL commit below |

# §8. Hard pause trigger ledger (this run)

| # | Trigger (per dispatch packet) | Status |
|---|---|---|
| 1 | Crate end-state exceeds ~1050 LOC by >50% | N/A — no crate created |
| 2 | Any file >500 LOC after split | N/A |
| 3 | Wasm32 workspace build breaks | NOT TRIGGERED — workspace unchanged |
| 4 | Trait surfaces in `corelink-cas`/`corelink-auth`/`corelink-audit` don't actually contain the methods the contract references | **TRIGGERED — see §3** |
| 5 | Test count <10 | N/A — no tests written |
| 6 | Sccache wire interpretation deviates from contract §2 reference | Not examined past §3 halt |

# §9. Proposed path forward (escalation packet)

Two orthogonal blockers, both resolvable by the orchestrator / spec author
*before* re-dispatching the cargo adapter (and, by parallelism precedent, the
4 sibling adapter contracts which inherit the same trait surface):

## §9.1 Resolve the CAS surface

Pick exactly one:

**Option A — invent `CasStore`.** Land a thin façade trait in `corelink-cas`:
```rust
#[non_exhaustive]
pub trait CasStore: Send + Sync {
    async fn get(&self, t: &TenantId, d: &Digest) -> Result<Bytes, CasError>;
    async fn put(&self, t: &TenantId, d: &Digest, body: Bytes) -> Result<(), CasError>;
    async fn exists(&self, t: &TenantId, d: &Digest) -> Result<bool, CasError>;
}
```
plus `InMemoryCasStore`. Adapter wraps the existing `BlobStore` underneath
(translating `Bytes` ↔ `VerifiedBody` via `corelink-hash`). Architectural cost:
the `put` taking opaque `Bytes` *bypasses* the type-driven verify seam the WI
specifically introduced — this needs explicit ADR.

**Option B — rewrite contract to bind to `BlobStore`.** The adapter takes an
`Arc<dyn BlobStore<Ctx = TenantCtx, Error = R2Error>>` and the sccache write
path goes `Bytes → VerifiedBody::compute(...) → BlobStore::put_verified`.
`HEAD` becomes a guess (no `exists()` exists at this surface; either add it or
implement as "get + drop body" or "get-meta"). This is the simpler change but
requires touching the contract.

Either is fine. The agent does not pick.

## §9.2 Resolve the auth surface

Pick exactly one:

**Option A — invent `TenantResolver`.** Land `pub trait TenantResolver` in
`corelink-auth` returning `Result<TenantId, AuthError>` from a `&Bearer`
header, plus an `InMemoryTenantResolver` fake. The adapter consumes the trait.

**Option B — rewrite contract to bind to free-function PAT verify.** Adapter
calls `corelink_pat::verify::verify_with_hash` directly + threads its own
`TenantId` resolution. Less abstract; more direct.

Either is fine.

## §9.3 Audit surface — minor only

Contract pseudo-code reads as if `AuditEmitter::emit` is async. The real trait
is sync (deliberate, for wasm32). Trivial contract fix: drop the `.await`.

## §9.4 In-memory fakes

Once §9.1 + §9.2 are decided, the corresponding `InMemoryCasStore` +
`InMemoryTenantResolver` fakes land in the same commit as the trait. The
adapter then has everything the contract §5 promised.

## §9.5 Sibling-adapter contract sync

The 4 sibling adapter contracts (npm, pip, brew, oci) presumably reference
the same fictional `CasStore` / `TenantResolver`. They should be patched in
lockstep so the 5 adapter agents share a coherent foundation.

# §10. Recommendation

The orchestrator should:

1. Resolve §9.1 + §9.2 (architectural decisions; not delegable to a Sonnet
   agent).
2. Patch all 5 adapter contracts in lockstep.
3. Re-dispatch wave 34 with the patched contracts.

This agent stands down on this attempt; no merge to main warranted. SEAL
status is **PARTIAL — documented deferral** per Stream A1 + 2.C precedent.

# §11. Commit + DCO

This audit is committed on `wt/r-prep-w34-adapter-cargo` with:

```
Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
```

The branch tip after this commit can be discarded once the orchestrator has
read the escalation packet; nothing else on the branch warrants preservation
beyond this audit (which is also pushable to main as a standalone audit
addition, since it touches no source).
