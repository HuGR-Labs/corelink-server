---
name: own-corelink-dt-reconcile
description: >-
  Own the static daily Dependency-Track reconciliation binary surface in
  corelink-dt-reconcile. Use for source-contract changes only; never use it to
  claim reconciliation, delivery, provider state, scheduling, or deployment.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-dt-reconcile"
  manifest: "tools/dt-reconcile/Cargo.toml"
  source-commit: "398e586ccef712477f2a4ce51e026443b67e5747"
  evidence-set: "w013-dt-reconcile-static-source-20260921"
---

# Ownership — corelink-dt-reconcile

Candidate ownership of a manifest-declared binary and its inspected source only. It is not evidence that a reconciliation ran, that data changed, that any provider is reachable, or that an alert was delivered.

[Trigger](#s01) · [Territory](#s02) · [Reading](#s03) · [Decisions](#s04) · [Flow](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Use this skill when | Route elsewhere when |
|---|---|
| Changing `main`, `run_reconciliation`, `ReconcileResult`, or local exit branches | Changing a `corelink-dt-webhook` contract or its implementation |
| Changing the declared binary name/path or target dependencies | Establishing API access, a cron schedule, alert delivery, or deployment |
| Reviewing static DLQ-loop or handler-call source relations | Resolving data, provider, credential, consumer, or runtime questions |

<a id="s02"></a>
## S02 — Territory and authority

The territory is `tools/dt-reconcile/Cargo.toml` and `tools/dt-reconcile/src/main.rs`. The manifest declares `corelink-dt-webhook`, serialization, tracing, error, and Tokio dependencies. Those declarations and imports are compilation/source relationships, not proof of resolved features, invocation, or implementation behavior.

This skill grants no authority to access Dependency-Track, inspect a delivery log, drain a durable queue, send an alert, use a secret, change data, schedule a job, or deploy a binary. Maintainers, reverse consumers, configuration values, provider state, and runtime reachability are unknown from this scoped evidence.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| What files and declared target are owned? | [R01–R03](../../../docs/ownership/crates/corelink-dt-reconcile/REFERENCE.md#r01) |
| Which static predicates and failures matter? | [R04–R07](../../../docs/ownership/crates/corelink-dt-reconcile/REFERENCE.md#r04) |
| Which directional source edges can change? | [B02–B05](../../../docs/ownership/crates/corelink-dt-reconcile/BLAST_RADIUS.md#b02) |
| How is a documentation-only change handled? | [M01–M06](../../../docs/ownership/crates/corelink-dt-reconcile/MAINTENANCE.md#m01) |

<a id="s04"></a>
## S04 — Decision form

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Result fields or gap arithmetic change | Trace API-001 and INV-001 | `tools/dt-reconcile/src/main.rs` | A claimed reconciliation outcome is needed |
| Local DLQ loop or retry branch changes | Trace INV-002/003 and REL-003 | Same source file | Queue durability or delivery outcome is needed |
| Imported handler/signature call changes | Trace REL-002 and REL-004 | Manifest plus import/call text | Imported implementation or provider behavior is needed |
| Exit/log branch changes | Trace API-002 and REL-005 | Same source file | Process execution or alert receipt is asserted |
| Policy interpretation is needed | Route to canonical OKF reference | [Wave 013 plan](../../../docs/ownership/WAVE_013_PLAN.md) | Copying, redefining, or revalidating OKF is requested |

<a id="s05"></a>
## S05 — Work flow

1. Confirm the fixed baseline, manifest, and sole declared source path.
2. Read the affected R, B, and M records before making a static change.
3. State whether each claim is manifest/source text, documentary-check output, or unknown.
4. Preserve local arithmetic, loop, typed-result, and exit relations unless deliberately revised.
5. Run only the four supplied documentary checks and baseline diff hygiene when authorized.
6. Hand off provider, data, runtime, deployment, and reverse-consumer evidence needs.

<a id="s06"></a>
## S06 — Stop conditions

Stop if the request needs an actual finding count, delivery count, gap, queue state, replay result, HMAC secret, network/API result, alert receipt, cron reachability, binary invocation, build/test result, deployment, or provider state. Comments, environment-variable reads, URL literals, log text, imports, and exit branches do not prove those facts.

<a id="s07"></a>
## S07 — Handoff and definition of done

Report baseline, exact changed paths, affected API/invariant/relation IDs, source paths read, evidence class, checker commands and statuses, skipped checks, and explicit unknowns. Completeness requires S01–S07, R01–R08, B01–B06, M01–M06, four S-profile structural passes, and a scope-only whitespace-clean diff. Structural passes are not semantic approval, runtime evidence, or cold review.

[Back to start](#s01)
