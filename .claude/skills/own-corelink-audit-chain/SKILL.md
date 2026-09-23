---
name: own-corelink-audit-chain
description: >-
  Use when changing the observable audit-chain module, feature, binary, or test
  surface of corelink-audit-chain. It routes source-graph evidence without
  assigning server composition or remote-runtime operation to this crate.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-audit-chain"
  manifest: "crates/corelink-audit-chain/Cargo.toml"
  source-commit: "59c76cf260bcdeb5246772f70821ac8b7e8a9780"
  evidence-set: "audit-chain-static-graph-20260920"
---

# Ownership — corelink-audit-chain

Candidate ownership from source and static dependency graph only. It does not establish production wiring, scheduled operation, remote persistence, or an assigned human owner.

[Entry](#s01) · [Boundary](#s02) · [Read](#s03) · [Change](#s04) · [Tests](#s05) · [Stops](#s06) · [Record](#s07).

<a id="s01"></a>
## S01 — Entry

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A change names audit events, links, archives, exports, verifier, or Neon shadow | Enter this skill and identify the affected public module | `src/lib.rs` reexports and [R03](../../../docs/ownership/crates/corelink-audit-chain/REFERENCE.md#r03) | The target is a server route or remote operator concern |
| A request claims an R2, cron, queue, or object-lock result | Separate the claim from this crate’s source surface | Manifest/lib comments and [R08](../../../docs/ownership/crates/corelink-audit-chain/REFERENCE.md#r08) | No deployed artifact evidence is supplied |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing a crate-owned type, trait, pure helper, fake, or binary | Keep the contract in this package and map consumers | `src/{audit,chain,epoch,error,event,exporter,sink,verifier,sealed_archive,archive_producer}.rs` | Change needs container composition |
| Wiring a server handler, credentials, schedule, bucket, or database runtime | Hand off to composition/runtime owner | [B04](../../../docs/ownership/crates/corelink-audit-chain/BLAST_RADIUS.md#b04) | Do not present the handoff as implementation proof |

<a id="s03"></a>
## S03 — Read route

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Event or hash-link contract changes | Read `event.rs`, `chain.rs`, `epoch.rs`, then R04 | [R04](../../../docs/ownership/crates/corelink-audit-chain/REFERENCE.md#r04) | A compatibility decision is missing |
| Archive/export/verifier changes | Read `sealed_archive.rs`, `archive_producer.rs`, `exporter.rs`, `verifier.rs` | [R05](../../../docs/ownership/crates/corelink-audit-chain/REFERENCE.md#r05) | Retention or delivery is inferred from a fake |
| Neon feature or tests change | Read `neon_shadow/**`, manifest features, and named test files | [R06](../../../docs/ownership/crates/corelink-audit-chain/REFERENCE.md#r06) | Docker/native availability is unknown |

<a id="s04"></a>
## S04 — Change decisions

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Wire format, hash input, sequence, epoch, or key changes | Enumerate reader and consumer compatibility before editing | [B01](../../../docs/ownership/crates/corelink-audit-chain/BLAST_RADIUS.md#b01), [B02](../../../docs/ownership/crates/corelink-audit-chain/BLAST_RADIUS.md#b02) | N/N-1 reader behavior is unknown |
| Trait/error/public reexport changes | Trace static known consumers and feature gates | [B03](../../../docs/ownership/crates/corelink-audit-chain/BLAST_RADIUS.md#b03) | A consumer contract cannot be classified |
| A source-only review is requested | Record it as static evidence | source paths and git SHA | Do not claim runtime, cold review, or deployment |

<a id="s05"></a>
## S05 — Test routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Pure chain/event behavior changes | Select module tests, `tests/prop_audit_chain.rs`, or `tests/mutation_kills.rs` as applicable | [M03](../../../docs/ownership/crates/corelink-audit-chain/MAINTENANCE.md#m03) | Required command/target is not authorized or available |
| `neon-real` feature changes | Treat the feature as a native build witness, not runtime proof | manifest and [M04](../../../docs/ownership/crates/corelink-audit-chain/MAINTENANCE.md#m04) | Native target evidence is absent |
| A `neon_shadow_real` live case changes | Treat its module as native + `neon-real`; cases remain ignored and need `NEON_TEST_DSN`; Docker is optional | manifest and [M04](../../../docs/ownership/crates/corelink-audit-chain/MAINTENANCE.md#m04) | DSN/backend route is not evidenced |

<a id="s06"></a>
## S06 — Stops and escalation

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Baseline, target module, or manifest differs | Preserve the observation and request scope correction | checkout/manifest diff | Make no ownership conclusion |
| Production durability, scheduling, SIEM, or object locking is needed | Escalate to server composition and remote runtime operators | [B05](../../../docs/ownership/crates/corelink-audit-chain/BLAST_RADIUS.md#b05) | This crate alone cannot close the claim |
| Unknown consumer or persistent format compatibility exists | Request a contract decision | [B06](../../../docs/ownership/crates/corelink-audit-chain/BLAST_RADIUS.md#b06) | Do not guess compatibility |

<a id="s07"></a>
## S07 — Record

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work is ready to report | State baseline, paths read, affected surface, static consumers, and unrun checks | [M06](../../../docs/ownership/crates/corelink-audit-chain/MAINTENANCE.md#m06) | Do not label author validation as cold review |
| An unknown remains | State its boundary and handoff explicitly | R08 and B06 | Do not convert absence of evidence into a pass |

[Back to entry](#s01)
