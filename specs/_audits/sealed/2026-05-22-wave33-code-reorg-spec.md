# Wave 33 — Code Reorganization Spec (2026-05-22)

> **Doc kind:** wave-scope spec (evidence; `_audits/` excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 after architectural pattern research (107 → 19 crates, modular monolith + hexagonal + EDA(audit) + actor(DO) + microkernel(BYOK)).
>
> **Trigger:** user mandate 2026-05-22 — "Isso ai ta uma bosta. Amadorismo total, nao tem arquitetura real ai. Qual e o melhor tipo de arq pro corelink? ... Quero o SOTA" → research dispatched → user greenlit hybrid Mod-Mono+Hex recommendation → this spec.
>
> **Charter compliance:** SOTA bar 8.5/10; trust-but-verify per `feedback-synchronous-agents`; rollback documented per stream; zero gambiarra; preserves every existing charter constraint (`#![forbid(unsafe_code)]`, no `unwrap()`/`expect()`/`panic!()` in src/, `subtle::ConstantTimeEq`, audit fail-CLOSED, `#[non_exhaustive]`, `SecretString` for credentials, trait-abstraction-defer pattern, TLA+ verification on 61 CRITICAL invariants).
>
> **Blocks:** Wave 32 Phases B-I (worker shim, container deploy, DNS production). Wave 33 must SEAL before Wave 32 Phase B starts.

## 1. Scope

Reorganize the CoreLink Rust workspace from **107 fragmented crates** to **19 bounded-context crates** following modular monolith + hexagonal architecture, while decomposing 5 mega-files (>76KB each) and preserving every functional behaviour + charter invariant.

Out of scope (deferred to future waves):
- Cross-context Saga implementation for transactions spanning bounded contexts (current code doesn't need it; flagged as future iteration per research §5).
- Full CQRS read models for analytics (Neon shadow continues; explicit materialized views deferred).
- Cell-based per-tenant isolation à la AWS (premature at 1-binary scale).

## 2. Architectural pattern (per research 2026-05-22)

**Modular Monolith (macro)** + **Hexagonal / Ports & Adapters (intra-module)** + **Event-Driven (scoped to audit chain)** + **Actor (Cloudflare Durable Objects, runtime layer only)** + **Microkernel (BYOK with 4 KMS providers via uniform port)**.

Decision matrix winner: Modular Monolith (122) ≫ Hexagonal (108) ≫ others. Top-2 are complements, not competitors: Mod-Mono resolves the *macro* (one binary, many bounded contexts), Hex resolves the *micro* (uniform shape inside each context). EDA scoped strictly to audit because REAPI v2 is synchronous request/response — full ES on hot path would catastrophically degrade latency. Actor model at runtime (DO), not source. Microkernel at BYOK only.

Full research report dispatched 2026-05-22; key citations: Cockburn (Hexagonal) [1], Tilkov (Modular Monolith) [10], Shopify Engineering (Deconstructing the Monolith) [5], Vernon (IDDD §4) [3], Young (CQRS) [15].

## 3. Target structure (19 crates)

```
crates/

# Cross-cutting (4):
├── corelink-core/                 # types, errors, time, tenant-id, region, secret-wrap
├── corelink-crypto/               # BLAKE3, Ed25519, HMAC, subtle::ConstantTimeEq
├── corelink-telemetry/            # OTLP, tracing, metrics, SLO emit
└── corelink-audit/                # EDA chokepoint; CloudEvents Merkle chain; fail-CLOSED policy

# Bounded contexts (11):
├── corelink-cas/                  # Domain 1: CAS read/write/verify, chunking, dedup, multipart
├── corelink-ac/                   # Domain 2: action cache, REAPI v2 schema
├── corelink-reapi/                # Protocol surface (handler.rs split internally)
├── corelink-billing/              # Domain 4: Stripe + tier + quota + rate-limit + abuse
├── corelink-privacy/              # Domain 5: DSR + erasure + ARCO + pseudonymize + consent + DPA + residency
├── corelink-byok/                 # Domain 6: microkernel; 4 features (aws/gcp/azure/vault)
├── corelink-auth/                 # Domain 7: Clerk JWT + WebAuthn + PAT + tenant-path scoping
├── corelink-replication/          # Domain 8: multi-region, failover, replica lag, rollout, R2 multipart
├── corelink-gc/                   # Domain 9: GC sweep + reconcile (split reconcile.rs internally)
├── corelink-signup/               # Domain 10: pilot signup HMAC tokens
└── corelink-ops/                  # Domains 11+12+13: oncall + statuspage + admin + customer alerts + ops tooling

# Bindings (4):
├── corelink-adapters-cloud/       # ALL CF SDKs + Stripe HTTPS + BetterStack HTTPS + AWS/GCP/Azure SDKs + Neon + PagerDuty + Slack + Resend
├── corelink-adapters-vault/       # HashiCorp Vault + HuGR Wallet broker
├── corelink-worker/               # CF Worker (wasm32) entry — TS in apps/worker/; this Rust crate hosts wasm-bridged logic
└── corelink-container/            # CF Container (native) entry — wraps apps/server/main.rs
```

**Hard rule (cargo-deny enforced post-Stream 4):** only `adapters-*`, `worker`, `container` may depend on async runtimes (`tokio`), Cloudflare SDKs (`worker`, `cf-bindings-*`), HTTPS clients (`reqwest`), or platform SDKs (`aws-*`, `google-cloud-*`, `azure_*`, `stripe-rust`). Context crates depend only on cross-cutting + std + pure-logic third-party.

## 4. Crate mapping (107 → 19, exhaustive)

| Target crate | Old crates absorbed | Notes |
|---|---|---|
| **`corelink-core`** | (NEW) | Pure types: `TenantId`, `Digest`, `Region`, `SecretWrap`, error enums |
| **`corelink-crypto`** | `corelink-hash`, `corelink-client-verify`, `corelink-erasure-attestation` | BLAKE3, Ed25519, HMAC, subtle wrappers |
| **`corelink-telemetry`** | `corelink-tracing`, `corelink-logpush`, `corelink-otel-export`, `corelink-canary`, `corelink-synthetic-pager`, `corelink-slo`, `corelink-lighthouse-tracker` | OTLP, structured tracing, SLO emit, synthetic-pager |
| **`corelink-audit`** | `corelink-audit`, `corelink-audit-chain`, `corelink-analytics` | EDA chokepoint; Merkle chain; audit_export + audit_analytics handlers split internally |
| **`corelink-cas`** | `corelink-handler-cas` (folded), `corelink-chunker`, `corelink-dedup`, `corelink-edge`, `corelink-eviction`, `corelink-lru-tracker`, `corelink-r2-multipart`, `corelink-multipart-schema`, `corelink-meta`, `corelink-manifest` | Hot path: CAS read/write/verify; large because chunking+dedup+eviction belong here |
| **`corelink-ac`** | `corelink-ac`, `corelink-ac-schema`, `corelink-handler-ac` (folded) | Action cache, schema |
| **`corelink-reapi`** | `corelink-reapi` | Protocol surface; `handler.rs` (124KB) split into 4-5 files by service surface |
| **`corelink-billing`** | `corelink-billing-aggregator`, `corelink-billing-emit`, `corelink-billing-reconcile`, `corelink-billing-replay`, `corelink-billing-stripe`, `corelink-billing-stripe-materializer`, `corelink-stripe-real` (pure-logic portion; HTTPS in adapters), `corelink-tier-selection`, `corelink-quota`, `corelink-quota-cas`, `corelink-quota-fsm`, `corelink-rate-headers`, `corelink-ratelimit`, `corelink-abuse` | Wallet broker dual-mode already landed; preserved |
| **`corelink-privacy`** | `corelink-dsr`, `corelink-dsr-statuspage-scheduler`, `corelink-privacy-breach-emit`, `corelink-privacy-consent-ledger`, `corelink-privacy-erasure-worker`, `corelink-privacy-notice-emit`, `corelink-privacy-pseudonymize`, `corelink-privacy-residency-enforcement`, `corelink-privacy-sub-processor-emit`, `corelink-dpa-acceptance`, `corelink-dpa-versioning` | 11 crates → 1; submodules per privacy concern |
| **`corelink-byok`** | `corelink-byok`, `corelink-byok-aws`, `corelink-byok-azure`, `corelink-byok-gcp`, `corelink-byok-vault`, `corelink-byok-revocation`, `corelink-byok-matrix-test` (→ `tests/`) | Microkernel; 4 cargo features (aws/gcp/azure/vault); mutual-exclusion enforced |
| **`corelink-auth`** | `corelink-clerk`, `corelink-clerk-cf` (pure-logic portion; CF bindings in adapters), `corelink-auth-schema`, `corelink-webauthn`, `corelink-pat`, `tenant-path` | tenant-path is small (CTRL-CAS-001 enforcer) but critical; folds here |
| **`corelink-replication`** | `corelink-region`, `corelink-replica-worker`, `corelink-replication-coordinator`, `corelink-failover-router`, `corelink-rollout-controller` | Multi-region + failover + canary rollout |
| **`corelink-gc`** | `corelink-gc` | `reconcile.rs` (82KB) split internally by phase |
| **`corelink-signup`** | `corelink-signup` | Pilot signup HMAC |
| **`corelink-ops`** | `corelink-oncall`, `corelink-statuspage-real` (pure-logic; HTTPS in adapters), `corelink-slack-real` (pure-logic), `corelink-admin-api`, `corelink-admin-dry-run`, `corelink-handler-admin`, `corelink-dual-approval`, `corelink-customer-alerts`, `corelink-enterprise-inquiry`, `corelink-survey`, `corelink-runbook-tracker`, `corelink-tenant-offboarding`, `corelink-deploy-verifier`, `corelink-terraform-drift-consumer`, `corelink-dr-drill`, `corelink-backup-verify`, `corelink-chaos-scheduler`, `corelink-rotation-adapters`, `corelink-rotation-worker`, `corelink-dt-cli`, `corelink-dt-reconcile`, `corelink-dt-webhook`, `corelink-drata-sync`, `corelink-supply-chain-policy`, `corelink-supply-verify`, `corelink-config-api`, `corelink-config-do`, `corelink-d1-migrations` | Largest absorption (28 → 1); submodules: `oncall`, `statuspage`, `admin`, `dr`, `compliance` |
| **`corelink-adapters-cloud`** | `corelink-cf-bindings`, `corelink-clerk-cf` (binding portion), `corelink-stripe-real` (HTTPS portion), `corelink-statuspage-real` (HTTPS portion), `corelink-slack-real` (HTTPS portion) | Single place for ALL platform SDKs + HTTPS clients |
| **`corelink-adapters-vault`** | `corelink-byok-vault` (HTTPS portion) | Vault + HuGR Wallet broker (already lands wave-31 dual-mode) |
| **`corelink-worker`** | `corelink-worker` (existing 19K LOC; **WILL BE DECOMPOSED**) | Current `corelink-worker` is misnamed — it's actually storage adapters (R2 read/write, cache, middleware, reapi adapter, region resolver, auth). **Decomposition plan §6 splits this across cas/ac/auth/replication contexts.** New `corelink-worker` becomes thin CF Worker entry. |
| **`corelink-container`** | (NEW; absorbs `apps/server/src/main.rs` boot + scaffolding; `apps/server` becomes Cargo bin alias) | gRPC server entry; calls into context crates |

### Out-of-tree migrations (not crates, side-effect of reorg)

| Old location | New location | Reason |
|---|---|---|
| `crates/corelink-go/` | `tools/sdks/go/` | Go SDK examples; not a Rust crate; doesn't belong in `crates/` |
| `crates/corelink-py/` | `tools/sdks/python/` OR stays | Python PyO3 binding; conventionally `tools/` if not consumed by binary builds |
| `crates/corelink-wasm/` | merged into `corelink-worker` OR stays as thin shim crate | wasm shim; investigate during Stream 1 |
| `crates/corelink-cli/` | `tools/cli/` or `apps/cli/` | Operator CLI is an app, not a library crate |
| `crates/corelink-openapi/` | `tools/openapi/` | OpenAPI doc generator; tooling not runtime |
| `crates/corelink-d1-migrations/` | absorbed into `corelink-ops::migrations` submodule | Schema migration runner |

Post-reorg: `crates/` has exactly 19 entries (the 19 target crates). Tools and SDKs live in `tools/`. Apps stay in `apps/`.

## 5. Mega-file decomposition (5 files)

| File | Size | Strategy | Target |
|---|---|---|---|
| `crates/corelink-reapi/src/handler.rs` | 124KB / ~3500 LOC | Split by REAPI service surface | 5 files: `handler/cas.rs`, `handler/ac.rs`, `handler/execution.rs`, `handler/capabilities.rs`, `handler/mod.rs` (trait + dispatch) |
| `apps/server/src/routes/audit_export.rs` | 97KB | Split by pipeline phase | 4 files: `routes/audit_export/stream.rs`, `.../anchor.rs`, `.../payload.rs`, `.../manifest.rs`, `mod.rs` |
| `apps/server/src/routes/audit_analytics.rs` | 76KB | Split by query type | 3-4 files: `routes/audit_analytics/timeseries.rs`, `.../aggregates.rs`, `.../filters.rs`, `mod.rs` |
| `crates/corelink-gc/src/reconcile.rs` | 82KB | Split by reconciliation phase | 3 files: `reconcile/plan.rs`, `reconcile/execute.rs`, `reconcile/verify.rs`, `mod.rs` |
| `crates/corelink-worker/` aggregate (19K LOC distributed) | — | Decompose entire crate per §4 row | Move R2 reader/writer to `corelink-cas`; cache+middleware to `corelink-auth`; reapi adapter to `corelink-reapi`; region resolver to `corelink-replication`; auth to `corelink-auth` |

Per file decomposition: NO behavioural change. The split MUST preserve every public symbol's path via re-exports OR coordinate the rename through dependent crates atomically. Test count MUST stay ≥ pre-split count (no dropped tests).

## 6. Parallelization plan — 3 streams, 4 stages

Stream architecture: orchestrator-direct foundation (Stage 0), 3-stream parallel context work (Stage 1), orchestrator-direct binding consolidation (Stage 2), orchestrator-direct lockdown (Stage 3).

### Stage 0 — Foundation (orchestrator-direct, ~4-8h)

**Why orchestrator-direct:** every other crate depends on these. If two parallel agents touch `corelink-audit` simultaneously they will conflict. Foundation must be stable before parallelization.

**Scope:**
1. Create `corelink-core` (NEW): collect `TenantId`, `Digest`, `Region`, error enums, time clock trait from existing scattered locations.
2. Create `corelink-crypto`: absorb `corelink-hash` + `corelink-client-verify` + `corelink-erasure-attestation`.
3. Create `corelink-telemetry`: absorb 7 telemetry/observability crates per §4.
4. **Restructure `corelink-audit`**: keep crate name; absorb `corelink-audit-chain` + `corelink-analytics`; define `trait AuditEmitter` as the single chokepoint that all Stage 1 streams MUST consume.

**Gates:**
- `cargo build --workspace`: green.
- `cargo test -p corelink-{core,crypto,telemetry,audit}`: green.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `validate_specs.py` + `validate_references.py`: green.
- `cargo deny check`: clean (preserve existing rules).
- TLA+ specs that name audit/crypto invariants: still valid (no spec changes, only Rust restructure).

**SEAL artifact:** `specs/_audits/sealed/2026-05-22-w33-stage0-foundation.md`.

### Stage 1 — 3 parallel agents (~3-5 days wall-clock)

Each agent owns a non-overlapping set of context crates. NO shared writes; each agent's worktree is isolated. The `corelink-audit` chokepoint is consumed but NOT modified by any of the three.

#### Stream A — Data path (agent dispatch template ready)

**Target crates:** `corelink-cas` (10 → 1), `corelink-ac` (3 → 1), `corelink-reapi` (1, split internal), `corelink-gc` (1, split internal).

**Mega-file decomposition included:** `handler.rs` (124KB) + `reconcile.rs` (82KB).

**Inbound deps:** `corelink-core`, `corelink-crypto`, `corelink-audit`, `corelink-telemetry` (all Stage 0).

**Outbound deps consumed by:** `corelink-billing` (quota check), `corelink-replication` (multi-region read), `corelink-ops` (admin tools), `corelink-worker`, `corelink-container`.

**Charter:** preserve every `INV-CAS-*` and `INV-AC-*`; CAS read MUST stay `< 200ms p99` per spec; `INV-CAS-IDEMPOTENCY` and `InvCASImmutability` TLA properties unchanged.

#### Stream B — Policy contexts (agent dispatch template ready)

**Target crates:** `corelink-billing` (14 → 1), `corelink-byok` (7 → 1; microkernel features), `corelink-auth` (6 → 1), `corelink-signup` (1), `corelink-privacy` (11 → 1).

**Mega-file decomposition included:** none (none of Stream B's crates have files > 76KB).

**Inbound deps:** Stage 0 + `corelink-cas` (privacy needs erasure on CAS).

**Outbound deps consumed by:** `corelink-ops` (admin uses auth + billing), `corelink-worker`, `corelink-container`.

**Charter:** preserve `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` (RLS WITH CHECK on every txn); `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`; `subtle::ConstantTimeEq` for token compare; `SecretString` wrap for every cred; BYOK mutual-exclusion at compile time (4 features mutually exclusive).

#### Stream C — Infra + ops (agent dispatch template ready)

**Target crates:** `corelink-replication` (5 → 1), `corelink-ops` (28 → 1; LARGEST absorption), `corelink-adapters-cloud` (assemble from binding portions), `corelink-adapters-vault` (assemble from binding portions).

**Mega-file decomposition included:** none.

**Inbound deps:** Stage 0 + ALL Stream A and B crates (ops references everything via admin tooling).

**Outbound deps consumed by:** `corelink-worker`, `corelink-container`.

**Charter:** `corelink-ops` is large but internally segmented (`oncall`, `statuspage`, `admin`, `dr`, `compliance` modules); enforce module-internal hexagonal shape (`domain/`, `ports/`, `policy/`); no domain logic in adapters-* (translation only).

**Stream C runs AFTER Stream A + B have at least their port definitions stable.** Coordination: orchestrator-direct review at Stream A/B half-way mark before greenlight on Stream C (decision gate Stream A/B midpoint → C start).

### Stage 2 — Binding consolidation (orchestrator-direct, ~1-2 days)

**Scope:**
1. Decompose existing `corelink-worker` (19K LOC) per §4 row; redistribute logic across cas/ac/auth/replication; new thin `corelink-worker` becomes CF Worker (wasm32) entry shell.
2. Create `corelink-container`: absorb `apps/server/src/main.rs` gRPC boot + Cargo bin alias; calls context crates.
3. Finalize `corelink-adapters-cloud` and `corelink-adapters-vault`: confirm every HTTPS client + platform SDK landed; no domain logic.
4. Out-of-tree migrations: move `corelink-go` → `tools/sdks/go/`; `corelink-py` → `tools/sdks/python/`; `corelink-cli` → `tools/cli/`; `corelink-openapi` → `tools/openapi/`; `corelink-d1-migrations` folded into `corelink-ops::migrations`.

**Gates:**
- `cargo build --workspace`: green.
- `cargo build --target wasm32-unknown-unknown -p corelink-worker`: green (wasm32 still compiles).
- `ls -1 crates/ | wc -l` returns `19`.
- Full test suite (`cargo test --workspace`) green; test count ≥ pre-reorg count.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `validate_specs.py` + `validate_references.py` + `check_migrations_additive.py` + `validate_inv_inheritance.py` + `validate_inv_promotion.py`: all green.

**SEAL artifact:** `specs/_audits/2026-05-22-w33-stage2-binding-consolidation.md`.

### Stage 3 — Lockdown (orchestrator-direct, ~4-8h)

**Scope:**
1. Author `deny.toml` rules that enforce:
   - Context crates may NOT depend on `tokio`, `worker`, `reqwest`, `aws-*`, `google-cloud-*`, `azure_*`, `stripe-rust`, any CF SDK.
   - Only `adapters-*`, `worker`, `container` may.
   - `corelink-core` may NOT depend on any other `corelink-*` crate (apex of dep graph).
2. CI script `scripts/validate-architecture.sh` runs cargo-deny + cargo-tree analysis to confirm hex boundaries.
3. Adversarial review by independent Sonnet (cold context) of the entire reorg; SOTA bar ≥8.5/10.
4. Final closure audit `specs/_audits/2026-05-22-w33-closure.md`.
5. Tag `corelink-reorg-v1` on the commit that completes Stage 3.
6. DEBT register update: new top-of-doc entry `> **2026-05-22 update (v1.5.0):** Wave 33 reorg SEALed; 107 → 19 crates; mod-mono + hex + EDA(audit) + actor(DO) + µK(BYOK)`.
7. Memory update: `corelink_wave33_reorg_sealed.md`.

**Gates:**
- `cargo deny check` with new rules: clean.
- Adversarial review report ≥8.5/10.
- All audit docs (Stage 0, 1×3, 2, 3) cross-referenced consistently.

**SEAL artifact:** `specs/_audits/2026-05-22-w33-closure.md`.

## 7. Hard pause triggers

(per `corelink-autonomous-execution-charter` 8-trigger model, scoped to Wave 33)

1. Stage 0 reveals that a foundation crate has hidden async/platform dep that can't be pulled out without exposing a downstream breakage cascade.
2. Stream A loses ≥1 `INV-CAS-*` test that was previously green (CAS hot path regression).
3. Stream B breaks RLS WITH CHECK or token constant-time-compare (security regression).
4. Stream C `corelink-ops` aggregation surfaces ≥3 transitively-shared mutable singletons (refactor cost explodes).
5. Stage 2 `corelink-worker` decomposition reveals hidden coupling that requires Stream A/B re-work (architecturally invalid plan).
6. Wasm32 build breaks during Stage 2 and root cause requires changing wave-26 `getrandom_backend="wasm_js"` fix.
7. `cargo deny check` with new rules surfaces ≥5 unfixable violations (architecture target is wrong for current code).
8. Stage 3 adversarial review scores < 7.5/10 (SOTA bar miss).

On any trigger: HALT the active stream/stage, document in the relevant audit doc §7, escalate to Owner, do NOT auto-recover. Charter rule: rollback per-stream is per-merge revert; per-stage rollback is the previous SEAL tag.

## 8. Decision gates (Owner approval required)

- **Pre-Stage-0:** this spec approved (current step; user greenlit 2026-05-22).
- **Stage 0 → Stage 1:** after foundation lands, Owner reviews trait surface of `corelink-audit` (most load-bearing change).
- **Stream A/B mid-point → Stream C:** after Stream A and B have stable port definitions, Owner approves Stream C dispatch (which depends on those ports).
- **Stage 1 → Stage 2:** after all 3 streams merge, Owner approves the binding consolidation (irreversible cargo-deny lockdown comes next).
- **Stage 2 → Stage 3:** after 19-crate workspace builds clean, Owner approves cargo-deny lockdown + adversarial review.

## 9. Cross-references

- Parent: this is its own wave; sibling to `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` (Wave 32 paused until Wave 33 SEALs).
- Research source: agent-delivered architectural pattern survey + decision matrix (2026-05-22; full report archived in conversation transcript).
- Charter: `specs/03_architecture/invariant_registry.md` (198 INVs, 61 CRITICAL TLA-verified, must remain TLA-proved post-reorg).
- Previous waves: 18-30 (engineering completion), 31 (DEBT-029-cas + wallet broker dual-mode), 32 Phase A (BetterStack live).
- Memory: `[[corelink-wave32-prod-deploy]]`, `[[corelink-autonomous-execution-charter]]`, `[[feedback-synchronous-agents]]`.

## 10. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Wave 33 code reorganization spec.**
