---
name: own-corelink-runner-aggregate
description: >-
  Own source-defined runner usage aggregation, per-region hash-link output, and
  shadow-ledger contracts in corelink-runner-aggregate. Use for static contract
  changes; do not use as authority to operate runners, billing, storage, or workflows.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-runner-aggregate"
  manifest: "crates/corelink-runner-aggregate/Cargo.toml"
  source-commit: "cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6"
  evidence-set: "runner-aggregate-main-readback-20260923"
---

# Ownership — corelink-runner-aggregate

Candidate ownership for the package's static aggregate and binary contracts. It is not an approval, observed-runner result, or authority to operate billing, D1, a workflow, or any provider.

[Trigger](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) · [Flow](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Use this skill when | Route elsewhere when |
|---|---|
| Changing `aggregate_runner_usage`, its public input/output/error shapes, or aggregate ordering | Changing `HashChainBuilder`, `AggregatedCounter`, or emitted event types in their defining packages |
| Changing the manifest-declared `runner-aggregate-run` target or its source JSON shell | Establishing a workflow, D1 read/write, watermark, credential, Stripe, or runner result |
| Reviewing source-visible shadow-charge or chain-output compatibility | Resolving reverse consumers, deployment, or actual invocation |

<a id="s02"></a>
## S02 — Territory and authority

The territory is `crates/corelink-runner-aggregate/Cargo.toml`, `src/lib.rs`, and the declared `src/bin/runner-aggregate-run.rs`. The manifest declares direct dependencies on `corelink-billing-emit`, `corelink-billing-aggregator`, and `corelink-runner-overage`; this package consumes their contracts rather than owning them.

This skill grants no authority to operate a runner, charge a tenant, contact Stripe, read or write D1, set a watermark, use credentials, deploy a binary, or assert that the target runs. Maintainers, review authority, and all reverse consumers are unknown from the scoped static evidence.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| What is the bounded source surface? | [R01–R03](../../../docs/ownership/crates/corelink-runner-aggregate/REFERENCE.md#r01) |
| Which exported shapes and predicates matter? | [R04–R07](../../../docs/ownership/crates/corelink-runner-aggregate/REFERENCE.md#r04) |
| Which source edges can a change affect? | [B02–B05](../../../docs/ownership/crates/corelink-runner-aggregate/BLAST_RADIUS.md#b02) |
| How is a static change checked and handed off? | [M01–M06](../../../docs/ownership/crates/corelink-runner-aggregate/MAINTENANCE.md#m01) |

<a id="s04"></a>
## S04 — Decision form

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Version, immutable terms, or prior-consumption binding changes | Trace API-001/002, INV-001/002, and REL-010 | Validation branches and snapshot digest recipe in source | Terms authority or external snapshot producer must be established |
| Deduplication, grouping, or row fields change | Trace INV-003/004 and REL-006/011 | Key-set scope and checked group accumulation | Cross-run/store idempotency is asserted |
| Prior-head, sequence, or digest mapping changes | Trace INV-005 and REL-002/012 | Builder call arguments and pre/post output mapping | The defining billing-aggregator contract must change |
| Cumulative shadow math or terms change | Trace INV-006 and REL-003/013 | Prior plus batch formula and immutable terms calls | A charge, entitlement, or Stripe behavior is asserted |
| Binary target/shell changes | Trace API-006, R06, and REL-014/015 as static target/API compatibility | Manifest declaration, bin source, and integration-test source | Invocation, scheduling, or runner operation must be proved |
| A production claim is requested | Request the appropriate operational evidence | Evidence class outside this packet | SOURCE evidence is the only evidence available |

<a id="s05"></a>
## S05 — Work flow

1. Confirm the fixed baseline and package manifest.
2. Read the affected R, B, and M records before editing.
3. State the source predicate and separate imported, local, and unknown behavior.
4. Preserve deterministic ordering and typed error/output boundaries unless deliberately revised.
5. Run only the four documentary checks and `git diff --check` when authorized.
6. Hand off unresolved consumers or operational evidence rather than inferring it.

<a id="s06"></a>
## S06 — Stop conditions

Stop when the request needs a real runner result, workflow schedule, D1 transaction, staged-event completeness, watermark enforcement, entitlement provenance, Stripe behavior, credential use, deployment, or reverse-consumer compatibility. Do not promote manifest prose, comments, a binary declaration, static imports, or source tests into proof of any of those facts.

<a id="s07"></a>
## S07 — Handoff and definition of done

Report baseline, exact changed paths, affected API/invariant/relation IDs, evidence class, checks and exit status, skipped checks, and unknowns. Success is a source-consistent bounded packet with no operational claim. Completeness requires this skill, R01–R08, B01–B06, M01–M06, four S-profile structural checks, and a scope-only whitespace-clean diff. Author checks are not cold review.

[Back to start](#s01)
