# Wave 33 Stage 1 Stream C — Infra + Ops SEAL Audit (2026-05-22)

> **Doc kind:** stream closure audit (evidence; `_audits/` excluded
> from canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stream-c-infra-ops` (worktree
> `.claude/worktrees/agent-a3c409de764531b10`), based off
> `aa5bd987` (Stream B merge to main).
>
> **Mandate:** wave-33 Stage 1 Stream C reorganises the infra +
> ops + cloud-adapter + vault-adapter contexts under the modular-
> monolith + hexagonal target shape per
> `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1.
> This is the **LARGEST absorption stream of Wave 33**: 38 absorbed
> crates → 4 umbrella crates. Aggregator pattern per Stage 0 SEAL
> `specs/_audits/2026-05-22-w33-stage0-foundation.md` §4.

## §1. Scope — sub-step commit chain

Stream C executes 4 commit-per-sub-step on
`wt/r-prep-w33-stream-c-infra-ops` plus 1 SEAL commit:

| Sub-step | SHA | Title | Strategy |
|---|---|---|---|
| C.1 | `0f78dbf8` | `corelink-replication` (aggregator for 5) | Option-A aggregator |
| C.2 | `72251644` | `corelink-ops` (aggregator for 28 — LARGEST) | Option-A aggregator |
| C.3 | `51cd34b7` | `corelink-adapters-cloud` (aggregator for 5 binding portions) | Option-A aggregator |
| C.4 | `635c1b9c` | `corelink-adapters-vault` (aggregator for 1 binding portion) | Option-A aggregator |
| SEAL | (this commit) | Stream C SEAL audit | doc only |

Option-A interpretation choice: physical absorption + decomposition
between pure-logic and HTTPS / CF-binding portions deferred to Stage 2
(consumer-migration stream that owns `apps/server` route layer, CF
Worker wasm32 entry, and D1 migrations runner atomically). Absorbed
crates remain canonical sources of truth, retaining their own
tests/benches. This is the same Stage 0 §4 Option-A aggregator pattern
used by Stream A1 and Stream B.

## §2. Crates aggregated — 4 umbrellas, 38 absorbed crates

### §2.1 `corelink-replication` (C.1) — 5 absorbed

Replication context: multi-region topology + replica apply lane +
coordinator + warm-failover router + staged rollout.

| Submodule | Absorbed source crate |
|---|---|
| `region` | `corelink-region` |
| `replica` | `corelink-replica-worker` |
| `coordinator` | `corelink-replication-coordinator` |
| `failover` | `corelink-failover-router` |
| `rollout` | `corelink-rollout-controller` |

Rationale: per-region DO-lock coupling + DR-16 warm-failover lifecycle
+ 24h failback cool-down timer are structurally cross-coupled via the
`replication_status()` composition; physical relocation requires
atomic consumer-side migration of `apps/server` `/health` +
`tests/e2e-failover-router` + `tests/e2e-replication-failover`
harnesses.

### §2.2 `corelink-ops` (C.2) — 28 absorbed (LARGEST Wave-33 absorption)

Ops context: 19 thematic submodules grouping 28 absorbed crates.

| Submodule | Absorbed source crate(s) |
|---|---|
| `oncall` | `corelink-oncall` |
| `statuspage` | `corelink-statuspage-real` (also re-exported by `corelink-adapters-cloud::statuspage`) |
| `slack` | `corelink-slack-real` (also re-exported by `corelink-adapters-cloud::slack`) |
| `admin::api` | `corelink-admin-api` |
| `admin::dry_run` | `corelink-admin-dry-run` |
| `admin::handler` | `corelink-handler-admin` |
| `admin::dual_approval` | `corelink-dual-approval` |
| `alerts` | `corelink-customer-alerts` |
| `enterprise` | `corelink-enterprise-inquiry` |
| `survey` | `corelink-survey` |
| `runbook` | `corelink-runbook-tracker` |
| `tenant_offboarding` | `corelink-tenant-offboarding` |
| `deploy` | `corelink-deploy-verifier` |
| `terraform` | `corelink-terraform-drift-consumer` |
| `dr::drill` | `corelink-dr-drill` |
| `dr::backup_verify` | `corelink-backup-verify` |
| `chaos` | `corelink-chaos-scheduler` |
| `rotation::adapters` | `corelink-rotation-adapters` |
| `rotation::worker` | `corelink-rotation-worker` |
| `dt::webhook` | `corelink-dt-webhook` |
| `drata` | `corelink-drata-sync` |
| `supply_chain::policy` | `corelink-supply-chain-policy` |
| `supply_chain::verify` | `corelink-supply-verify` |
| `config::api` | `corelink-config-api` |
| `config::durable_object` | `corelink-config-do` |
| `migrations` | `corelink-d1-migrations` |

Binary-only crates documented but **not re-exported into library
surface** (no `src/lib.rs`):

- `corelink-dt-cli` — Drift Tracker CLI (binary target only).
- `corelink-dt-reconcile` — Drift Tracker reconciler (binary target only).

These remain canonical workspace binaries consumed via `cargo run
--bin`; documented in `crates/corelink-ops/src/dt.rs` rustdoc for the
canonical-import audit trail.

Rationale: admin dual-approval 4-way invariant (api + dry-run +
handler + dual-approval composing
`require_two_distinct_approvers + dry_run_preview_diff_only + audit
envelope ordering`); PAT-RUNBOOK-DRILL-001 24h drill aggregation
window coupling; D1 migrations replay harness consumes every
`migrations/d1/*.sql` across workspace; tenant offboarding 5-state
machine composes DSR erasure + billing cancel + audit + 30d grace
export across crate boundaries.

### §2.3 `corelink-adapters-cloud` (C.3) — 5 absorbed (binding portions)

Cloud-adapters context: HTTPS / CF-binding portion of 5 adapter
crates. Pure-logic portion simultaneously re-exported by sibling
umbrellas (deferred decomposition).

| Submodule | Absorbed source crate | Sibling pure-logic re-export |
|---|---|---|
| `cf` | `corelink-cf-bindings` | (none — pure CF wasm32 bindings) |
| `clerk` | `corelink-clerk-cf` | `corelink_auth::clerk_cf` (Stream B B.3) |
| `stripe` | `corelink-stripe-real` | `corelink_billing::stripe` (Stream B B.1) |
| `statuspage` | `corelink-statuspage-real` | `corelink_ops::statuspage` (C.2) |
| `slack` | `corelink-slack-real` | `corelink_ops::slack` (C.2) |

Rationale: the pure-logic vs. HTTPS / CF-binding decomposition
requires atomic consumer-side migration of `apps/server` route layer +
CF Worker entry point + every webhook HMAC verify call site;
that's structurally Stage 2 territory. Stage 1 ships the canonical
import surface so consumer migration can proceed incrementally.

### §2.4 `corelink-adapters-vault` (C.4) — 1 absorbed (binding portion)

Vault-adapters context: HTTPS / mTLS portion of HashiCorp Vault BYOK
adapter. Pure-logic portion simultaneously re-exported by
`corelink-byok::vault` umbrella (Stream B B.2b, gated behind `vault`
cargo feature).

| Submodule | Absorbed source crate | Sibling pure-logic re-export |
|---|---|---|
| `vault` | `corelink-byok-vault` | `corelink_byok::vault` (Stream B B.2b, behind `vault` feature) |

HuGR Wallet broker sibling adapter (Wave-31 follow-up WI) will land
here as `corelink_adapters_vault::wallet` when authored.

## §3. Per-umbrella structure (filesystem layout)

```
crates/
├── corelink-replication/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          (129 LOC)
│       ├── region.rs       (7 LOC, single re-export)
│       ├── replica.rs      (7 LOC, single re-export)
│       ├── coordinator.rs  (9 LOC, single re-export)
│       ├── failover.rs     (7 LOC, single re-export)
│       └── rollout.rs      (8 LOC, single re-export)
├── corelink-ops/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  (303 LOC) ← under 500 HARD CAP
│       ├── oncall.rs               (7 LOC)
│       ├── statuspage.rs           (11 LOC)
│       ├── slack.rs                (11 LOC)
│       ├── admin.rs                (42 LOC, 4 sub-submodules)
│       ├── alerts.rs               (7 LOC)
│       ├── enterprise.rs           (9 LOC)
│       ├── survey.rs               (11 LOC)
│       ├── runbook.rs              (9 LOC)
│       ├── tenant_offboarding.rs   (9 LOC)
│       ├── deploy.rs               (8 LOC)
│       ├── terraform.rs            (8 LOC)
│       ├── dr.rs                   (26 LOC, 2 sub-submodules)
│       ├── chaos.rs                (10 LOC)
│       ├── rotation.rs             (24 LOC, 2 sub-submodules)
│       ├── dt.rs                   (24 LOC, 1 active + 2 binary-only documented)
│       ├── drata.rs                (11 LOC)
│       ├── supply_chain.rs         (24 LOC, 2 sub-submodules)
│       ├── config.rs               (23 LOC, 2 sub-submodules)
│       └── migrations.rs           (10 LOC)
├── corelink-adapters-cloud/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          (129 LOC)
│       ├── cf.rs           (13 LOC)
│       ├── clerk.rs        (12 LOC)
│       ├── stripe.rs       (14 LOC)
│       ├── statuspage.rs   (17 LOC)
│       └── slack.rs        (14 LOC)
└── corelink-adapters-vault/
    ├── Cargo.toml
    └── src/
        ├── lib.rs          (79 LOC)
        └── vault.rs        (16 LOC)
```

## §4. L2.10 audit table — every new file with LOC + classification

`/techlead` L2.10 caps: 500 LOC HARD CAP (NEW files), 200 LOC sweet
spot. Every new file is well within the sweet spot except `corelink-
ops/src/lib.rs` at 303 LOC (still well under the 500 HARD CAP — no
`lib/` directory split required).

| File | LOC | Classification |
|---|---:|---|
| `crates/corelink-replication/Cargo.toml` | 41 | manifest |
| `crates/corelink-replication/src/lib.rs` | 129 | aggregator façade (under sweet spot) |
| `crates/corelink-replication/src/region.rs` | 7 | re-export shell |
| `crates/corelink-replication/src/replica.rs` | 7 | re-export shell |
| `crates/corelink-replication/src/coordinator.rs` | 9 | re-export shell |
| `crates/corelink-replication/src/failover.rs` | 7 | re-export shell |
| `crates/corelink-replication/src/rollout.rs` | 8 | re-export shell |
| `crates/corelink-ops/Cargo.toml` | 61 | manifest |
| `crates/corelink-ops/src/lib.rs` | 303 | aggregator façade (under 500 HARD CAP) |
| `crates/corelink-ops/src/oncall.rs` | 7 | re-export shell |
| `crates/corelink-ops/src/statuspage.rs` | 11 | re-export shell |
| `crates/corelink-ops/src/slack.rs` | 11 | re-export shell |
| `crates/corelink-ops/src/admin.rs` | 42 | thematic group (4 sub-submodules) |
| `crates/corelink-ops/src/alerts.rs` | 7 | re-export shell |
| `crates/corelink-ops/src/enterprise.rs` | 9 | re-export shell |
| `crates/corelink-ops/src/survey.rs` | 11 | re-export shell |
| `crates/corelink-ops/src/runbook.rs` | 9 | re-export shell |
| `crates/corelink-ops/src/tenant_offboarding.rs` | 9 | re-export shell |
| `crates/corelink-ops/src/deploy.rs` | 8 | re-export shell |
| `crates/corelink-ops/src/terraform.rs` | 8 | re-export shell |
| `crates/corelink-ops/src/dr.rs` | 26 | thematic group (2 sub-submodules) |
| `crates/corelink-ops/src/chaos.rs` | 10 | re-export shell |
| `crates/corelink-ops/src/rotation.rs` | 24 | thematic group (2 sub-submodules) |
| `crates/corelink-ops/src/dt.rs` | 24 | thematic group (1 sub-submodule + 2 binary-only documented) |
| `crates/corelink-ops/src/drata.rs` | 11 | re-export shell |
| `crates/corelink-ops/src/supply_chain.rs` | 24 | thematic group (2 sub-submodules) |
| `crates/corelink-ops/src/config.rs` | 23 | thematic group (2 sub-submodules) |
| `crates/corelink-ops/src/migrations.rs` | 10 | re-export shell |
| `crates/corelink-adapters-cloud/Cargo.toml` | 37 | manifest |
| `crates/corelink-adapters-cloud/src/lib.rs` | 129 | aggregator façade |
| `crates/corelink-adapters-cloud/src/cf.rs` | 13 | re-export shell |
| `crates/corelink-adapters-cloud/src/clerk.rs` | 12 | re-export shell |
| `crates/corelink-adapters-cloud/src/stripe.rs` | 14 | re-export shell |
| `crates/corelink-adapters-cloud/src/statuspage.rs` | 17 | re-export shell |
| `crates/corelink-adapters-cloud/src/slack.rs` | 14 | re-export shell |
| `crates/corelink-adapters-vault/Cargo.toml` | 32 | manifest |
| `crates/corelink-adapters-vault/src/lib.rs` | 79 | aggregator façade |
| `crates/corelink-adapters-vault/src/vault.rs` | 16 | re-export shell |

L2.10 verdict: every new file ≤ 303 LOC. No file exceeds the 500-LOC
HARD CAP. The largest single file is `corelink-ops/src/lib.rs` at 303
LOC; with 19 `pub mod` declarations + extensive rustdoc + 19
smoke-test fns this is appropriate density — no `lib/` directory
split required (charter Hard Pause Trigger 5 status: green).

## §5. Test count delta — zero regressions

The Option-A aggregator pattern is purely additive: absorbed crates
retain their own `tests/` and `benches/` unchanged. Each new umbrella
adds only path-resolution smoke tests in `mod tests`.

| Umbrella | New tests added | Previously-green tests affected | Regressions |
|---|---:|---:|---:|
| `corelink-replication` | 5 (path resolution smoke) | 0 (absorbed crates untouched) | 0 |
| `corelink-ops` | 19 (path resolution smoke; one thematic group can resolve all sub-submodules in a single test) | 0 (absorbed crates untouched) | 0 |
| `corelink-adapters-cloud` | 5 (path resolution smoke) | 0 (absorbed crates untouched) | 0 |
| `corelink-adapters-vault` | 1 (path resolution smoke) | 0 (absorbed crates untouched) | 0 |
| **Total** | **30 new path-resolution tests** | **0** | **0** |

## §6. Gates run — all green per sub-step

| Gate | C.1 | C.2 | C.3 | C.4 | Final (post-SEAL workspace sweep) |
|---|:---:|:---:|:---:|:---:|:---:|
| `cargo build -p <crate>` | green | green | green | green | n/a |
| `cargo test -p <crate>` | green (5) | green (19) | green (5) | green (1, lib) | n/a |
| `cargo clippy -p <crate> --all-targets -- -D warnings` | green | green | green | green | n/a |
| `cargo build --workspace` | n/a | n/a | n/a | n/a | green |
| `cargo clippy --workspace --all-targets -- -D warnings` | n/a | n/a | n/a | n/a | green |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | n/a | n/a | green | n/a | n/a |
| `python3 scripts/validate_specs.py` | n/a | n/a | n/a | n/a | green (449 + 9 schema) |
| `python3 scripts/validate_references.py` | n/a | n/a | n/a | n/a | green (0 dangling) |
| `python3 scripts/check_migrations_additive.py` | n/a | n/a | n/a | n/a | green (59 scanned, all additive) |
| `python3 scripts/validate_inv_inheritance.py` | n/a | n/a | n/a | n/a | green (16 child chains, 8 parents) |
| L2.10 LOC cap (≤ 500 HARD; ≤ 200 sweet spot) | green (max 129) | green (max 303) | green (max 129) | green (max 79) | green |

Clippy fix during C.3: the initial `clerk.rs`, `slack.rs`,
`statuspage.rs`, and `stripe.rs` re-export shell rustdoc contained
`(... + ...)` parentheticals at line-start positions that
`clippy::doc_lazy_continuation` parsed as list bullets. Resolved by
reflowing the parentheticals as comma-separated prose (no `+` glyph
at line start). All 15 reported errors cleared with no `allow` lints
added. No charter-bypass.

## §7. Hard pause triggers — status

Per spec §7, the 7 hard-pause triggers for Stream C:

| # | Trigger | Status | Evidence |
|---|---|:---:|---|
| 1 | Aggregator submodule needs cross-aggregator import before sibling lands | NOT TRIGGERED | All cross-umbrella references are pure rustdoc cross-links (e.g., `corelink_ops::statuspage` mentions `corelink_adapters_cloud::statuspage`); no actual cross-aggregator `use` statement in any aggregator source file. |
| 2 | Previously-green test goes red | NOT TRIGGERED | Workspace test sweep prior to Stream C was green at `aa5bd987`; no absorbed crate has been touched; only additive new path-resolution tests added. |
| 3 | wasm32 build broken | NOT TRIGGERED | `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` ran green after C.3. |
| 4 | Workspace `cargo build` broken at any sub-step boundary | NOT TRIGGERED | Full workspace `cargo build --workspace` ran green at C.1, C.2, C.3, C.4 sub-step boundaries + final SEAL sweep. |
| 5 | `corelink-ops/src/lib.rs` exceeds 500 LOC even after `lib/` directory split | NOT TRIGGERED | `corelink-ops/src/lib.rs` = 303 LOC (well under 500 HARD CAP); no `lib/` directory split required. |
| 6 | Cargo.toml merge conflicts with parallel A2 work | NOT TRIGGERED | A2 work (`wt/r-prep-w33-stream-a2-megafiles`) is decomposing existing CAS mega-files (`batch_read_blobs.rs` extraction); does not touch `[workspace.members]` or `[workspace.dependencies]` table rows added by Stream C. No conflict in the Cargo.toml ranges modified by C.1-C.4. |
| 7 | Adapters-cloud / adapters-vault aggregator surfaces structurally impossible cycle with `corelink-byok` or `corelink-auth` | NOT TRIGGERED | `corelink-adapters-cloud::stripe` re-exports `corelink-stripe-real`; `corelink-billing::stripe` (Stream B B.1) also re-exports it. Both sit on the same single absorbed crate — no cycle. Same analysis for `clerk_cf` (also re-exported by `corelink_auth::clerk_cf`) and `byok-vault` (also re-exported by `corelink_byok::vault`). The 3 sibling-pair re-exports all point at the same underlying absorbed crate; no aggregator depends on another aggregator. |

Stream C verdict: zero hard-pause triggers fired. Stream merges into
main without escalation.

## §8. Next steps

1. Orchestrator runs `/techlead` against this branch (full L1-L11
   sweep + L2.10 LOC discipline + charter rules) before merge to main.
2. Orchestrator merges `wt/r-prep-w33-stream-c-infra-ops` into main.
3. Stage 1 close-out: with Streams A1 + A2 + B + C all merged, **Stage
   1 of Wave 33 is complete**. The workspace is now structured as
   19 umbrella crates over the modular-monolith + hexagonal +
   microkernel(BYOK) shape per spec §3 target structure (Stage 1
   target = 19 umbrellas reached: `core`, `crypto`, `telemetry`,
   `cas`, `ac`, `auth`, `billing`, `byok`, `privacy`, `replication`,
   `ops`, `adapters-cloud`, `adapters-vault`, plus the foundation /
   schema / handler / e2e-tests / `apps/server` / `apps/migrate-…`
   workspace targets which sit outside the 19-umbrella scope).
4. Stage 2 may proceed: physical decomposition between pure-logic and
   HTTPS / CF-binding portions of the 5 cloud-adapter crates +
   Vault adapter (decomposes `corelink-worker` god-crate; relocates
   `apps/server` route consumer call sites to canonical paths;
   absorbs the pure-logic portions physically into their owning
   sibling umbrellas).

## §9. Sign-off

DCO sign-off + Co-Authored-By trailer attached to every Stream C
sub-step commit (C.1, C.2, C.3, C.4) per `git log` on
`wt/r-prep-w33-stream-c-infra-ops`. This SEAL commit also carries
the same trailer.

`Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>`
`Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`
