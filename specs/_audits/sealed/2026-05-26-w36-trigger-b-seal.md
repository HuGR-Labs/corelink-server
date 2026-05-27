---
id: "AUDIT-2026-05-26-W36-TRIGGER-B-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-36", "trigger-b", "wasm32", "platform-gate", "seal"]
references:
  - "specs/_audits/2026-05-26-w36-stage2c-closure.md"
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
---

# Wave 36 — Trigger B closure SEAL (wasm32 platform-gate)

## §1 Closes W36 Stage 2.C Trigger B

Per closure audit
`specs/_audits/2026-05-26-w36-stage2c-closure.md` §5.2 (Hard Pause
Trigger B — wasm32 mio/tokio transitive pull) Option 1 path:

1. **Platform-gate `corelink-ops`** in
   `crates/corelink-dsr-statuspage-scheduler/Cargo.toml` under
   `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`. The
   ops umbrella is therefore present ONLY on native targets, so the
   wasm32 build never resolves `tokio v1.52.3 → mio v1.2.0` (mio is
   unsupported on wasm32 — `"This wasm target is unsupported by mio.
   If using Tokio, disable the net feature."`).

2. **Complete the consumer migration** of
   `corelink_statuspage_real::*` → `corelink_ops::statuspage::*` that
   was halted by the Trigger B pause:
   - `src/scheduler.rs` (1 hot-path `use` + 1 inner-test `use` + 3
     test-fn references): canonical `corelink_ops::statuspage::*` on
     native, fallback direct `corelink_statuspage_real::*` on wasm32
     for the hot path (both resolve to the same symbols — the umbrella
     is a pure `pub use` re-export, see
     `crates/corelink-ops/src/statuspage.rs`).
   - `tests/dsr_statuspage_cron.rs` (1 `use`): unconditional canonical
     `corelink_ops::statuspage::*` because integration tests are
     native-only (no test runner under the wasm32 worker target).

Closes the §6 closure-spec "[statuspage-real] — 3 remaining lines in
`corelink-dsr-statuspage-scheduler` (2 files × import sites)" item.

## §2 Acceptance criteria

- [x] `corelink-ops` dep present ONLY under
      `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` in
      `crates/corelink-dsr-statuspage-scheduler/Cargo.toml`.
- [x] Hot-path `src/scheduler.rs` consumes `corelink_ops::statuspage::*`
      on native, `corelink_statuspage_real::*` on wasm32 (cfg-gated dual
      import with explanatory comment).
- [x] Inner test module + integration test use canonical
      `corelink_ops::statuspage::*` umbrella.
- [x] `cargo build -p corelink-dsr-statuspage-scheduler`: GREEN.
- [x] `cargo build -p corelink-dsr-statuspage-scheduler --target
      wasm32-unknown-unknown`: GREEN (was the failure mode under
      Trigger B baseline).
- [x] `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`:
      GREEN (the canonical upstream wasm32 verification command from
      closure spec §6 — `mio v1.2.0` no longer reachable on wasm32).
- [x] `cargo clippy -p corelink-dsr-statuspage-scheduler --tests --
      -D warnings`: GREEN (no `#[allow]` masks added).
- [x] `cargo test -p corelink-dsr-statuspage-scheduler`: 7/7 GREEN
      (6 unit + 1 proptest; doctest count unchanged at 0).
- [x] INV-AUDIT preserved — scheduler still emits canonical
      `scheduled` / `succeeded` / `failed` / `skipped` events through
      `SchedulerAuditSink`; no audit-chain logic touched.
- [x] INV-OPS-* invariants preserved — re-export semantics of
      `corelink_ops::statuspage` are unchanged (pure `pub use`).

## §3 Output evidence

| Gate | Command | Result |
| --- | --- | --- |
| Native build | `cargo build -p corelink-dsr-statuspage-scheduler` | `Finished dev profile in 2m 41s` |
| wasm32 build | `cargo build -p corelink-dsr-statuspage-scheduler --target wasm32-unknown-unknown` | `Finished dev profile in 57.48s` |
| Upstream wasm32 (canonical Trigger B verification) | `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | `Finished dev profile in 5.50s` (was: 48 errors from mio v1.2.0) |
| Clippy w/ tests | `cargo clippy -p corelink-dsr-statuspage-scheduler --tests -- -D warnings` | `Finished dev profile in 1m 20s` |
| Test run | `cargo test -p corelink-dsr-statuspage-scheduler` | 6 + 1 passed; 0 failed |

Files changed:

- `crates/corelink-dsr-statuspage-scheduler/Cargo.toml` — added
  `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` section
  with `corelink-ops = { workspace = true }` + explanatory comment
  block referencing this audit + the closure spec.
- `crates/corelink-dsr-statuspage-scheduler/src/scheduler.rs` —
  cfg-gated dual import for the hot path; inner test module + 3
  test-fn refs migrated to `corelink_ops::statuspage::*`.
- `crates/corelink-dsr-statuspage-scheduler/tests/dsr_statuspage_cron.rs`
  — single `use` migrated to `corelink_ops::statuspage::*` with
  rationale comment.

## §4 Downstream unlock

- W36 Stage 2.C closure spec §8 #3 (cargo-deny lockdown) — Trigger B
  blocker for `deny-direct` enforcement on `corelink-statuspage-real`
  is now resolved. Stage 3 cargo-deny lockdown can proceed for the
  statuspage canonical-LOC-owner crate.
- Wave 33+34 closure-followups §4 #2 (Stage 2.E Phase 2 — 72 absorbed
  crates removal) — statuspage sub-path unblocked; materializer
  (Trigger A) is still the remaining blocker for full closure.

## §5 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
