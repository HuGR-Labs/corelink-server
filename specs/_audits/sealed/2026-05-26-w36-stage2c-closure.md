---
id: "AUDIT-2026-05-26-W36-STAGE2C-CLOSURE"
type: "audit"
doc_status: "ACTIVE"
audit_status: "PARTIAL-SEAL"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
closed: null
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit","wave-36","stage-2c","closure","consumer-migration","follow-up","partial-seal"]
references:
  - "specs/_audits/sealed/2026-05-22-w33-stage2-c-adapter-splits.md"
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/sealed/2026-05-26-w35-adapter-host-prep.md"
---

# Wave 36 Stage 2.C Closure — Consumer Migration (Partial SEAL)

> **Status: PARTIAL-SEAL** — Two structural hard pause triggers
> activated during the migration attempt. Three consumer files
> successfully migrated to canonical umbrella paths. Six consumer
> files remain blocked by dep-graph cycles and wasm32 constraints;
> both are documented with precise root causes and escalation paths.

## §1. Scope

This audit covers **Wave 33+34 closure-followups §3 follow-up #1**:
migrate all workspace consumer files away from direct
`corelink_<absorbed-crate>::*` imports onto canonical umbrella paths:

- `corelink_stripe_real::*` → `corelink_billing::stripe::real::*`
- `corelink_statuspage_real::*` → `corelink_ops::statuspage::*`
- `corelink_slack_real::*` → `corelink_ops::slack::*`
- `corelink_clerk_cf::*` → `corelink_auth::clerk_cf::*`

Source: `specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md`
§3 follow-up #1.

## §2. Pre-migration surface

### 2.1 Absorbed crates × consumer files

Full surface enumerated by orchestrator grep sweep. All 17 files
with direct absorbed-crate imports:

| Absorbed crate | File | Import pattern | Migration status |
|---|---|---|---|
| `corelink-stripe-real` | `crates/corelink-adapters-cloud/src/stripe.rs` | `pub use corelink_stripe_real::*` | KEPT — canonical re-export shim |
| `corelink-stripe-real` | `crates/corelink-billing/src/stripe.rs` | `pub use corelink_stripe_real::*` | KEPT — umbrella re-export (exception) |
| `corelink-stripe-real` | `crates/corelink-billing-stripe-materializer/src/audit.rs` | `use corelink_stripe_real::webhook_dispatch::*` | BLOCKED — dep cycle |
| `corelink-stripe-real` | `crates/corelink-billing-stripe-materializer/src/handler.rs` | `use corelink_stripe_real::webhook_dispatch::*` | BLOCKED — dep cycle |
| `corelink-stripe-real` | `crates/corelink-billing-stripe-materializer/src/idempotency.rs` | `use corelink_stripe_real::webhook_dispatch::*` | BLOCKED — dep cycle |
| `corelink-stripe-real` | `crates/corelink-billing-stripe-materializer/tests/materializers_e2e.rs` | `use corelink_stripe_real::*` (2 lines) | BLOCKED — dep cycle |
| `corelink-stripe-real` | `crates/corelink-container/src/main.rs` | `use corelink_stripe_real::webhook_dispatch::*` | **MIGRATED** |
| `corelink-stripe-real` | `crates/corelink-container/src/webhook.rs` | `use corelink_stripe_real::*` (2 sites) | **MIGRATED** |
| `corelink-stripe-real` | `crates/corelink-container/tests/webhook_unified.rs` | `use corelink_stripe_real::*` (2 lines) | **MIGRATED** |
| `corelink-statuspage-real` | `crates/corelink-adapters-cloud/src/statuspage.rs` | `pub use corelink_statuspage_real::*` | KEPT — canonical re-export shim |
| `corelink-statuspage-real` | `crates/corelink-ops/src/statuspage.rs` | `pub use corelink_statuspage_real::*` | KEPT — umbrella re-export (exception) |
| `corelink-statuspage-real` | `crates/corelink-dsr-statuspage-scheduler/src/scheduler.rs` | `use corelink_statuspage_real::*` (2 sites) | BLOCKED — wasm32 hard pause |
| `corelink-statuspage-real` | `crates/corelink-dsr-statuspage-scheduler/tests/dsr_statuspage_cron.rs` | `use corelink_statuspage_real::*` | BLOCKED — wasm32 hard pause |
| `corelink-slack-real` | `crates/corelink-adapters-cloud/src/slack.rs` | `pub use corelink_slack_real::*` | KEPT — canonical re-export shim |
| `corelink-slack-real` | `crates/corelink-ops/src/slack.rs` | `pub use corelink_slack_real::*` | KEPT — umbrella re-export (exception) |
| `corelink-clerk-cf` | `crates/corelink-adapters-cloud/src/clerk.rs` | `pub use corelink_clerk_cf::*` | KEPT — canonical re-export shim |
| `corelink-clerk-cf` | `crates/corelink-auth/src/clerk_cf.rs` | `pub use corelink_clerk_cf::*` | KEPT — umbrella re-export (exception) |

### 2.2 Structural notes

The 4 umbrella re-export files (billing/stripe.rs,
ops/statuspage.rs, ops/slack.rs, auth/clerk_cf.rs) are the KEPT
exception per the closure-followup §3 spec. The 4 adapters-cloud
files are also canonical re-export shims whose function is to expose
the HTTPS surface through the adapters-cloud namespace — they are
preserved to not break `corelink_adapters_cloud::*` consumers.

## §3. Migration applied

### 3.1 Successfully migrated: corelink-container (3 files)

**Cargo.toml change:** `crates/corelink-container/Cargo.toml` —
added `corelink-billing = { workspace = true }` as a dependency.
Cycle check: `corelink-billing` does NOT depend on
`corelink-container`; no cycle introduced.

**src/main.rs** (line 30):
```
- use corelink_stripe_real::webhook_dispatch::{
+ use corelink_billing::stripe::real::webhook_dispatch::{
      RecordingSliRecorder, StateMaterializer, SystemClock, WebhookDispatcher,
  };
```

**src/webhook.rs** (line 53):
```
- use corelink_stripe_real::webhook_dispatch::{DispatchResponse, WebhookDispatcher};
+ use corelink_billing::stripe::real::webhook_dispatch::{DispatchResponse, WebhookDispatcher};
```

**src/webhook.rs** inline test (line 151):
```
- use corelink_stripe_real::webhook_dispatch::{
+ use corelink_billing::stripe::real::webhook_dispatch::{
      AuditOutcome, DispatchResponse, FixedClock, InMemoryIdempotencyStore,
      RecordingAuditEmitter, RecordingSliRecorder, RecordingStateMaterializer,
  };
```

**tests/webhook_unified.rs** (lines 41-45):
```
- use corelink_stripe_real::webhook::compute_signature;
- use corelink_stripe_real::webhook_dispatch::{
+ use corelink_billing::stripe::real::webhook::compute_signature;
+ use corelink_billing::stripe::real::webhook_dispatch::{
      AuditOutcome, CanonicalWebhookEventType, FixedClock, InMemoryIdempotencyStore,
      RecordingAuditEmitter, RecordingSliRecorder, RecordingStateMaterializer, WebhookDispatcher,
  };
```

Canonical umbrella path: `corelink_billing::stripe::real::*` (defined
in `crates/corelink-billing/src/stripe.rs` as `pub mod real { pub use
corelink_stripe_real::*; }`).

### 3.2 Rollback: corelink-dsr-statuspage-scheduler

Initially migrated; reverted when wasm32 hard pause trigger activated
(see §5.2). Files restored to baseline `23547dc1` state.

### 3.3 Untouched: corelink-billing-stripe-materializer

Not attempted; dep-graph cycle analysis done pre-mutation (see §5.1).
Files unchanged from baseline `23547dc1`.

## §4. Gates — all GREEN for migrated files

### Build
```
cargo build --workspace
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 3m 56s
→ GREEN
```

### Clippy
```
cargo clippy --workspace --all-targets -- -D warnings
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 14s
→ GREEN (zero warnings promoted to errors)
```

### Test compile
```
cargo test --workspace --no-run
→ Finished `test` profile [unoptimized + debuginfo] target(s)
→ GREEN (all test executables compiled)
```

### wasm32 (corelink-clerk-cf)
```
cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.39s
→ GREEN
```

All 4 gates GREEN at the same HEAD with migrated + unchanged files.

## §5. Hard pause triggers activated

Two structural hard pause triggers were activated during this
migration attempt. Both documented per charter hard-pause protocol.

### §5.1 Hard Pause Trigger A — Dep-graph cycle (billing → materializer)

**Files blocked:**
- `crates/corelink-billing-stripe-materializer/src/audit.rs`
- `crates/corelink-billing-stripe-materializer/src/handler.rs`
- `crates/corelink-billing-stripe-materializer/src/idempotency.rs`
- `crates/corelink-billing-stripe-materializer/tests/materializers_e2e.rs`

**Root cause:** `corelink-billing` already depends on
`corelink-billing-stripe-materializer` (`corelink-billing/Cargo.toml`
line 10: `corelink-billing-stripe-materializer = { workspace = true
}`). Adding `corelink-billing` as a dep to the materializer creates
the cycle:

```
corelink-billing → corelink-billing-stripe-materializer → corelink-billing
```

Cargo rejects this with:
```
use of unresolved module or unlinked crate `corelink_billing`
= help: if you wanted to use a crate named `corelink_billing`, use
  `cargo add corelink_billing` to add it to your `Cargo.toml`
```

(Note: `cargo add` would trigger the cycle; Cargo's dep-graph
resolver would reject it at resolution time.)

**Analysis:** `corelink-billing-stripe-materializer` is a direct
CHILD of the `corelink-billing` umbrella. It is not an independent
consumer — it IS part of the billing family. The correct resolution
is a dep-graph inversion (as originally documented in
`specs/_audits/sealed/2026-05-22-w33-stage2-c-adapter-splits.md` §5):
the materializer's trait imports (AuditEmitter, IdempotencyStore,
StateMaterializer, etc.) should be defined INSIDE `corelink-billing`
(not in `corelink-stripe-real`), and `corelink-stripe-real`'s adapter
implementations should import from `corelink-billing`. This is the
6-crate dep-graph inversion deferred from Stage 2.C.

**Escalation path:** Wave 36 Stage 3 cargo-deny lockdown should
include the materializer exception explicitly, OR the dep-graph
inversion from the Stage 2.C HALT audit §5 should be executed as a
dedicated sprint before the cargo-deny lockdown enforces deny-direct
rules.

### §5.2 Hard Pause Trigger B — wasm32 mio/tokio transitive pull

**Files blocked:**
- `crates/corelink-dsr-statuspage-scheduler/src/scheduler.rs`
- `crates/corelink-dsr-statuspage-scheduler/tests/dsr_statuspage_cron.rs`

**Root cause:** `corelink-dsr-statuspage-scheduler` is a dependency
of `corelink-clerk-cf` (verified in `crates/corelink-clerk-cf/Cargo.toml`
line 125). Adding `corelink-ops` to the scheduler pulls `tokio` (with
`mio v1.2.0`) onto the wasm32 target via:

```
corelink-clerk-cf
  └─ corelink-dsr-statuspage-scheduler
       └─ corelink-ops (new dep)
            └─ ... → tokio v1.52.3
                         └─ mio v1.2.0
```

`mio v1.2.0` on wasm32 emits 48 compilation errors:
```
error: This wasm target is unsupported by mio. If using Tokio,
       disable the net feature.
```

**Verification:** git stash confirmed the wasm32 build is GREEN at
baseline `23547dc1` without the scheduler change. After restoring the
change, mio failures reappear on `cargo build --target
wasm32-unknown-unknown -p corelink-clerk-cf`. After reverting the
scheduler Cargo.toml change, wasm32 goes GREEN again.

**Escalation path:** The DSR statuspage scheduler migration requires
one of:

1. **Feature-gating `corelink-ops`** — add `corelink-ops` to the
   scheduler under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`
   and provide a wasm32-compatible alternative path (e.g., directly
   from `corelink-statuspage-real` under wasm32, from
   `corelink-ops::statuspage` on native).

2. **Upstream fix in `corelink-ops`** — remove the transitive
   tokio/mio pull from `corelink-ops` on wasm32 targets (requires
   auditing which of the 28 absorbed crates pulls tokio unconditionally
   and gating it under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`).

3. **Defer scheduler migration to post-dep-graph-inversion sprint.**

## §6. Verification — partial

```bash
for crate in stripe-real statuspage-real slack-real clerk-cf; do
  echo "[$crate]"
  grep -rn "use corelink_${crate//-/_}::" \
    --include="*.rs" --exclude-dir=target \
    crates/ apps/ tools/ 2>/dev/null | \
    grep -v "crates/corelink-${crate}/" | \
    grep -v "pub use corelink_${crate//-/_}::" | \
    head
done
```

Results (as of this audit):

- `[stripe-real]` — 5 remaining lines in `corelink-billing-stripe-materializer`
  (4 files × import sites). Remaining lines = Trigger A blockers.
  `corelink-container` imports: ZERO (migrated).
- `[statuspage-real]` — 3 remaining lines in `corelink-dsr-statuspage-scheduler`
  (2 files × import sites). Remaining lines = Trigger B blockers.
- `[slack-real]` — EMPTY (no consumer migration needed; only re-export
  shims exist).
- `[clerk-cf]` — EMPTY (no consumer migration needed; only re-export
  shims exist).

## §7. Architectural note

The 4 absorbed crates `corelink-{stripe-real,statuspage-real,
slack-real,clerk-cf}` REMAIN as workspace members. They are the
canonical LOC owners; the umbrella re-export pattern stays. What
CHANGED in this audit: the non-cyclic consumer surface of
`corelink-container` (corelink-server) now uses canonical umbrella
paths instead of direct absorbed-crate imports.

What STILL BLOCKS full closure:

1. **Materializer dep-graph inversion** (Trigger A) — requires
   defining the stripe business trait surface (AuditEmitter,
   IdempotencyStore, StateMaterializer, etc.) inside
   `corelink-billing` and inverting the dep direction so
   `corelink-stripe-real` depends on `corelink-billing` (not the
   current inverse). This is the 6-crate dep-graph inversion from
   `specs/_audits/sealed/2026-05-22-w33-stage2-c-adapter-splits.md` §5.

2. **DSR scheduler wasm32 tokio gate** (Trigger B) — requires
   platform-gating `corelink-ops` in the scheduler's Cargo.toml or
   fixing `corelink-ops`'s wasm32 tokio pull upstream.

Once both blockers are resolved, the following unblocks:

- Wave 33+34 closure-followups §4 #2 FULLY: Stage 2.E Phase 2 — 72
  absorbed crates removal (materializer cycle must be resolved first).
- Wave 36 Stage 3 cargo-deny lockdown can enforce deny-direct rules
  for `corelink-stripe-real` + 3 siblings ONLY after all consumers
  (materializer + scheduler) are migrated. A cargo-deny exception will
  be required for materializer until Trigger A is resolved.

## §8. What's next

1. **Trigger A — materializer dep-graph inversion sprint** (new wave
   36 sprint, estimated 2-3 days):
   - Define `AuditEmitter`, `IdempotencyStore`, `StateMaterializer`,
     `WebhookDispatcher` ports inside `corelink-billing::stripe::*`.
   - Move `corelink-stripe-real`'s adapter implementations to depend
     on `corelink-billing` (inversion).
   - Remove the materializer's `corelink-stripe-real` dep; update its
     imports to `corelink-billing::stripe::*`.
   - References: §5.1 + HALT audit §5.

2. **Trigger B — scheduler wasm32 gate** (small wave 36 task):
   - Platform-gate `corelink-ops` in scheduler Cargo.toml under
     `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`.
   - On wasm32, keep direct `corelink-statuspage-real` import OR
     create a wasm32-compatible statuspage umbrella path.

3. **cargo-deny lockdown** (Wave 36 Stage 3): After #1 + #2, add
   `deny-direct` rules for `corelink-stripe-real`,
   `corelink-statuspage-real`, `corelink-slack-real`,
   `corelink-clerk-cf` in `cargo-deny.toml`. The lockdown enforces
   that only the umbrella re-export shims may import directly.

## §9. Charter compliance recap

All charter invariants preserved in migrated and unchanged files:

- `#[non_exhaustive]` — preserved on all existing enums/structs
  (no new types created).
- `#![forbid(unsafe_code)]` — preserved; no unsafe added.
- `subtle::ConstantTimeEq` — preserved in `idempotency.rs` (unchanged).
- `SecretString` — preserved on all credential fields (unchanged).
- Audit fail-CLOSED — preserved (no audit chain logic touched).
- Wave-31 wallet broker dual-mode — preserved (unchanged; the
  `corelink_billing::stripe::real::*` umbrella path exposes the same
  `StripeAuthMode::Direct` / `StripeAuthMode::WalletBroker` variants).

Test count delta: zero (no tests added or removed; test executables
all compile clean via `cargo test --workspace --no-run`).

## §10. Closure note — W36 Stage 3 cargo-deny lockdown enforced (2026-05-27)

The §8 step 3 "cargo-deny lockdown" task is now executed and SEALed
in `specs/_audits/sealed/2026-05-27-w36-stage-3-seal.md`. The lockdown
adds 4 `[bans] deny` entries (one per absorbed adapter crate) to
the workspace `deny.toml`, each with a `wrappers = [...]` allowlist
enumerating exactly the legal historical direct importers (Wave-33
umbrella façades + this audit's dangling `corelink-server` dep +
the `e2e-signup-flow` workspace test crate + the Trigger B wasm32
fallback edges). New consumers attempting direct imports fail CI at
the cargo-deny gate with an explanatory error pointing to the
canonical umbrella path.

Combined with Trigger A SEAL (materializer src migrated to
`corelink-billing-stripe-traits`,
`specs/_audits/sealed/2026-05-27-w36-trigger-a-seal.md`) and Trigger B
SEAL (scheduler `corelink-ops` platform-gated to `not(wasm32)`,
`specs/_audits/2026-05-27-w36-trigger-b-seal.md`), this Partial-SEAL
is upgraded to **FULL Wave-36 closure** end-to-end: closure-followups
§3 follow-up #1 fully closed via path (c).

## §11. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com> (§10 closure
note 2026-05-27 W36 Stage 3 SEAL).

**End of Wave 36 Stage 2.C Closure audit (Partial SEAL upgraded to
FULL closure 2026-05-27 via W36 Stage 3 SEAL).**
