---
name: own-corelink-reapi
description: >-
  Own the static contracts of corelink-reapi gRPC handlers, REAPI wire shapes,
  CAS orchestration, and its hash, worker, and meta seams. Use for source-level
  contract changes; do not use it to assert gRPC serving, storage, or runtime state.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-reapi"
  manifest: "crates/corelink-reapi/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "reapi-static-6ed297f5b"
---

# Ownership — corelink-reapi

Candidate, source-only ownership. It describes checked-in handler contracts and static dependency seams; it does not establish a listening gRPC server, R2/D1 effects, auth-provider behavior, deployment, or production health.

[Trigger](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) · [Flow](#s05) · [Stops](#s06) · [Output](#s07)

<a id="s01"></a>
## S01 — Trigger

| Use this skill when | Route elsewhere when |
|---|---|
| Changing REAPI handler, proto conversion, capability, read, write, or find-missing contracts | Changing hash computation implementation, R2 adapter behavior, or MetaStore schema |
| Changing `CasWriteOrchestrator`, `PatValidator`, or public re-exports | Proving an endpoint listens, a bucket/database changed, or a service is deployed |

<a id="s02"></a>
## S02 — Authority and boundary

Own `crates/corelink-reapi` source contracts and static edges to `corelink-hash`, `corelink-worker`/CAS, and `corelink-meta`. The manifest declares optional `host-server`; declaration is not execution evidence. Consult verified OKF only as canonical context, without copying, redefining, or revalidating it.

<a id="s03"></a>
## S03 — Reading route

| Question | Open |
|---|---|
| Which public modules and feature gate exist? | [R01](../../../docs/ownership/crates/corelink-reapi/REFERENCE.md#r01) |
| What write order is static source evidence? | [R02](../../../docs/ownership/crates/corelink-reapi/REFERENCE.md#r02) |
| Which dependency seams can change? | [B03](../../../docs/ownership/crates/corelink-reapi/BLAST_RADIUS.md#b03) |
| Which evidence is required before a change claim? | [M04](../../../docs/ownership/crates/corelink-reapi/MAINTENANCE.md#m04) |

<a id="s04"></a>
## S04 — Decision rules

| Condition | Required action | Stop if |
|---|---|---|
| Wire or handler contract changes | Trace R01–R08 and B01–B06; preserve falsifiable source citations | A claim depends on an unobserved runtime |
| Write ordering changes | Review R02, R03, B01, and the three dependency owners | Recovery/rollback ownership is unknown |
| Scope or find-missing changes | Review R06 and B05; keep scope non-implication explicit | Policy must be decided outside this crate |
| Feature changes | Separate manifest declaration from compiled/served evidence | Feature selection or target is not supplied |

<a id="s05"></a>
## S05 — Source-only flow

1. Confirm the baseline and clean intended diff.
2. Read only the manifest and source files bearing the affected contract.
3. Map the change to R, B, and M records before editing.
4. Mark SOURCE evidence separately from any execution evidence supplied later.
5. Ask each boundary owner for contract review when a public seam changes.
6. Report unknowns rather than inferring runtime or storage state.

<a id="s06"></a>
## S06 — Stop conditions

Stop on baseline drift, an untraceable public consumer, a required R2/D1/auth/deployment assertion, or a request to use this document as operational proof. Do not make Cargo, test, network, deploy, publish, or production claims from this source-only pack.

<a id="s07"></a>
## S07 — Output and done gate

Output the baseline, changed source paths, affected R/B/M records, static citations, execution mode, and explicit unknowns. Success requires source claims to remain falsifiable; completeness requires all affected static seams to be named; quality requires no policy fork or runtime inference. Definition of Done is candidate documentation plus independent review—not an approval, release, or runtime certification.

[Back to trigger](#s01)
