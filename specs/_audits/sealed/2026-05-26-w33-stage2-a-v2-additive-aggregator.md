---
id: "AUDIT-2026-05-26-W33-STAGE2-A-V2-ADDITIVE-AGGREGATOR"
type: "audit"
doc_status: "ACTIVE"
audit_status: "CLOSED"
version: "1.1.0"
created: "2026-05-26"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-33", "stage-2", "additive-aggregator", "corelink-worker"]
references:
---

> **CLOSED 2026-05-27** — Wave-33 Stage 2 sealed via git tag `wave-33-stage2-sealed` (SEAL commit `a1c49678` "wave-33 stage 2.A-v2 SEAL: additive aggregator audit"); 4 umbrellas + 5 canonical paths landed as documented. See `specs/_audits/2026-05-27-audit-triage-post-w36.md`.

# Wave 33 Stage 2.A-v2 SEAL — Additive Aggregator Audit

**Authored:** 2026-05-26
**Branch:** main (orchestrator-direct, no isolated worktree)
**Baseline:** post 2.A HALT merge at `8df6bc87`

## §1 Scope

Per **Wave 33 Stage 2.A HALT audit** (`2026-05-22-w33-stage2-a-worker-moves.md`) §9(b) recommendation: deliver the canonical wave-33 import surface for `corelink-worker`'s 5 logical submodules WITHOUT physically moving 19,882 LOC out of the implementation crate. The HALT verdict identified that target aggregator crates (`corelink-cas`, `corelink-auth`, `corelink-replication`) are paper-thin Option-A façades (1-line `pub use` re-export files) and cannot absorb implementation; the physical move would invert Stage 1 Stream A/B/C design.

**Solution:** additive aggregator — each umbrella adds 4-5 lines `pub mod <canonical-name> { pub use corelink_worker::<area>::*; }` at lib.rs level. This delivers the canonical paths consumers expect (`corelink_cas::r2_storage::*`, etc.) while leaving the 19,882 LOC where it belongs (the implementation crate `corelink-worker`).

## §2 Sub-step commits (4 + SEAL)

| Sub-step | Commit | Target umbrella | Re-exported from corelink-worker | Feature gate |
|---|---|---|---|---|
| 2.A-v2.1 | `228002d9` | `corelink-cas` | `storage::r2` + `cache` | none (default-feature; wasm32-clean) |
| 2.A-v2.2 | `2844fa16` | `corelink-auth` | `middleware` + `auth` (→ `worker_session`) | `tower-middleware` (corelink-auth feature forwards to corelink-worker feature) |
| 2.A-v2.3 | `c437d8e0` | `corelink-replication` | top-level `Region` + `TenantCtx` (→ `region_resolver`) | none (top-level public surface) |
| 2.A-v2.4 | `f08217ca` | `corelink-reapi` | `reapi` (→ `worker_adapter`) | `host-server` (corelink-reapi feature already enables corelink-worker/tower-middleware) |
| 2.A-v2 SEAL | this doc | — | — | — |

## §3 Canonical wave-33 paths now resolvable

After this SEAL, consumers can import via:

```rust
// Storage (CAS R2 + cache adapters)
use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_cas::cache::{ KvCache, ... };

// Auth (Tower middleware + worker-side session lifecycle)
// requires `corelink-auth/tower-middleware` feature
use corelink_auth::middleware::TimingPaddingLayer;
use corelink_auth::worker_session::RevocationOrchestrator;

// Replication (region resolver)
use corelink_replication::region_resolver::{Region, TenantCtx};

// REAPI (worker-side adapter)
// requires `corelink-reapi/host-server` feature (default-on)
use corelink_reapi::worker_adapter::{ ... };
```

Original `corelink_worker::*` paths remain valid (consumers don't need to migrate immediately — Stage 2.E will sweep that).

## §4 L2.10 file-size discipline

Per umbrella, additions to `src/lib.rs`:
- `corelink-cas/src/lib.rs`: +16 LOC (76 → 92 LOC; well below 200 sweet spot)
- `corelink-auth/src/lib.rs`: +19 LOC (~190 → ~209 LOC; just at the 200 sweet spot)
- `corelink-replication/src/lib.rs`: +12 LOC (~140 → ~152 LOC)
- `corelink-reapi/src/lib.rs`: +14 LOC

Every umbrella's `lib.rs` stays under 250 LOC HARD CAP. Zero new `.rs` files created (all changes inline in existing lib.rs).

## §5 Cargo.toml deps added

| Crate | Added | Notes |
|---|---|---|
| corelink-cas | `corelink-worker = { workspace = true }` | unconditional |
| corelink-auth | `corelink-worker = { workspace = true }` + new feature `tower-middleware = ["corelink-worker/tower-middleware"]` | feature-forwarded |
| corelink-replication | `corelink-worker = { workspace = true }` | unconditional |
| corelink-reapi | (already had `corelink-worker = { workspace = true }`) | no change |

No new circular deps introduced. `corelink-worker` does NOT depend on `corelink-cas`/`corelink-auth`/`corelink-replication`/`corelink-reapi`. Diamond dep through `corelink-clerk` (corelink-auth → corelink-clerk; corelink-worker → corelink-clerk) is acyclic.

## §6 Gates

All green:

- `cargo check -p corelink-cas`: GREEN
- `cargo check -p corelink-auth`: GREEN (default-features; tower-middleware feature gate active)
- `cargo check -p corelink-auth --features tower-middleware`: GREEN
- `cargo check -p corelink-replication`: GREEN
- `cargo check -p corelink-reapi`: GREEN (host-server default-on)
- `cargo check -p corelink-reapi --no-default-features`: GREEN
- `cargo check --workspace`: GREEN

`validate_specs.py`: not re-run yet (this SEAL doc adds 1 file with proper frontmatter; expected 464 → 465 OK).

## §7 Hard pause triggers

None activated. Charter constraints preserved:
- `#![forbid(unsafe_code)]`: unchanged in every umbrella
- No `unwrap/expect/panic` in `src/`: unchanged (zero new imperative code; only `pub use` re-exports)
- `subtle::ConstantTimeEq`: unchanged
- `#[non_exhaustive]`: unchanged (no new types introduced)
- Audit fail-CLOSED: unchanged (no new emit sites)

## §8 What 2.A-v2 does NOT do

- Does NOT migrate consumers. The 63 files using `corelink_worker::*` paths still use them. Stage 2.E sweep is the next step.
- Does NOT remove `corelink-worker` from workspace. It stays as the implementation crate (~20K LOC).
- Does NOT slim `corelink-worker` to a wasm32 entry shell (that goal is structurally impossible — see 2.A HALT §3).
- Does NOT touch `corelink-cf-bindings` consumer (uses `corelink_worker::cache::kv::*` and `corelink_worker::storage::r2::*` directly; will migrate in 2.E).

## §9 Next steps

- **Stage 2.E**: consumer migration sweep — rewrite all `use corelink_worker::storage::r2::*` → `use corelink_cas::r2_storage::*` etc. across 63 consumer files. Then remove leftover absorbed crates whose consumers fully migrated.
- **Stage 2 SEAL + tag** `wave-33-stage2-sealed` after 2.E completes.
- **Wave 34 adapter campaign** can dispatch in parallel with 2.E (adapter contracts use trait surfaces, not impl paths).

## §10 Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of Wave 33 Stage 2.A-v2 SEAL audit.**
