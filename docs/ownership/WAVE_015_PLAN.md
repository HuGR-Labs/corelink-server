# Ownership wave 015 — remaining harnesses and migration tool

Source/static-only ownership documentation. OKF remains the designated
canonical routing reference; do not duplicate, revalidate or redefine it.
Descriptions in Cargo manifests are intent, not proof that harnesses,
providers, public APIs or migration modes executed.

## Fixed baseline and disjoint ownership

Source/evidence snapshot: re-anchored `main` at
`1177dad2ca2a9f21c29b5a118aa7944b77147798` (observed 2026-09-21). The
package trees in this plan and the root `Cargo.toml` were compared against the
older planning snapshot `1be2cfcd82783b93cece2668717fe4ad78600752`; those
paths had no content diff, although the commit histories are not ancestor
related. Re-read `main` immediately before dispatch and repeat that scoped
comparison; if any assigned manifest/source or workspace membership changes,
reconcile and update the source evidence pin before authoring. The integration
branch baseline is a separate exact SHA supplied in each dispatch packet.
Authors own only the four assigned package documents. No source, test,
manifest, CI, standard, registry, index, rollout/status or other package path
may change. Lead owns the census, integration and review.

| Package | Manifest | Profile | Static starting boundary |
|---|---|---|---|
| `e2e-replication-failover` | `tests/e2e-replication-failover/Cargo.toml` | S | library plus one `scenarios` test target; first-party deps `corelink-replication-coordinator`, `corelink-replica-worker` |
| `e2e-resilience` | `tests/e2e-resilience/Cargo.toml` | S | library plus one `scenarios` test target; first-party deps `corelink-ratelimit`, `corelink-rate-headers` |
| `e2e-signup-flow` | `tests/e2e-signup-flow/Cargo.toml` | S | library plus 6 declared test targets; first-party deps signup, DPA acceptance, tier selection, Stripe adapter and hash |
| `e2e-tenant-isolation` | `tests/e2e-tenant-isolation/Cargo.toml` | S | library plus explicit `adversarial` test target; `autotests = false`; first-party deps tenant-path, audit and BYOK |
| `e2e-user-journeys` | `tests/e2e-user-journeys/Cargo.toml` | H | one binary and 23 modules exported by `src/journeys/mod.rs`; no first-party Cargo dependency is declared |
| `migrate-single-to-multi-region` | `apps/migrate-single-to-multi-region/Cargo.toml` | S | one binary; first-party dependency `corelink-region`; source advertises dry-run/execute/rollback modes that must not be invoked |

The high profile for `e2e-user-journeys` is justified by 23 semantic journey
modules at the pinned `src/journeys/mod.rs` (the earlier seed count of 22 was
one short), above the candidate H threshold of 20. Its “black-box” package
description and public-API names do not prove a deployment or an actual HTTP
request. Other profiles remain S unless source census shows a candidate H
threshold; do not omit relations to fit a cap.

Harnesses must distinguish their fakes, direct imports, CLI/binary process
boundaries, gated/ignored live tests and actual composition. The
`e2e-signup-flow` manifest declares a Stripe test-mode ignored path; do not
read or execute secrets/provider configuration. The migration binary is
operationally sensitive: inspect source and document stop/escalation boundaries
only; never call any mode or touch tenant/storage/Terraform state.

## Deliverables and acceptance

Each package owns exactly:

1. `.claude/skills/own-<package>/SKILL.md`, sections S01–S07.
2. `docs/ownership/crates/<package>/REFERENCE.md`, R01–R08.
3. `docs/ownership/crates/<package>/BLAST_RADIUS.md`, B01–B06.
4. `docs/ownership/crates/<package>/MAINTENANCE.md`, M01–M06.

Success criteria: maintainers can route package work, locate contracts and
invariants, understand boundaries between harness and production code, and
select a safe documentary procedure.

Completeness criteria: exact Cargo identity, targets/features, semantic module
map, contracts and falsifiable invariants, every declared first-party edge,
source imports/composition and material external/CI/process boundaries are
covered; unknown reachability and execution facts remain explicit.

Quality standards: every material claim cites a source path/anchor, every
relationship has one producer and consumer and a bounded activation/failure
mechanism, navigation is usable, cross-package implementation owner remains
distinct from the API facade, and the declared profile's size caps hold.

Definition of Done: exact four-path diff, all required sections and five
axioms, per-declaration manifest records, four profile checks and
`git diff --check`, independent cold `APPROVE` per artifact, then lead scope
verification and integration. Checker output is structural evidence only.

Invariants: `[package].name` is authoritative; test declaration is not test
execution; a dependency/import is not runtime reachability; fake behavior is
not provider behavior; manifest-advertised migration modes are not permission
to execute; no database/storage/customer mutation is in author scope; any
changed artifact bytes invalidate that artifact's review.

## Exact v1.3 candidate contract for authors and reviewers

The supplied archive is `/Users/gustavoschneiter/Downloads/corelink-ownership-v1.3.zip`, SHA-256
`6c5cb12d53f6042775e8284de45a27865f872f0955cff1fc619a0c0ac8973197`. It is
the current candidate input, **not a frozen or repository-integrated
standard**. Authors and reviewers must use the extracted archive's exact files:

| Contract input | SHA-256 |
|---|---|
| `STANDARD.md` | `f9dad29060135b993b45a427078865ef3d0f7141437d44813b7802e415046b4a` |
| `COMMON.md` | `2519e39ee38cc59f727f8750599fa0fc1d292872a0838cf90548d28511166df2` |
| `templates/MAINTENANCE.md.tmpl` | `9bdb6fc1448e64f11531a1c3fc58a850ad3d24d0647cc807729a869da337dfec` |
| `templates/COLD_REVIEW.md` | `fb94221f6b44b6626990b25d9b1d252e1ff797b436b4b49f7df1ae742127c912` |
| `schemas/package-record.schema.json` | `5289e9332d800fe51333ffcca5076a5d1a0f34a45a7a4790110d8afccdd75a15` |

The lead supplies the controlled extraction path in each dispatch packet; do
not guess it, use an older embedded copy, or hardcode a machine-specific path
as permanent package policy. Each `REL` is one producer→consumer boundary with
its own surface, activation, contract, state/effect, failure propagation,
boundary, validation, coordination, and evidence; split records when any of
those facts differ. Keep dependency, data-flow and impact-propagation
directions distinct, and search outside Cargo before recording unknowns.

Each `PROC-###` must have objective/trigger, preconditions, canonical mode,
environment/permissions, inputs, bounded steps/commands, observable expected
predicate, stop/failure, recovery, evidence, `required_for_acceptance`, and
separate schema-vocabulary fields for review status, execution status, and
result. Include an M04 change-type matrix with package/target/features,
command, environment and expected predicate; document relevant commands but
do not run Cargo in this wave. M05 must cover compatibility and realistic
recovery/rollback, or state and route the package's actual composition-root
recovery boundary. M06 records baseline/result, cleans task-created temporary
files, reconciles affected docs/backlog, and routes changed hashes to fresh
cold review.

Cold reviewers receive an immutable candidate and pinned source, not the
author's rationale or previous verdicts. Return four separate verdicts. The
skill challenge includes three activating tasks, two non-activating tasks and
two out-of-authority refusals; the reference challenge follows entrypoint,
contracts, state/failure and critical invariant enforcers; the blast challenge
repeats direct/inverse census and searches non-Cargo paths; maintenance checks
procedure fields, exact static gates, stop/recovery paths and honest execution
states. No structural checker result substitutes for these challenges.

## Execution limits

Authors use static manifests and source only. Never run Cargo, test/fuzz
targets, package binaries, HTTP clients, network, providers, GitHub, deploy,
production, database, storage, Terraform or migration modes. For local doc
checks, use a configurable checker path and label the supplied candidate as
unintegrated; do not hardcode a temporary extraction path as permanent policy.
Any blocked gate is reported directly, never bypassed.

## Dispatch log

| Package | Source pin re-read | Integration baseline | Author worktree/branch | State |
|---|---|---|---|---|
| `e2e-replication-failover` | `main` still `1177dad2ca2a9f21c29b5a118aa7944b77147798`; assigned source, manifest and root workspace manifest unchanged from the source pin | `527234c0a316e7f01615a7549e1797da355d69c5` | `/tmp/corelink-ownership-w015-e2e-replication-failover`, `codex/w015-e2e-replication-failover` | Candidate `d7edc8db316d576c3888cac4071385654480104d`; four checks pass, independent review pending; peer REL reconciliation, escalation routes, Cargo resolution, CI selection, and current tests remain unknown |
| `e2e-resilience` | `main` still `1177dad2ca2a9f21c29b5a118aa7944b77147798`; assigned source, manifest and root workspace manifest unchanged from the source pin | `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540` | `/tmp/corelink-ownership-w015-e2e-resilience`, `codex/w015-e2e-resilience` | Candidate `b8a0d858d896cba6e9f3da9de2f857529cd9dfb2`; four S checks and scope gates pass; first reviewer was contaminated by opening unrelated REVIEW-INPUT, so its verdict is discarded and fresh independent review is required |
| `e2e-signup-flow` | same source pin; assigned source, manifest and root workspace manifest unchanged | `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540` | `/tmp/corelink-ownership-w015-e2e-signup-flow`, `codex/w015-e2e-signup-flow` | Candidate `be26fbf47d05f197a3104da699eb6ed1243a234c`; four checks pass, cold review pending; static scan stopped at ignored Stripe-test references, with no config/secrets/runtime inspected |
| `e2e-tenant-isolation` | same source pin; `origin/main` re-read by lead as 1177; local branch `main=cdd6…` is divergent and noncanonical; assigned source/manifest match the pinned tree | `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540` | `/tmp/corelink-ownership-w015-e2e-tenant-isolation`, `codex/w015-e2e-tenant-isolation` | Candidate `91738f2860e5e2f75b87da7a9c816287d10382ad`; four S checks pass; contradictory verifier expectations versus `adversarial_tail.rs` documented UNKNOWN/unresolved; cold review pending |
| `e2e-user-journeys` | same source pin; assigned source/manifest/root workspace manifest byte-identical to integration tree; source count verified 23 | `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540` | `/tmp/corelink-ownership-w015-e2e-user-journeys`, `codex/w015-e2e-user-journeys` | Candidate `dad7d1dde`; four H checks and scoped diff-check pass. 23 source dispatches/functions vs plan's 22 remains recorded discrepancy; cold review pending |
| `migrate-single-to-multi-region` | same source pin; assigned source, manifest and root workspace manifest unchanged | `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540` | `/tmp/corelink-ownership-w015-migrate-single-to-multi-region`, `codex/w015-migrate-single-to-multi-region` | Candidate `6d42d6bae965fb9a4c62e5b6d9055618370ef654`; four S checks and source scope pass per author; migration modes explicitly never executed; cold review pending |
