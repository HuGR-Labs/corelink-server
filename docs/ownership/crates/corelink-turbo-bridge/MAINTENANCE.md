---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-turbo-bridge
manifest: crates/corelink-turbo-bridge/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-turbo-bridge-structural-normalization-20260921
---

# corelink-turbo-bridge — static maintenance

All procedures are `SOURCE_REVIEW_ONLY`; this pack authorizes neither Cargo execution nor network, runtime, storage, audit, client, or deployment claims.
[Mode](#m01) · [Index](#m02) · [Procedures](#m03) · [Evidence](#m04) · [Recovery](#m05) · [Escalation](#m06)

<a id="m01"></a>
## M01 — Mode and baseline

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

**Baseline:** `16d9f0303d849a1ab3df14688bd2c7cdbfee8140`. **Scope:** manifest and `src/{lib,handler,adapter,error,audit,events,status}.rs`; tests may be read as declared source but are not run. **Mode:** `SOURCE_REVIEW_ONLY`. If baseline, package, or changed path differs, stop and obtain the intended revision; do not rewrite evidence around the mismatch.

<a id="m02"></a>
## M02 — Procedure index

[PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)


<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Surface and limits review

**Mode:** `SOURCE_REVIEW_ONLY`; **execution:** not executed. **Trigger:** export, request/response, error, status/event, or constant change. **Action:** compare `lib.rs`, type signatures, and `MAX_HASH_LEN`/`MAX_TEAM_ID_LEN`/`MAX_ARTIFACT_TAG_LEN`; record the altered grammar/value and REL-001/006. **Evidence:** baseline, source paths, symbol diff. **Stop/recovery:** if compatibility requires a caller or transport, stop and route it; retain the prior contract until an owner decides.
[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Key and validation review

**Mode:** `SOURCE_REVIEW_ONLY`; **execution:** not executed. **Trigger:** hash, team, caller tenant, or tag change. **Action:** trace guard → audit → key in both implementations; preserve `R03/AX-001` through `R03/AX-003` or explicitly record the changed predicate. **Evidence:** `error.rs`, `handler.rs`, `adapter.rs`, REL-002/004. **Stop/recovery:** caller-tenant authentication, header treatment, or client grammar required means stop; do not infer it from local fields.
[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Port and create-only review

**Mode:** `SOURCE_REVIEW_ONLY`; **execution:** not executed. **Trigger:** `CasReadStore`, `CasWriteStore`, adapter probe/write, or mapped error change. **Action:** record reader/writer signature, `Ok`/`NotFound`/other probe branch, and the distinction from `InMemoryTurboHandler` overwrite behavior. **Evidence:** `adapter.rs`, `R04/AX-004`, `R04/AX-005`, REL-003. **Stop/recovery:** provider behavior, a race/lock guarantee, or recovery requires a concrete owner; no local source claim repairs that boundary.
[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Tag and audit order review

**Mode:** `SOURCE_REVIEW_ONLY`; **execution:** not executed. **Trigger:** tag, artifact write, audit event, or error-order change. **Action:** trace validation, artifact write, optional sidecar, then `PutCommitted`; trace GET attempted/read/tag/served. Record the post-write failure edge separately from pre-operation audit failure. **Evidence:** `src/{adapter,handler,audit,error}.rs`, `R03/AX-003`, `R04/AX-006`, REL-004/005. **Stop/recovery:** do not claim rollback, sink delivery, or object cleanup without the owning provider/audit contract.
[Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Handoff and static gate

**Mode:** `SOURCE_REVIEW_ONLY`; **execution:** record actual command status. **Trigger:** documentation or source-change handoff. **Action:** state baseline, paths, symbols, AX/REL/PROC IDs, and unknowns; run only the authorized ownership-document checker plus `git diff --check`. **Evidence:** checker output and clean diff. **Stop/recovery:** a failed gate is repaired only in the authorized artifact; a requested Cargo/client/network/runtime proof is a separate owner task.
[Procedure index](#m02)


<a id="m04"></a>
## M04 — Evidence standard

Done means every claim is source-falsifiable, every relation is atomic, mode is named, and static unknowns remain visible. It does not mean compiled, tested, mounted, authenticated, reachable, durable, deployed, or live. Use the canonical [OKF Turborepo route](../../../knowledge/surfaces/turborepo.md) only as routing context; do not duplicate or revalidate it.

<a id="m05"></a>
## M05 — Recovery and escalation

| Condition | Route | Do not claim |
|---|---|---|
| Route, request, header, auth, scope, or transport mapping changes | composition/auth owner | local handler source owns the boundary |
| Port implementation, provider atomicity, object cleanup, or durability changes | concrete storage owner | a trait call proves provider behavior |
| Audit sink delivery/recovery changes | audit-sink owner | event construction proves delivery |
| Client compatibility, network, deployment, or runtime question | authorized operational/client owner | static inspection answers it |

<a id="m06"></a>
## M06 — Escalation and residual unknowns

Route every request/composition/auth/transport decision to its composition or auth owner; concrete port semantics, atomicity, durability, and cleanup to the storage owner; audit delivery to the sink owner; and client, network, deployment, or runtime evidence to its authorized operational owner. Preserve the R08 unknown list in the handoff. A reference-qualified axiom identifies local source meaning only; it does not transfer ownership across these seams.

[Reference](REFERENCE.md#r01) · [Impact relations](BLAST_RADIUS.md#b01)
