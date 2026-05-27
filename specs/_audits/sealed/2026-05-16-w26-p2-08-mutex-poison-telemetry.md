# W26-P2-08 Closure Audit — `emit_synthetic` Mutex-Poison Telemetry on CF Worker Prefetch Fail-CLOSED Path

> **Doc kind:** P2-DEFER-POST-GA closure audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-30+ P2-defer closure agent (Claude Opus 4.7) — branch `wt/r-prep-w26-p2-08-mutex-poison-telemetry`.
> **Base:** `main` @ `0f77f48` (post wave-30 stream merges).
> **Cross-ref:** `specs/_audits/2026-05-16-wave26-adversarial-review.md §228` + §429 (origin of DEFER-POST-GA classification), `specs/03_architecture/invariant_registry.md` (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER row), `crates/corelink-clerk-cf/src/audit_sink.rs` (silent-drop site pre-fix), `crates/corelink-clerk-cf/src/prod_wiring.rs` (single call site of `emit_synthetic` on the fail-CLOSED path).

---

## 1. Scope + provenance

### 1.1 Origin of the defect

Wave-26 wired `prod_wiring::prefetch_request_prelude` as the canonical CF Worker request-prelude. On resolver failure (tenant_config row missing / D1 backend error / async-prefetch failure) the wire returns `PrefetchWireError`; the `health::main` fetch handler maps that to `worker::Response::error(format!("tenant_region_unresolved: {e}"), 503)` (`health.rs:400`). Before returning the error, the wire emits a synthetic audit row via `AuditSink::emit_synthetic` against a `tenant_region_unresolved` `AuditEvent`.

`AuditSink::emit_synthetic` is *infallible by signature* — it returns `()`. The recorder backend's mutex-poison case at `audit_sink.rs:232-236` (pre-fix) was implemented as:

```rust
SinkBackend::Recorder(buf) => {
    if let Ok(mut guard) = buf.lock() {
        guard.push(event);
    }
    // Err(PoisonError) silently dropped here.
}
```

Wave-26 adversarial review §228 flagged the silent drop as a defensible-but-suboptimal INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER concern: the customer-visible 503 still fires, but the operator loses observability into whether the audit-emit succeeded. The fix was classed DEFER-POST-GA in §429 because converting the silent drop into a recorded event must NOT reintroduce the double-fault risk the silent drop was originally protecting against (an audit-emit inside the audit-emit-failure handler that itself faults).

### 1.2 W26-P2-08 mandate

Convert the silent mutex-poison drop into a recorded telemetry event with **zero** risk of double-fault. Definition of done: (a) 503 fail-CLOSED preserved, (b) atomic counter increments per poison event, (c) structured `tracing` line emitted, (d) recording mechanism does not re-acquire the poisoned mutex nor call back into the audit chain, (e) integration test that simulates a real poisoned mutex (not a mock) and pins the post-fix behaviour.

---

## 2. Silent-drop sites (pre-fix)

Grep sweep across `crates/corelink-clerk-cf/` for `.ok()`, `.unwrap_or`, `unwrap_or_default()`, `PoisonError`, `is_poisoned`, `lock().map_err`:

| File | Line(s) pre-fix | Disposition |
|---|---|---|
| `crates/corelink-clerk-cf/src/audit_sink.rs` | `232-236` (`SinkBackend::Recorder` arm of `emit_synthetic`) | **Silent drop** — the `if let Ok(mut guard) = buf.lock()` pattern silently discards `Err(PoisonError)`. **This is the W26-P2-08 fix site.** |
| `crates/corelink-clerk-cf/src/tenant_region_real.rs` | `147`, `163`, `254` | `.lock().map_err(\|_\| "…")?` — Err is **propagated**, not silently dropped. No fix needed. |
| `crates/corelink-clerk-cf/src/prod_wiring.rs` | `649` | `audit_sink.emit_synthetic(event);` — the single fail-CLOSED call site. Fix is upstream in `audit_sink.rs`, leaving this call site intact. |
| `crates/corelink-clerk-cf/src/health.rs` | `175-178`, `407` | `.ok()` on a JS-binding fetch + `.unwrap_or_else(...)` on a timestamp fallback. These are NOT mutex-poison paths and NOT on the fail-CLOSED audit-emit chain. Out of scope. |

**Conclusion:** exactly one silent-drop site on the fail-CLOSED audit-emit chain, at `audit_sink.rs:232-236`. The fix is a surgical edit to that arm.

---

## 3. Double-fault risk analysis

### 3.1 Why a naive "just emit again" approach is wrong

A natural-but-wrong fix would be:

```rust
SinkBackend::Recorder(buf) => {
    if let Err(_) = buf.lock().map(|mut g| g.push(event.clone())) {
        // RE-EMIT through the same chain. WRONG.
        self.emit_synthetic(AuditEvent { op: "audit_emit_failed", ..event });
    }
}
```

This re-enters the same `SinkBackend::Recorder` arm, hits the same poisoned mutex, and loops (or stack-overflows). The original silent-drop chose correctness-over-observability to avoid exactly this.

Other failing approaches:
- **Buffer the event for later replay** — requires another `Arc<Mutex<...>>`, which can also poison; just relocates the problem.
- **`PoisonError::into_inner()` and push anyway** — corrupts the canonical "poisoned = data-race-suspect" semantics; the recovered vec may have torn writes from the panicking thread. The downstream test assertions could see stale state.
- **`panic!`** — explicitly forbidden by `corelink-clerk-cf` crate-level lint `panic = "deny"`; also drops the 503 response (the panic propagates out of the fetch handler).

### 3.2 W26-P2-08 chosen mechanism

A **side-channel** recording that:

1. **Does not re-acquire the poisoned mutex.** A `static AtomicU64` named `AUDIT_MUTEX_POISON_TOTAL_PREFETCH_FAIL_CLOSED` lives in module scope. `fetch_add(1, Ordering::Relaxed)` is lock-free, allocation-free, and cannot itself fault.
2. **Does not call back into the audit chain.** A `tracing::error!` macro with structured fields (`event`, `surface`, `op`) emits to the global tracing subscriber. On the production `wasm32` target the macro is effectively a no-op (no subscriber is installed in CF Workers; the underlying global-subscriber lookup is lock-free). On native test targets the tracing subscriber writes to a separate test recorder.
3. **Is exposed via a public read accessor** (`audit_mutex_poison_total_prefetch_fail_closed() -> u64`) so the operator's metrics exporter can publish it as `corelink_audit_mutex_poison_total{path="prefetch_fail_closed"}` and alert on non-zero.

### 3.3 Why this cannot double-fault

- `AtomicU64::fetch_add` is `Ordering::Relaxed` lock-free atomic primitive: no mutex, no allocation, no panic path on the CPU primitive itself.
- `tracing::error!` with literal `&'static str` fields + already-allocated `event.surface` / `event.op` (also `&'static str` from the `AuditEvent` struct definition) does not allocate on the structured-field path.
- The `event` argument is consumed (not cloned) on the poisoned path — no extra allocation.
- The poisoned `buf` is dropped at function exit per normal Rust lifetime; we never call `into_inner()` or otherwise mutate the poisoned state.

The 503 fail-CLOSED upstream is **untouched**: `emit_synthetic` returns `()` unconditionally, so `emit_tenant_region_unresolved` returns, `prefetch_request_prelude` continues to its `return Err(PrefetchWireError::...)`, and `health::main` maps to the 503. Availability semantics preserved.

---

## 4. Implementation diff summary

### 4.1 `crates/corelink-clerk-cf/src/audit_sink.rs`

- **+ added:** `use std::sync::atomic::{AtomicU64, Ordering};`
- **+ added:** `static AUDIT_MUTEX_POISON_TOTAL_PREFETCH_FAIL_CLOSED: AtomicU64 = AtomicU64::new(0);` with a 26-line doc-comment explaining double-fault safety + W26-P2-08 provenance.
- **+ added:** `pub fn audit_mutex_poison_total_prefetch_fail_closed() -> u64` accessor (operator metrics export).
- **Δ modified:** `pub fn emit_synthetic(&self, event: AuditEvent)` — the `SinkBackend::Recorder(buf)` arm rewritten:
  - `if let Ok(mut guard) = buf.lock() { ... }` → `match buf.lock() { Ok(...) => ..., Err(_poisoned) => { fetch_add + tracing::error! } }`
  - Doc-comment expanded to describe the W26-P2-08 telemetry semantics + double-fault-safety argument.

Diff scope: ~70 lines added, 5 lines modified (the `Recorder` arm body); zero lines deleted from the production logic. The production `wasm32` `ConsoleNdjson` arm is unchanged (it is infallible — `console_log!` cannot poison anything).

### 4.2 No changes to

- `src/prod_wiring.rs` — the call site at line 649 is unchanged; the fix is fully upstream.
- `src/health.rs` — the 503 response path is unchanged.
- `src/tenant_region_real.rs` — already propagated poison errors correctly.
- `crates/corelink-audit-chain/` — no changes; the synthetic-emit surface lives entirely in `corelink-clerk-cf`.

---

## 5. Test design — `tests/audit_sink_mutex_poison_telemetry.rs`

### 5.1 Setup: poison a real mutex

The canonical Rust idiom: spawn a thread, acquire the lock, panic.

```rust
let buf_clone = Arc::clone(&buf);
let handle = std::thread::spawn(move || {
    let _guard = buf_clone.lock().expect("poisoner lock");
    panic!("intentional panic-while-holding-lock");
});
let _ = handle.join(); // returns Err — thread panicked.
// buf.lock() now returns Err(PoisonError) forever.
```

A `probe = buf.lock(); assert!(probe.is_err())` step pins that the poisoning actually took effect before exercising the fix (prevents false-green from a broken setup).

### 5.2 Three pinning tests

1. **`emit_synthetic_healthy_recorder_captures_event_without_poison_path`** — control. Calls `emit_synthetic` against a non-poisoned recorder; asserts the event lands in the buffer and `op` / `surface` match. This pins that the poison branch is NOT entered on the happy path.

2. **`emit_synthetic_poisoned_recorder_increments_counter_and_does_not_panic`** — core. Poisons the recorder per §5.1, then:
   - Asserts `emit_synthetic` returns normally (no panic).
   - Samples the atomic counter immediately before/after the call; asserts `after >= before + 1` (the per-call delta is at least the 1 increment this call contributes; `>=` rather than `==` because `cargo test` runs the file's tests in parallel and the counter is process-global).
   - Recovers the recorder vec via `PoisonError::into_inner()` and asserts it is **empty** — the event was telemetered, not silently pushed into the buffer.

3. **`emit_synthetic_poisoned_recorder_counter_is_monotonic_across_calls`** — monotonicity. Three independent poisoned recorders, three `emit_synthetic` calls; per-call delta asserted to be `>= 1` and `after >= before` (non-decreasing). Pins the counter is process-global and each call contributes at least one increment.

### 5.3 Why this pins the bug

A test that mocked the fail-CLOSED path without poisoning a real `std::sync::Mutex` would not exercise the `Err(PoisonError)` branch — the compiler would happily allow the silent-drop pattern to remain. By poisoning a real mutex through real thread-panic propagation (Rust's standard mechanism), the test exercises the EXACT branch flagged by the wave-26 adversarial review.

The recorder buffer assertion (test #2) is the critical one: if a future refactor regresses to the silent-drop pattern, the counter assertion would still pass (counter increments would just be missing), but the buffer-empty assertion AND the counter-incremented assertion together pin the contract: "the event MUST be telemetered through the side-channel AND MUST NOT be silently dropped INTO the buffer".

---

## 6. INV impact — INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER clarification

The registry row at `specs/03_architecture/invariant_registry.md §3.7` (line 241) has been expanded with a W26-P2-08 clarification:

- The "with handler" suffix now ALSO covers synthetic-emit paths (`AuditSink::emit_synthetic`).
- Silent drops on the audit-emit failure are **forbidden** because the operator loses observability into whether the 503 fail-CLOSED arm completed its audit row.
- The handler MUST record telemetry via a side-channel that itself cannot re-fault — atomic counter + structured `tracing` line.
- The recording side-channel MUST NOT re-acquire the faulted resource nor call back into the audit chain (no double-fault).

The TLA spec `audit_emit_atomic.tla` still covers the primary invariant (D1-batch pairing); the side-channel telemetry is asserted by the integration test rather than the TLA spec because (a) lock-free atomics are not naturally modelled in TLC, and (b) the side-channel is a fault-injection property, not a state-machine reachability property.

---

## 7. Gates + results

| Gate | Command | Result |
|---|---|---|
| Build | `cargo build -p corelink-clerk-cf --features tenant-region-real` | green (compiles clean; ~3.8s incremental) |
| Test | `cargo test -p corelink-clerk-cf --features tenant-region-real` | green — 23 + 3 + 3 + 6 + 6 + 2 = **43 tests** pass; **3 new** W26-P2-08 tests pass on first SEAL run |
| Clippy | `cargo clippy -p corelink-clerk-cf --features tenant-region-real --all-targets -- -D warnings` | green (no warnings; test-file scoped `clippy::int_plus_one` + `clippy::indexing_slicing` allow-attrs with `reason = "…"` per crate convention) |
| Specs validator | `python3 scripts/validate_specs.py` | green (run at SEAL time) |
| References validator | `python3 scripts/validate_references.py` | green (run at SEAL time) |

Charter constraints satisfied:
- `#![forbid(unsafe_code)]` preserved.
- No `unwrap` / `expect` / `panic` in `src/` (the test file panics inside a spawned thread to set up the poison scenario; that's the canonical Rust idiom and is scoped via test-file-level `#![allow(clippy::panic, ...)]`).
- `subtle::ConstantTimeEq` — not relevant on this path (no sensitive comparison surfaces).
- Fail-CLOSED preserved — the 503 still fires; verified by the existing wave-26 `cf_worker_prefetch_wire.rs` tests (still green post-fix).

---

## 8. DCO sign-off

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
