---
name: own-corelink-turbo-bridge
description: >-
  Route source-backed changes to the opaque-key Turbo artifact bridge, its
  local adapters, validation, audit surface, and static responses.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-turbo-bridge
  manifest: crates/corelink-turbo-bridge/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-turbo-bridge-structural-normalization-20260921
---

# Ownership — corelink-turbo-bridge

SOURCE-only routing. It is not evidence of route mounting, request handling, a Turbo client, storage-provider behavior, network delivery, deployment, or runtime.

[Start](#s01) · [Boundary](#s02) · [Read](#s03) · [Axioms](#s04) · [Flow](#s05) · [Stop](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Start

| Trigger | Do | Stop when |
|---|---|---|
| Changed `src/{handler,adapter,error,audit,events,status,lib}.rs` | Confirm the baseline and map the changed symbol to R/B/M. | The change needs composition or an external provider. |
| Changed port or audit trait | Freeze the signature with the dependency owner. | A concrete reader, writer, or sink contract is assumed. |

<a id="s02"></a>
## S02 — Boundary

This package owns request/response types, validation, local fakes, the `TurboArtifactHandler` trait, opaque `(tenant,key)` ports, and static event/status values. It does not own authentication, authorization, route registration, body/header extraction, transport status mapping, locking outside its source, concrete storage, audit delivery, or a client. The one canonical context route is [OKF Turborepo surface](../../../docs/knowledge/surfaces/turborepo.md); route to it, do not copy or revalidate it here.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| Public types, limits, or error grammar | [R01–R03](../../../docs/ownership/crates/corelink-turbo-bridge/REFERENCE.md#r01) |
| Key, audit, tag, or port impact | [REL-001–REL-005](../../../docs/ownership/crates/corelink-turbo-bridge/BLAST_RADIUS.md#b01) |
| Review mode and recovery | [PROC-001–PROC-005](../../../docs/ownership/crates/corelink-turbo-bridge/MAINTENANCE.md#m02) |

<a id="s04"></a>
## S04 — Axioms

| Axiom | Required static reading | Falsifier |
|---|---|---|
| R03/AX-001 | Hash rejection is length-only. | Hash grammar/content validation appears or the bound disappears. |
| R03/AX-002 | Artifact key is `caller_tenant` plus `team_id/hash`; team is a sub-namespace, not an equality check. | PUT/GET uses another tenant/key construction. |
| R03/AX-003 | A present tag has the stated grammar; adapter GET revalidates a stored tag. | Grammar or adapter revalidation changes. |
| R04/AX-006 | Attempted audit precedes its operation; committed/served follows its named work. | A named audit emit crosses its operation. |
| REL-004 | Adapter source writes artifact before its optional sidecar. | The static write order changes. |
| R04/AX-005 | `CasAdapterTurboHandler` refuses a confirmed pre-existing artifact key; `InMemoryTurboHandler` overwrites. | Either implementation loses this stated distinction. |

<a id="s05"></a>
## S05 — Review flow

1. Pin `16d9f0303`, manifest, and changed source paths.
2. Identify the public type, validator, implementation, and seam affected.
3. State a reference-qualified axiom (for example `R03/AX-001`) or REL predicate and a counterexample before changing it.
4. Record pre- and post-mutation error edges separately.
5. Run only the authorized static checker and whitespace diff gate; report unexecuted work distinctly.

<a id="s06"></a>
## S06 — Stop conditions

Stop and route to the owner when a request boundary, authentication/authorization decision, transport response, provider semantics, cross-process atomicity, audit delivery, or client compatibility is required. Do not turn source comments into a durability, network, runtime, or deployment claim.

<a id="s07"></a>
## S07 — Handoff

Provide baseline, changed symbols, affected reference-qualified axiom/R/REL/PROC IDs, static evidence, checker status, and unknowns. A failing post-write audit or sidecar edge is not recoverable by this ownership document; name the concrete storage/audit owner before proposing recovery.
