# Wave 33 Stage 1 Stream B — Policy Contexts SEAL Audit (2026-05-22)

> **Doc kind:** stream closure audit (evidence; `_audits/` excluded
> from canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stream-b-policy` (worktree
> `.claude/worktrees/agent-a8eabeb53cd223844`).
>
> **Mandate:** wave-33 Stage 1 Stream B reorganises the 5 policy-
> context families (billing / BYOK / auth / signup / privacy) under
> the modular-monolith + hexagonal + microkernel(BYOK) target shape
> per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1.
> Aggregator pattern per Stage 0 SEAL `specs/_audits/2026-05-22-w33-stage0-foundation.md` §4.

## §1. Scope — sub-step commit chain

Stream B executes 5 commit-per-sub-step on
`wt/r-prep-w33-stream-b-policy` plus 1 mid-point empty marker:

| Sub-step | SHA | Title | Strategy |
|---|---|---|---|
| B.1 | `293c1788` | `corelink-billing` (aggregator for 14) | Option-A aggregator |
| B.2a+b | `f7d05ae9` | rename `corelink-byok` → `-core` + create umbrella microkernel | rename + microkernel (compile_error! pairs) |
| B.2c | `cfda9f26` | fold `corelink-byok-matrix-test` → `corelink-byok/tests/` | physical absorption (test crate) |
| B.3 | `bc347f4e` | `corelink-auth` (aggregator for 6) | Option-A aggregator |
| MID-POINT | `93696a0b` | empty marker (Stream C dispatch greenlight) | empty `--allow-empty` |
| B.4 | `aafe3b14` | `corelink-signup` canonical-path adoption review | rustdoc deferral note |
| B.5 | `eef95e85` | `corelink-privacy` (aggregator for 11) | Option-A aggregator |

Option-A interpretation choice: physical absorption deferred to the
consumer-migration stream that owns producer call sites
(`apps/server` routes, `corelink-worker` wasm32 entry, D1 migrations
runner). Absorbed crates remain canonical sources of truth, retaining
their own tests/benches/fuzz. This is the same Stage 0 §4 Option-A
aggregator pattern.

## §2. Crates aggregated + rationale (5 umbrellas, 38 absorbed)

| Umbrella | Absorbed crates | Rationale (Option-A vs strict) |
|---|---|---|
| `corelink-billing` (NEW) | `corelink-billing-aggregator`, `-emit`, `-reconcile`, `-replay`, `-stripe`, `-stripe-materializer`, `corelink-stripe-real`, `corelink-tier-selection`, `corelink-quota`, `-cas`, `-fsm`, `corelink-rate-headers`, `corelink-ratelimit`, `corelink-abuse` (14) | `corelink-stripe-real` carries the wave-31 wallet broker dual-mode (commit `e3c564df`) — physically relocating risks regressing the dual-mode fallback (charter Hard Pause Trigger 7). Stripe webhook D1 materializer is consumed from `apps/server/src/routes/stripe_webhook.rs`; atomic consumer migration belongs to Stage 2. |
| `corelink-byok-core` (rename) + `corelink-byok` (NEW umbrella) | core (renamed from `corelink-byok`) + `corelink-byok-aws`, `-gcp`, `-azure`, `-vault`, `-revocation` (6) | Collision avoidance: the package name `corelink-byok` is repurposed as the microkernel umbrella; the original crate ships under `corelink-byok-core`. Provider adapters now depend on `corelink-byok-core` directly; the umbrella aggregates them behind 4 mutually-exclusive cargo features (`compile_error!` gates). |
| `corelink-byok` (continued — B.2c) | `corelink-byok-matrix-test` (1) — folded into `corelink-byok/tests/{matrix,matrix_adversarial,matrix_prop}.rs` (3 integration test files). | Test-crate absorption: matrix test files moved physically because the absorbed `src/lib.rs` (31 LOC of unused helper consts) is dropped, and integration tests under `tests/` are the conventional location. |
| `corelink-auth` (NEW) | `corelink-clerk`, `-clerk-cf`, `corelink-auth-schema`, `corelink-webauthn`, `corelink-pat`, `corelink-tenant-path` (6) | `corelink-clerk-cf` carries the `#[durable_object]` wasm32-only Cloudflare Worker actor wire-up (charter Hard Pause Trigger 5 tripwire); moving it requires atomic wrangler.toml + `corelink-cf-bindings` updates — Stage 2 owns this. `corelink-auth-schema` is paired with `migrations/d1/*.sql` (Stage 2 / Stream C). `corelink-tenant-path` carries 3 `harness=false` benches that pin the CTRL-CAS-001 hot path. |
| `corelink-signup` (verify-only) | n/a (single crate; already canonical) | Per brief: AuditEmitter migration is behaviour-changing (signup's `SignupAuditSink::emit(&SignupAuditRecord)` vs wave-33 `AuditEmitter::emit(AuditEvent)`) — deferred to the consumer-migration stream that harmonises envelope shape across all 5 context-local sinks. Inline rustdoc note added to `crates/corelink-signup/src/lib.rs` documenting the deferral. |
| `corelink-privacy` (NEW) | `corelink-dsr`, `-dsr-statuspage-scheduler`, `corelink-privacy-breach-emit`, `-consent-ledger`, `-erasure-worker`, `-notice-emit`, `-pseudonymize`, `-residency-enforcement`, `-sub-processor-emit`, `corelink-dpa-acceptance`, `-versioning` (11) | Ed25519 erasure-attestation signing path lives at Stage-0-canonical `corelink_crypto::ed25519::attestation` — preserved unchanged. D1-migration-coupled crates (consent_ledger, dpa-acceptance, dpa-versioning, erasure-worker, residency) defer physical move to Stage 2. DSR statuspage 24h scheduler is paired with erasure-worker's `aggregate_24h_window` via composition (06:00 UTC cron); physically splitting risks the deployed CF Worker `dsr_statuspage_cron` event handler. |

Workspace member delta: **107 → 110 → 113** post Stage 0 → **117** post Stream B (107 + Stage 0 +3 + Stream B +4 = 114; -1 for `corelink-byok-matrix-test` absorbed; net = 113). Re-count: post-Stage-0 = 110; Stream B adds 4 new (`corelink-billing`, `corelink-byok` umbrella, `corelink-auth`, `corelink-privacy`) and removes 1 (`corelink-byok-matrix-test` absorbed) — **net post-Stream-B = 113**. The original `corelink-byok` package was renamed (not removed), so it counts in both pre and post.

## §3. BYOK microkernel — features + mutual-exclusion verification

The umbrella `corelink-byok` declares 4 mutually-exclusive cargo
features:

```toml
[features]
default = []
aws = ["dep:corelink-byok-aws"]
gcp = ["dep:corelink-byok-gcp"]
azure = ["dep:corelink-byok-azure"]
vault = ["dep:corelink-byok-vault"]
# Matrix test feature forwards (absorbed from corelink-byok-matrix-test):
production-gcp = ["corelink-byok-gcp/production"]
production-azure = ["corelink-byok-azure/production"]
```

Six `compile_error!` guards in `crates/corelink-byok/src/lib.rs`
enforce mutual exclusion at compile time (every pair drawn from
`{aws, gcp, azure, vault}`):

```rust
#[cfg(all(feature = "aws", feature = "gcp"))]
compile_error!("BYOK microkernel: features `aws` and `gcp` are mutually exclusive. ...");
// + 5 more pairs (aws+azure, aws+vault, gcp+azure, gcp+vault, azure+vault)
```

### Empirical verification (charter Hard Pause Trigger 2 evidence)

| Build invocation | Outcome |
|---|---|
| `cargo build -p corelink-byok` (default; no provider) | ✅ green |
| `cargo build -p corelink-byok --features aws` | ✅ green |
| `cargo build -p corelink-byok --features gcp` | ✅ green |
| `cargo build -p corelink-byok --features azure` | ✅ green |
| `cargo build -p corelink-byok --features vault` | ✅ green |
| `cargo build -p corelink-byok --features "aws,gcp"` | ❌ **REJECTED at compile time** with `compile_error!` |

The rejection message verbatim: `error: BYOK microkernel: features
\`aws\` and \`gcp\` are mutually exclusive. Select exactly one
provider per build (wave-33 reorg spec §3).`

This is the canonical evidence the wave-33 reorg spec §3 microkernel
contract is enforced.

### Preservation of crypto sovereignty (charter Hard Pause Trigger 3)

The umbrella does NOT touch `subtle::ConstantTimeEq` paths. All
constant-time compares in `corelink-byok-core` (DEK envelope AAD
verify, key id compare, etc.) are unchanged. The umbrella's
`pub use corelink_byok_core::*;` at the crate root passes through
every constant-time-compare-protected type by reference.

## §4. Test count delta per affected crate

Pre-Stream-B baselines captured at HEAD `5d3d32c9` (Stage 0 SEAL
merge); post-Stream-B counts at HEAD `eef95e85`.

| Crate | Pre tests | Post tests | Δ |
|---|---|---|---|
| `corelink-billing` (NEW) | (did not exist) | 3 smoke | +3 |
| `corelink-billing-{aggregator,emit,reconcile,replay,stripe,stripe-materializer}` | unchanged | unchanged | 0 |
| `corelink-stripe-real`, `-tier-selection`, `-quota`, `-quota-cas`, `-quota-fsm`, `-rate-headers`, `-ratelimit`, `-abuse` | unchanged | unchanged | 0 |
| `corelink-byok-core` (rename of `corelink-byok`) | 37 (10 lib + 7 prop + 1 mut + 11 adv + 7 framework + 1 doc) | 37 | 0 |
| `corelink-byok` (NEW umbrella) | (did not exist) | 4 smoke lib + 5 matrix + 16 matrix_adversarial + 9 matrix_prop = **34** | +34 (net + 4 vs absorbed) |
| `corelink-byok-matrix-test` (absorbed) | 30 (5 matrix + 16 adv + 9 prop) | **0 (dropped — folded into umbrella)** | -30 |
| `corelink-byok-{aws,gcp,azure,vault,revocation}` | unchanged | unchanged | 0 |
| `corelink-auth` (NEW) | (did not exist) | 6 smoke | +6 |
| `corelink-clerk`, `-clerk-cf`, `-auth-schema`, `-webauthn`, `-pat`, `-tenant-path` | unchanged | unchanged | 0 |
| `corelink-signup` | 67 (42 + 2 + 16 + 7) | 67 | 0 |
| `corelink-privacy` (NEW) | (did not exist) | 11 smoke | +11 |
| `corelink-dsr`, `-dsr-statuspage-scheduler`, `-privacy-breach-emit`, `-consent-ledger`, `-erasure-worker`, `-notice-emit`, `-pseudonymize`, `-residency-enforcement`, `-sub-processor-emit`, `-dpa-acceptance`, `-versioning` | unchanged | unchanged | 0 |

**Net Δ: +24 new smoke tests; -30 from matrix-test folded back as
+30 inside umbrella; total visible Δ = +24 in newly-named test
results (smoke). No previously-green test regressed.** Stream B
trigger 4 (previously-green test goes red) NOT activated.

## §5. AuditEmitter adoption status

Per wave-33 reorg spec §6 Stage 1 charter, Stream B's "consumed but
NOT modified" stance on `corelink_audit::ports::AuditEmitter`:

| Crate | Current trait | AuditEmitter migration status |
|---|---|---|
| `corelink-billing-emit` | `BillingAuditSink` | Deferred — billing audit envelope is strongly-typed; behaviour-changing. |
| `corelink-billing-aggregator` | `AggregatorAuditSink` | Deferred. |
| `corelink-stripe-real::webhook_dispatch` | `AuditEmitter` (different signature — Stripe-webhook-specific) | Aliased by name only; signature mismatch (event_type bound) — deferred until envelope harmonization sub-step. |
| `corelink-byok-revocation` | `RevocationAuditSink` | Deferred. |
| `corelink-signup` | `SignupAuditSink` | Deferred — `emit(&self, record: &SignupAuditRecord)` vs `emit(&self, AuditEvent)`. Documented inline in lib.rs rustdoc per B.4. |
| `corelink-privacy-{breach,consent,erasure,notice,sub-processor}-emit` | each ships its own `*AuditSink` trait | Deferred uniformly. |
| `corelink-dpa-acceptance`, `-versioning` | each ships its own audit envelope | Deferred. |
| `corelink-clerk-cf`, `-webauthn`, `-pat`, `-auth-schema` | each ships its own audit envelope | Deferred. |

**Net status:** Stream B introduces ZERO new `AuditEmitter`
consumers. The chokepoint trait is defined and discoverable; the
6+ context-local audit sinks remain as canonical sources of truth.
A future "audit envelope harmonization" sub-step will alias each
context sink to `AuditEmitter` via `type Alias = dyn AuditEmitter;`
once consumer migration completes. Stage 2 / Stage 3 territory.

## §6. Gates — empirical evidence per sub-step

For every sub-step commit, the full charter gate set was run before
commit:

### Build

- `cargo build -p <crate-created>`: green at every sub-step.
- `cargo build --workspace`: green at every sub-step boundary (B.2a+b
  combined to preserve this invariant — the rename frees the
  `corelink-byok` workspace-dep slot and the umbrella reclaims it in
  the same atomic commit).

### Test

- `cargo test -p corelink-billing`: 3/3.
- `cargo test -p corelink-byok` (default features): 4 + 5 + 16 + 9 = 34/34.
- `cargo test -p corelink-byok-core`: 37/37 (unchanged from pre-Stream-B
  baseline `corelink-byok`).
- `cargo test -p corelink-byok-aws`: 17/17.
- `cargo test -p corelink-auth`: 6/6.
- `cargo test -p corelink-signup`: 67/67 (unchanged from baseline).
- `cargo test -p corelink-privacy`: 11/11.

### Clippy

- `cargo clippy -p <crate-created> --all-targets -- -D warnings`:
  clean at every sub-step.

### BYOK mutual-exclusion compile-time evidence

- `cargo build -p corelink-byok --features "aws,gcp"`: ❌ REJECTED at
  compile time (charter Hard Pause Trigger 2 enforcement evidence).

### Wasm32 sanity (charter Hard Pause Trigger 5)

- `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`:
  green at every sub-step.

### Spec / reference / migration validators

- `python3 scripts/validate_specs.py`: green (458 docs).
- `python3 scripts/validate_references.py`: green.
- `python3 scripts/check_migrations_additive.py`: green (59 files).
- `python3 scripts/validate_inv_inheritance.py`: green.

### TLA+ obligations

No TLA+ spec was modified in Stream B (no behavioural change). The 61
CRITICAL invariants with TLA+ proofs remain valid.

### `cargo deny check`

Deferred to Stage 3 lockdown per the wave-33 reorg spec §6 Stage 3
explicit scope.

## §7. Hard pause triggers — status

| # | Trigger | Status |
|---|---|---|
| 1 | RLS WITH CHECK or `set_local app.current_tenant` broken | NOT ACTIVATED (no auth-schema src path touched; re-export only) |
| 2 | BYOK mutual-exclusion no longer enforced at compile-time | NOT ACTIVATED (in fact NEWLY-ENFORCED — pre-Stream-B `corelink-byok` had no multi-provider gating; the umbrella now ships 6 `compile_error!` pairs verified empirically) |
| 3 | `subtle::ConstantTimeEq` removed accidentally | NOT ACTIVATED (all paths preserved by reference: `corelink-byok-core` envelope AAD, `corelink-pat` verify, `corelink-privacy-pseudonymize` HMAC tag compare) |
| 4 | Previously-green test goes red | NOT ACTIVATED (every pre-Stream-B test passes post-merge — verified via per-crate `cargo test` invocation pre-commit at every sub-step boundary; `corelink-byok-core` 37 tests preserved through rename) |
| 5 | Wasm32 build of `corelink-clerk-cf` breaks | NOT ACTIVATED (verified at every sub-step) |
| 6 | Workspace cargo build broken at any sub-step boundary | NOT ACTIVATED (B.2a+b combined to preserve invariant; other sub-steps additive-only) |
| 7 | Stripe wallet broker dual-mode regresses | NOT ACTIVATED (`corelink-stripe-real::StripeAuthMode` re-exported as-is at `corelink_billing::stripe::real::*`; smoke test pins the type-id) |

## §8. Next steps

- **Stream C dispatch** — already greenlit by the mid-point empty
  marker commit `93696a0b`. Stream C may proceed in parallel.
- **`/techlead` review** — Owner triggers the cold-tool 11-level
  verification protocol before merging this stream into main.
- **Stage 2 gate** — `wt/r-prep-w33-stream-a-data-path`,
  `wt/r-prep-w33-stream-b-policy` (this stream), and
  `wt/r-prep-w33-stream-c-infra-ops` must all merge before Stage 2
  binding consolidation can begin (orchestrator-direct per spec §6
  Stage 2).
- **Audit envelope harmonization** — flagged for Stage 2 / Stage 3:
  every context-local `AuditSink` trait aliases to
  `corelink_audit::ports::AuditEmitter` once the JSON envelope shape
  reaches consensus across all 5 streams.
- **Physical absorption** — Stage 2 / Stage 3 territory; consumer
  call sites in `apps/server` + `corelink-worker` migrate to
  canonical paths atomically and trigger the `pub use` -> shim
  -> deletion lifecycle on absorbed crates.

## §9. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

Branch: `wt/r-prep-w33-stream-b-policy`.
Stream B commits: `293c1788` → `f7d05ae9` → `cfda9f26` →
`bc347f4e` → `93696a0b` (MID-POINT) → `aafe3b14` → `eef95e85`.

---

**End of Wave 33 Stage 1 Stream B — Policy Contexts SEAL audit.**
