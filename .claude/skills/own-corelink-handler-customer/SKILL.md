---
name: own-corelink-handler-customer
description: >-
  Own the source-defined customer handler traits, requests, audit/SLI seams and
  in-memory fake; do not treat those surfaces as a customer backend or runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-handler-customer
  manifest: crates/corelink-handler-customer/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-handler-customer-structural-normalization-20260921
---

# Ownership — corelink-handler-customer

Candidate ownership is limited to source and manifest evidence. It does not
authorize customer-data access, identity decisions, key or billing operations,
audit persistence, route mounting, Cloudflare Worker execution, or deployment.

[Entry](#s01) · [Boundary](#s02) · [Read](#s03) · [Decide](#s04) · [Flow](#s05) · [Stop](#s06) · [Record](#s07).

<a id="s01"></a>
## S01 — Entry

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A change names customer overview, usage, billing, keys, team, or audit-query traits | Identify the matching trait and request/response module | [R03](../../../docs/ownership/crates/corelink-handler-customer/REFERENCE.md#r03) | The request is to mount or operate an HTTP endpoint |
| A change names audit, SLI, fake state, or error | Trace the local seam and all affected fake paths | [R04](../../../docs/ownership/crates/corelink-handler-customer/REFERENCE.md#r04) | The claim requires a durable sink, metrics delivery, or real identity |

Do not activate this skill for a task confined to HTTP route mounting, native PAT
gate, D1 adapter, CF Worker forwarding, deployment, or live customer traffic; use
the container/route owner and [R03](../../../docs/ownership/crates/corelink-handler-customer/REFERENCE.md#r03).
Do not activate it for real billing, PAT secret lifecycle, identity/session policy,
or durable audit/SLO operation without a change to this crate's typed contract;
route to the respective service owner. These are task-level negative triggers,
including when the task merely mentions a customer endpoint.

<a id="s02"></a>
## S02 — Territory and authority

**Implementation territory:** `src/{audit,error,handler,observer,request}.rs`,
`src/audit_query.rs`, the five request modules, and their `lib.rs` reexports.
**Public contract:** six traits, DTOs, `CustomerHandlerError`, `AuditSink`,
`SliObserver`, and the in-memory implementations. **Outside territory:** caller
authentication, customer records, PAT authority, Stripe/billing, audit storage,
SLO ingestion, container composition, route mount, CF Worker, and operation.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| Which API or local invariant changes? | [R03–R05](../../../docs/ownership/crates/corelink-handler-customer/REFERENCE.md#r03) and the named source module |
| What static relation may be affected? | [B01–B05](../../../docs/ownership/crates/corelink-handler-customer/BLAST_RADIUS.md#b01) |
| What evidence procedure is permitted? | [M01–M05](../../../docs/ownership/crates/corelink-handler-customer/MAINTENANCE.md#m01) |
| What remains unproved? | [R08](../../../docs/ownership/crates/corelink-handler-customer/REFERENCE.md#r08) and [B06](../../../docs/ownership/crates/corelink-handler-customer/BLAST_RADIUS.md#b06) |
| Qual contexto canônico se aplica? | [`okf-context`](../okf-context/SKILL.md); consulte [handler-trait-seam](../../../docs/knowledge/crates/handler-trait-seam.md) e, para signup/convites, [signup-auto-provision](../../../docs/knowledge/flows/signup-auto-provision.md). Use `python3 scripts/okf_context.py --file crates/corelink-handler-customer/src/handler.rs --full`; não copie nem redefina política OKF. |
| O código existe, mas está ligado ao artefato? | [`built-not-wired`](../built-not-wired/SKILL.md); reporte `implemented`, `wired` e `runtime_verified` separadamente. |

<a id="s04"></a>
## S04 — Decision form

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Trait, DTO, error, or reexport changes | Trace source callers and compatibility assumptions before editing | API rows in R03 and static consumers in B06 | Complete reverse-consumer compatibility is unknown |
| Fake mutation or audit kind changes | Preserve source ordering and failure propagation on every edited arm | R04/R05 and B02–B04 | A durable transaction or audit delivery is asserted |
| Tenant guard, SLI, role, token, or team behavior changes | Separate the local string/DTO/fake behavior from authentication and authorization | Source path and B03/B04 | Identity, privileges, real key material, or customer data are required |
| A claim says this crate is built, wired, or active in a shipped customer route | Apply [`built-not-wired`](../built-not-wired/SKILL.md); record the source-declared native customer route as `implemented` and `wired` through the container builder, conditional on selecting that binary path; keep shipped artifact and `runtime_verified` unknown | [R03](../../../docs/ownership/crates/corelink-handler-customer/REFERENCE.md#r03), [B03](../../../docs/ownership/crates/corelink-handler-customer/BLAST_RADIUS.md#b03) | Shipped artifact selection, Worker forwarding, deployment, or traffic needs separate evidence |

<a id="s05"></a>
## S05 — Work flow

1. Confirm the pinned manifest and source baseline.
2. Map the public symbol to request shape, audit/SLI seam, fake state, and static relations.
3. State a falsifiable source-level predicate before changing a local contract.
4. Preserve the distinction between a trait/fake and any adapter, route, provider, or runtime.
5. Run documentary checks in `READ_ONLY` mode. If the task includes isolated
   Rust code validation, route the exact targets in [M04](../../../docs/ownership/crates/corelink-handler-customer/MAINTENANCE.md#m04)
   to an authorized `LOCAL_ISOLATED` executor; record execution separately.
   Request independent review of final bytes.

<a id="s06"></a>
## S06 — Stop conditions

Stop for customer data, real principal identity, session authentication, PAT secret,
Stripe/billing action, audit backend, SLO backend, D1/other storage, route mount,
CF Worker, network, or deploy. Stop this source-only route if the task requires
Cargo compilation or tests and route to the isolated code-validation gate in
[M04](../../../docs/ownership/crates/corelink-handler-customer/MAINTENANCE.md#m04).
Stop if source no longer establishes the affected ordering or compatibility
requires an unclassified consumer; escalate with the specific unresolved edge.

<a id="s07"></a>
## S07 — Record

The change packet records objective, baseline, files read and changed, public
symbols, source-only relations, selected gates and literal results, residual
risk/unknowns, and next responsible owner. Mark each Rust gate executed,
reviewed-not-executed, or blocked with its executor and evidence. A checker pass is neither
cold review nor evidence of a real customer, key, billing, audit, route, worker, or
runtime operation.

[Back to entry](#s01)
