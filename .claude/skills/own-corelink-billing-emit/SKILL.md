---
name: own-corelink-billing-emit
description: Ownership routing for corelink-billing-emit; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-billing-emit
  manifest: crates/corelink-billing-emit/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-billing-emit-structural-normalization-20260921
---

# Own `corelink-billing-emit`

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

Candidate static ownership guide for `corelink-billing-emit`. It is grounded in
checked source and manifests at baseline `9f372cc1f`; it establishes neither a
deployed provider nor a runtime delivery path.

<a id="s01"></a>
## S01 — Boundary

Own the public usage-event, idempotency, audit, error, sink-port, and
in-memory-emitter surfaces in `crates/corelink-billing-emit/src/`. The package
has a direct `corelink-analytics` dependency for `Region`; that dependency is
a source/type relationship, not analytics export evidence. Real R2, Queue,
Cron, D1, provider credentials, host wiring, and downstream billing policy are
outside this ownership record.

Evidence: `Cargo.toml`; `src/{lib,event,idempotency,audit,sink,emitter,error}.rs`.

<a id="s02"></a>
## S02 — Start with the contract

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Event shape, kind, unit, period, or schema changes | Read R03–R04 before changing `event.rs` | `UsageEvent::new`, `validate_billing_period` | Consumer wire compatibility is needed |
| Key or replay behavior changes | Read R05 and B02 | canonical bytes and tracker `insert` | Collision/replay persistence is not in source |
| Sink/audit/emitter behavior changes | Read R06 and B03 | trait and in-memory implementations | The work needs a real provider or handler |

<a id="s03"></a>
## S03 — Canonical event decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Add/change a `UsageEventKind` | Preserve explicit serde mapping, canonical list, and unit mapping together | `event.rs`; R04 | Finance/consumer acceptance is not recorded |
| Change CE constants or fields | Trace the static re-export and direct imports first | `lib.rs`; B04 | A route, client, or wire-compatibility claim is required |
| Accept a period from a new path | call `validate_billing_period` or construct with `UsageEvent::new` | R03 | Other construction/deserialization paths need a policy |

<a id="s04"></a>
## S04 — Idempotency decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Change key input/hash/slot handling | Preserve “clone, zero slot, JCS, BLAKE3” unless an explicit compatibility decision exists | R05 | Existing persisted-reader compatibility is unknown |
| Change tracker state | Preserve tenant partition plus canonical-byte comparison on duplicate key | `idempotency.rs`; B02 | Durable transactional semantics are requested |
| Collision is observed | Keep it distinct from a replay and hand off operational response | `BillingEmitError::IdempotencyCollision`; M05 | Live severity/alert delivery is requested |

<a id="s05"></a>
## S05 — Ordering and persistence decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Change accepted emit path | Check its concrete order: tracker insertion, audit emit, then in-memory append | `emitter.rs`; R06 | Atomic rollback or durable ordering is required |
| Change duplicate/collision path | Check that its audit follows the tracker result | `emitter.rs`; R06 | Claiming audit-before-tracker is proposed |
| Change append behavior | Preserve key-set rejection and per-period sequence update in the fake | `sink.rs`; R06 | R2/Object Lock durability is requested |

<a id="s06"></a>
## S06 — Static consumers and providers

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Public export changes | Recheck known direct manifests and source imports/re-export | B04 | Complete reverse graph is requested |
| A real R2/audit/idempotency adapter is requested | Route to the provider/composition owner | B03 | Do not substitute comments or traits for a provider |
| Analytics behavior is requested | Route to `corelink-analytics` owner | manifest `Region` use | Do not infer an analytics export |

<a id="s07"></a>
## S07 — Static completion

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Documentation-only ownership change is ready | Record baseline, four paths, source/manifest paths read, checks, and unknowns | M06 | Do not call structural checks a cold review or runtime proof |
| Runtime, delivery, security, or compatibility proof remains | State it as unknown and request the appropriate owner/evidence | R08; B06 | Do not convert absence into a guarantee |

Continue with [reference](../../../docs/ownership/crates/corelink-billing-emit/REFERENCE.md#r01), [blast radius](../../../docs/ownership/crates/corelink-billing-emit/BLAST_RADIUS.md#b01), and [maintenance](../../../docs/ownership/crates/corelink-billing-emit/MAINTENANCE.md#m01).
