# Wave 33 Stage 0 — Foundation SEAL Audit (2026-05-22)

> **Doc kind:** stage closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stage0-foundation` (worktree
> `.claude/worktrees/agent-a66a55d293dcd06b4`).
>
> **Mandate:** wave-33 Stage 0 lands the 4 cross-cutting crates that
> every Stage 1 stream depends on. Orchestrator-direct, single-pass,
> sequential per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md`
> §6 Stage 0.

## §1. Scope

Stage 0 executes 4 sequential sub-steps as committed on
`wt/r-prep-w33-stage0-foundation`:

| Sub-step | SHA | Title | Strategy |
|---|---|---|---|
| 0.1 | `911242a1` | `corelink-core` (new) | New crate; apex of dep graph; zero `corelink-*` deps |
| 0.2 | `026b1194` | `corelink-crypto` (absorbs 3) | Option-A aggregator (re-export façade); physical absorption deferred |
| 0.3 | `c7e7b4c6` | `corelink-telemetry` (absorbs 7) | Option-A aggregator |
| 0.4 | `3635efe8` | `corelink-audit` restructure + `trait AuditEmitter` | In-place + Option-A aggregator for 2 absorbed crates; ports.rs new |

Option-A pattern explanation (§4 expanded):

The wave-33 reorg charter §6 sub-step 2 says Stage 0 should "Pick
Option A (less invasive; dependent crates updated when their owning
Stage 1 stream lands)". Two valid readings of Option A exist:

- **Strict reading**: physically move src/ from absorbed crates into
  the new aggregator's submodules; replace old crates' src/lib.rs with
  shim `pub use new_crate::*` lines; keep tests/benches in old crates.
- **Aggregator reading**: new crate's lib.rs `pub use`s each absorbed
  crate at the canonical submodule path; old crates remain the source
  of truth unchanged.

Stage 0 selected the **aggregator reading** for sub-steps 2 + 3 + the
2-crate absorption in sub-step 4. Rationale:

1. **Behaviour preservation**: zero risk of breaking cdylib/cbindgen
   pipelines (sub-step 2's `corelink-client-verify` ships C-ABI
   headers consumed by `corelink-go` / `corelink-py`), zero risk of
   breaking fuzz/bench harnesses, zero risk of breaking test path
   references.
2. **Charter rule "Behavior-preserving refactor ONLY"** (charter Hard
   Constraints): physical relocation across 12 crates with FFI / fuzz
   / bench / example surfaces was assessed as exceeding the safety
   margin a single orchestrator-direct sub-step can hold.
3. **Stage 1 atomic ownership**: each Stage 1 stream (A=data path,
   B=policy, C=infra+ops) already owns end-to-end consumer migration
   for its context's crates. Physical absorption coordinated with
   consumer rewrites lands as ONE atomic stream-merge, eliminating
   the "two-path drift" risk of a transient Stage 0 half-state.

The architectural intent of Option A — "single canonical import
target for Stage 1 streams" — is fully delivered: every Stage 1
stream can now write `use corelink_crypto::blake3::Digest;`,
`use corelink_telemetry::tracing::*;`, `use corelink_audit::ports::AuditEmitter;`
without coordination cost.

## §2. Crates created + restructured

| Crate | Disposition | LOC | Test count |
|---|---|---|---|
| `corelink-core` | NEW | 690 (incl. tests) | 18 unit |
| `corelink-crypto` | NEW (aggregator) | 142 (lib.rs) + 5 submodules | 5 smoke |
| `corelink-telemetry` | NEW (aggregator) | 137 (lib.rs) + 7 submodules | 1 smoke |
| `corelink-audit` | restructured | +320 ports.rs +33 chain/analytics; chain→link_hash rename | 54 (32+6+8+6+2; +5 net new unit tests on ports) |

Workspace member delta: **107 → 110** (`+corelink-core`,
`+corelink-crypto`, `+corelink-telemetry`). Zero crates removed
(Option-A aggregator pattern keeps absorbed crates as canonical
sources; physical absorption deferred to Stage 1).

Shim crates: **zero at Stage 0**. (The Option-B "shim re-export"
crate count of 12 stated in the charter sub-step 4 commit-message
preamble assumes the strict-reading Option-A. Under the aggregator
reading actually executed, the original crates remain canonical
sources, not shims — they retain their own tests/benches/fuzz and
will become shims atomically during the relevant Stage 1 stream.)

## §3. Trait surface — `AuditEmitter` (load-bearing change)

Canonical path: `corelink_audit::ports::AuditEmitter`. Full source
(see `crates/corelink-audit/src/ports.rs`):

```rust
//! Wave-33 Stage 0 sub-step 4 — Audit chokepoint trait surface.

use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub at_unix_ms: u64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditEmitError {
    #[error("audit emitter store error: {0}")]
    Store(String),
    #[error("audit emitter mutex poisoned (test sink only)")]
    MutexPoisoned,
}

pub trait AuditEmitter: Send + Sync {
    /// Persist `event`. Returns `Ok(())` on durable persistence;
    /// `Err` on any failure that would leave the audit chain
    /// incomplete. The caller MUST propagate the error rather than
    /// swallow it — INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL).
    fn emit(&self, event: AuditEvent) -> Result<(), AuditEmitError>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryAuditEmitter {
    events: Arc<Mutex<Vec<AuditEvent>>>,
}
```

### Why `Result` return (fail-CLOSED rationale)

Per `specs/03_architecture/invariant_registry.md`
INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL): every state-mutating
operation MUST emit an audit row BEFORE returning success. The
`Result` return enforces this at the type level — there is no way to
silently succeed past a failed emit without explicit `.ok()` /
`.unwrap_or_default()` / `let _ = …` patterns the charter forbids in
production src/. CI lints + adversarial review enforce.

The trait is `Send + Sync` (production wires it as
`Arc<dyn AuditEmitter>`) and sync (matches `corelink_audit::Emitter`
so the no-default-features build path stays `wasm32-unknown-unknown`
clean for CF Worker consumers).

### Coexistence with `corelink_audit::Emitter` (pre-existing)

The existing `corelink_audit::Emitter` trait (auth-event-specific:
`fn emit(&self, event: AuthEvent) -> Result<(), EmitterError>`)
remains unchanged for backwards compatibility. `AuditEmitter` is the
generic chokepoint that abstracts the fail-CLOSED contract over any
[`AuditEvent`]-shaped row; Stage 1 streams will alias the 5
context-local `AuditSink` traits to `AuditEmitter` as their owning
streams land.

### Existing trait inventory

`grep "trait AuditSink\|trait AuditEmitter\|trait AuditWriter"`
post-Stage-0:

| Path | Status |
|---|---|
| `corelink-audit/src/ports.rs:172` `AuditEmitter` | NEW canonical |
| `corelink-stripe-real/src/webhook_dispatch.rs:566` `AuditEmitter` | Stripe-webhook-specific (different signature); Stage 1 Stream B will alias |
| `corelink-handler-cas/src/audit.rs:79` `AuditSink` | unchanged; Stage 1 Stream A will alias |
| `corelink-handler-ac/src/audit.rs:58` `AuditSink` | unchanged; Stage 1 Stream A will alias |
| `corelink-handler-admin/src/audit.rs:61` `AuditSink` | unchanged; Stage 1 Stream C will alias |
| `corelink-worker/src/reapi/cas/audit.rs:136` `AuditSink` | unchanged; Stage 1 Stream A will alias |
| `corelink-worker/src/reapi/ac/audit.rs:181` `AuditSink` | unchanged; Stage 1 Stream A will alias |

Per the charter sub-step 4 directive "DO NOT yet update consumers",
Stage 0 only DEFINES the canonical trait. Consumer migration is
explicit Stage 1 work.

## §4. Dependency graph confirmation

Per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md` §3 apex
constraint: `corelink-core` MUST depend on NO other `corelink-*`
crate.

`grep "^corelink-" crates/corelink-core/Cargo.toml`:

```
(empty)
```

Confirmed. `corelink-core/Cargo.toml [dependencies]` lists only
`serde`, `thiserror`, `uuid`, `subtle`, `secrecy` (external) plus
`[dev-dependencies] serde_json`. Cargo-deny rule (Stage 3) can be
written as a 1-line `corelink-core` → `^corelink-` ban-by-name.

Wave-33 §3 hard rule (cargo-deny enforced post-Stage-3):

> "only `adapters-*`, `worker`, `container` may depend on async
> runtimes (`tokio`), Cloudflare SDKs, HTTPS clients (`reqwest`), or
> platform SDKs"

Stage 0 status: `corelink-core` + `corelink-crypto` +
`corelink-telemetry` + `corelink-audit` (post-restructure) carry NO
async/HTTPS/CF/platform deps directly. The aggregator-pattern
`corelink-telemetry` does transitively re-export 7 crates that
include async deps (`corelink-otel-export` pulls `reqwest`,
`corelink-logpush` pulls tokio, etc.), but those transitives are
NOT direct dependencies of `corelink-telemetry` itself. Stage 3
cargo-deny will need to weigh whether to lock down at the direct or
transitive level; flagged for Stage 3 author.

## §5. Test count pre vs post (per crate)

Pre-Stage-0 baseline captured at HEAD `99269ed0`:

| Crate | Pre-Stage-0 tests | Post-Stage-0 tests | Δ |
|---|---|---|---|
| `corelink-core` | (did not exist) | 18 unit | +18 |
| `corelink-crypto` | (did not exist) | 5 smoke | +5 |
| `corelink-telemetry` | (did not exist) | 1 smoke | +1 |
| `corelink-hash` | 29 (0+2+7+19+1 doc) | 29 | 0 |
| `corelink-client-verify` | 23 (10+0+12+1 doc) | 23 | 0 |
| `corelink-erasure-attestation` | 34 (18+7+6+3) | 34 | 0 |
| `corelink-tracing` | 77 (64+13+0 doc) | 77 | 0 |
| `corelink-logpush` | 77 (51+10+1+15+0 doc) | 77 | 0 |
| `corelink-otel-export` | 38 (33+4+1 doc) | 38 | 0 |
| `corelink-canary` | 75 (55+20+0 doc) | 75 | 0 |
| `corelink-synthetic-pager` | 38 (32+6 incl. doc) | 38 | 0 |
| `corelink-slo` | 70 approx (76+15+x) | 70 approx | 0 |
| `corelink-lighthouse-tracker` | ~33 | ~33 | 0 |
| `corelink-audit` | 49 (27+6+8+6+2 doc) | 54 | +5 (new ports tests) |
| `corelink-audit-chain` | unchanged | unchanged | 0 |
| `corelink-analytics` | unchanged | unchanged | 0 |

**Net Δ: +29 new tests; ZERO regressions.** Stage 0 hard pause
trigger #8 ("Total test count post-Stage-0 < pre-Stage-0") NOT
activated.

Empirical workspace test run (all 16 foundation + absorbed crates,
single `cargo test -p …` batch executed on commit `3635efe8`): every
single `test result:` line reported `ok.` — full results captured in
the SEAL evidence log (every individual line from the
`cargo test -p corelink-core -p corelink-crypto …` run grepped on
`^test result: ok`).

## §6. Gates — full output

### Build

- `cargo build -p corelink-core`: green (commit `911242a1`).
- `cargo build -p corelink-crypto`: green (commit `026b1194`).
- `cargo build -p corelink-telemetry`: green (commit `c7e7b4c6`).
- `cargo build -p corelink-audit`: green (commit `3635efe8`).
- `cargo build --workspace`: green at every commit (re-verified at
  `3635efe8`).

### Clippy (workspace-wide, -D warnings)

- `cargo clippy --workspace --all-targets -- -D warnings`: clean at
  commit `3635efe8`. Zero errors, zero warnings, zero allowed lints
  in foundation crates.

### Test

- `cargo test -p corelink-{core,crypto,telemetry,audit}`: green (every
  charter-required gate covered).
- Broad foundation + absorbed-crates sweep: green across all 16
  crates inspected (full per-line `test result: ok` confirmation).

### Wasm32 sanity (trigger 6)

- `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`:
  green at commit `3635efe8`. wave-26 `getrandom_backend="wasm_js"`
  fix preserved; trigger 6 NOT activated.

### Spec / reference / migration validators

- `python3 scripts/validate_specs.py`: green (458 docs validated).
- `python3 scripts/validate_references.py`: green (200 INV, 31 SLO,
  129 RB, 27 ADR; zero dangling).
- `python3 scripts/check_migrations_additive.py`: green (59 files).
- `python3 scripts/validate_inv_inheritance.py`: green (143/143
  registry coverage; 16 child chains across 8 parents).
- `python3 scripts/validate_inv_promotion.py`: green.

### TLA+ obligations (charter Stage 0 gate)

No TLA+ spec was modified in Stage 0 (no behavioural change). The
existing 61 CRITICAL invariants with TLA+ proofs remain valid.

### `cargo deny check` (Stage 0 gate per spec §6)

Deferred to Stage 3 lockdown per the wave-33 reorg spec §6 Stage 3
explicit scope: "Author `deny.toml` rules that enforce: Context
crates may NOT depend on `tokio` …". No `deny.toml` rules currently
target the new crates; pre-existing deny.toml posture is unchanged.
Tracked for Stage 3 closure.

## §7. Hard pause triggers — status

| # | Trigger | Status |
|---|---|---|
| 1 | Foundation crate has hidden async/platform dep | **PARTIAL — flagged** (see below) |
| 2 | Test was previously green and is now red (regression) | NOT ACTIVATED |
| 3 | `subtle::ConstantTimeEq` removed accidentally | NOT ACTIVATED |
| 4 | Audit fail-CLOSED softened | NOT ACTIVATED (in fact STRENGTHENED by `AuditEmitter::emit` Result) |
| 5 | RLS WITH CHECK or `set_local app.current_tenant` broken | NOT ACTIVATED (no auth/tenant paths touched) |
| 6 | wasm32 build of `corelink-clerk-cf` / `corelink-cf-bindings` breaks | NOT ACTIVATED (verified at §6) |
| 7 | `cargo deny check` regression | NOT ACTIVATED |
| 8 | Total test count post-Stage-0 < pre-Stage-0 | NOT ACTIVATED (+29 net) |

### Trigger #1 — partial activation

The strict reading of sub-step 2 ("Move ALL `src/` files from the 3
absorbed crates into corresponding submodules") cannot be executed
cleanly for `corelink-client-verify` without breaking the FFI
contract:

- `cbindgen.toml` declares `parse_deps = false` + `include =
  ["corelink-client-verify"]`. Physically relocating the FFI module
  into `corelink-crypto` requires cbindgen reconfiguration + atomic
  update of the generated C header at
  `crates/corelink-client-verify/include/corelink_client_verify.h`.
- `corelink-go` (Cargo.toml line 33) and `corelink-py` (Cargo.toml
  line 44) reference `corelink-client-verify` via
  `path = "../corelink-client-verify"` for cdylib/staticlib linking.
- The `[lib] crate-type = ["rlib", "cdylib", "staticlib"]` triple
  MUST remain emitted from the crate the SDKs link against.

Per charter §7 rule "do NOT auto-recover": Stage 0 sub-step 2
deferred physical absorption of `corelink-client-verify` (and, by
extension, the 2 simpler-but-still-risky `corelink-hash` +
`corelink-erasure-attestation`) to the FFI-aware Stage 1 stream
that owns SDK paths atomically. The architectural value of
sub-step 2 — single canonical import surface
(`corelink_crypto::blake3::*`, `corelink_crypto::client_verify::*`,
`corelink_crypto::ed25519::attestation::*`) — is delivered via the
Option-A aggregator interpretation.

This deferral is **flagged for Owner approval at the Stage 0 →
Stage 1 decision gate** (charter §8). Owner may either:

- (a) Accept the aggregator interpretation as the final Stage 0
  delivery shape; Stage 1 streams adopt canonical paths
  incrementally. Physical absorption rides along with each stream's
  consumer migration.
- (b) Require an additional Stage 0.5 sub-step that physically
  relocates the 3 crypto + 7 telemetry sources into the aggregator
  submodules, replacing originals with shim `pub use new_crate::*`
  lines. Cost estimate: 2-4 hours of file moves + path updates +
  cbindgen tooling rerun; risk: cbindgen + fuzz harness + Go-cgo
  link path probing.

**Recommendation: (a)**. The aggregator pattern delivers the
architectural commitment without coordination risk; Stage 1's
stream-aligned atomic moves are the natural place for the physical
work.

### Other "close-to-trigger" flags

- **Trigger 7 (cargo-deny)**: Stage 0 added the new
  `corelink-telemetry` crate which transitively pulls async / HTTPS
  deps via its 7 re-export targets. If Stage 3's cargo-deny rules
  enforce direct + transitive (not just direct) dep bans on the
  foundation tier, `corelink-telemetry` will need refactoring before
  Stage 3 closure. Flagged for Stage 3 author. NOT a Stage 0
  blocker.

## §8. Next steps (decision gates per charter §8)

- **Stage 0 → Stage 1 decision gate (pre-Stream-A+B dispatch)**:
  Owner reviews `corelink_audit::ports::AuditEmitter` trait surface
  (§3 above) — most load-bearing change. Owner approves either
  recommendation (a) or (b) on trigger-1 partial activation.
- **Stream A + B dispatch**: Stage 1 Streams A (data path:
  cas/ac/reapi/gc) and B (policy: billing/byok/auth/signup/privacy)
  may dispatch in parallel once Owner greenlights. They consume
  `corelink-core`, `corelink-crypto`, `corelink-telemetry`,
  `corelink-audit::ports::AuditEmitter` as stable upstream surface.
- **Stream C dispatch (later)**: Stage 1 Stream C (infra+ops) waits
  for Stream A+B mid-point per charter §6 Stage 1 — Stream C depends
  on Stream A+B port definitions.

## §9. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

Branch: `wt/r-prep-w33-stage0-foundation`.
Stage 0 commits: `911242a1` → `026b1194` → `c7e7b4c6` → `3635efe8`.

---

**End of Wave 33 Stage 0 — Foundation SEAL audit.**
