---
name: own-corelink-billing-aggregator
description: >-
  Own the counter aggregation, deterministic input ordering, JCS/BLAKE3 chain,
  in-memory store, and aggregation audit contracts in corelink-billing-aggregator.
  Use for changes to its public API or invariants; do not use for provider deployment.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-billing-aggregator"
  manifest: "crates/corelink-billing-aggregator/Cargo.toml"
  source-commit: "9f372cc1f5a34752a6b6eb4674df10b3921bce88"
  evidence-set: "w003-aggregator-source-static-20260920"
---

# Ownership — corelink-billing-aggregator

Candidate ownership for the package’s source-defined pure aggregation surface. It is not an approval, runtime certification, or authority to operate billing infrastructure.

[Trigger](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) · [Flow](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Use this skill when | Route elsewhere when |
|---|---|
| Changing aggregate shape, chain functions, ordering, store or audit traits | Changing `UsageEvent` or `IdemKey` implementation in `corelink-billing-emit` |
| Changing `CounterAggregator`, decisions, errors, or in-memory fakes | Wiring Cron, D1, R2, Queue, Cloudflare bindings, or deployment |
| Diagnosing a source-level replay, chain, or audit contract regression | Establishing reverse-consumer compatibility or production reachability |

<a id="s02"></a>
## S02 — Territory and authority

The owned implementation is `src/{aggregator,audit,chain,error,event,store}.rs` and the reexports in `src/lib.rs`; the manifest defines the package identity. `corelink-billing-emit` is a direct package dependency. The analytics dev-dependency supports tests only as declared; it is not evidence of an analytics export.

This skill grants no publication, billing operation, data mutation, deployment, provider credential, or cross-package compatibility authority. Named maintainers and an independent reviewer are not established by the inspected sources.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| What is public and source-defined? | [R03–R05](../../../docs/ownership/crates/corelink-billing-aggregator/REFERENCE.md#r03) |
| What can a change affect? | [B02–B05](../../../docs/ownership/crates/corelink-billing-aggregator/BLAST_RADIUS.md#b02) |
| How are safe evidence procedures bounded? | [M02–M05](../../../docs/ownership/crates/corelink-billing-aggregator/MAINTENANCE.md#m02) |
| Which uncertainty remains? | [R08](../../../docs/ownership/crates/corelink-billing-aggregator/REFERENCE.md#r08) and [B06](../../../docs/ownership/crates/corelink-billing-aggregator/BLAST_RADIUS.md#b06) |

<a id="s04"></a>
## S04 — Decision form

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Aggregate fields, serialization, or constants change | Trace API-001 and chain consumers before editing | Source diff plus canonical-byte and compatibility review | Wire format or reverse consumers are not resolved |
| Ordering, period window, or dedup changes | Preserve or deliberately revise INV-001–003 | Source-level decision-path evidence and focused tests when authorized | Input-set equivalence is ambiguous |
| Chain/link/store behavior changes | Trace API-002/003 and REL-002/003 | Formula, sequence, and failure-path evidence | Any partial-write or atomicity claim lacks a real backend proof |
| Audit event/order changes | Trace API-004 and REL-012 | Pre-mutation ordering in source and error propagation | Sink semantics or delivery guarantees are unknown |
| A production claim is requested | Obtain deployment or observed-runtime evidence | Appropriate evidence class, not source comments | Only SOURCE evidence is available |

<a id="s05"></a>
## S05 — Work flow

1. Confirm the package manifest and source baseline.
2. Read the relevant API, invariant, and relation records before editing.
3. State the affected state transition and evidence class.
4. Keep implementation, provider binding, and reverse-consumer assumptions separate.
5. Run only the authorized local checks; retain their exit status.
6. Request an independent review for final bytes and unresolved compatibility.

<a id="s06"></a>
## S06 — Stop conditions

Stop when the requested change depends on a real provider, a durable transaction, an undeclared consumer, or a compatibility promise not demonstrated in source. Stop if a change makes replay equivalence, chain partitioning, or audit-before-mutation ordering undefined. Do not use an in-memory fake, a package description, or a static dependency edge as evidence that Cron, D1, R2, Queue, or a production chain is reachable.

<a id="s07"></a>
## S07 — Handoff and definition of done

Report the baseline, exact files changed, affected APIs/invariants/relations, evidence class, commands and exit status, skipped checks, and remaining unknowns. Success is a source-consistent, bounded change with every affected contract mapped and no unsupported runtime claim. Completeness requires the four ownership artifacts, structural validation, and a scope-only diff; quality requires explicit boundaries and recovery notes. The author’s checks are not cold review.

[Back to start](#s01)
