---
name: own-corelink-core
description: >-
  Use when changing the CoreLink apex type and clock contracts: TenantId, Digest,
  Region, SecretWrap, CoreError, or Clock. It is source- and static-graph guidance,
  not proof of migration, runtime wiring, deployment, or secret handling authority.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-core"
  manifest: "crates/corelink-core/Cargo.toml"
  source-commit: "5d617634662ee9475dc66cb83b5a57296eb75dc2"
  evidence-set: "core-static-source-20260920"
---

# Ownership — corelink-core

Candidate guide based on checked source and direct Cargo manifests only. It does not establish consumer migration, selected features, runtime reachability, deployment, or secret values.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Identity](#s04) · [Secrets](#s05) · [Time](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A requested change names one of the six core surfaces | Open its reference contract before editing | `src/lib.rs` exports; R04 | Work is chiefly a consumer migration, deployment, or runtime operation |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Keep the change in this crate's pure type, error, or trait surface | `Cargo.toml` has no `corelink-*` dependency; R02 | Treat dependency presence or comments as proof of execution |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A contract or direct manifest consumer is needed | Read [reference](../../../docs/ownership/crates/corelink-core/REFERENCE.md#r04), then [relations](../../../docs/ownership/crates/corelink-core/BLAST_RADIUS.md#b03) | `src/{lib,types,errors,time}.rs`; consumer manifests | Infer a complete reverse graph from the four direct manifest entries |

<a id="s04"></a>
## S04 — Identity and digest formatting

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Tenant, digest, or region representation changes | Preserve documented text, length, casing, and enum contracts; identify affected consumers | R04; `types/{tenant,digest,region}.rs` | Compatibility or data migration is unspecified |

<a id="s05"></a>
## S05 — Secret boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Secret wrapper construction, exposure, or formatting changes | Preserve explicit exposure and redacted debug behavior; use no secret values as evidence | R04; `types/secret.rs` | A request requires logging, copying, or operating credentials |

<a id="s06"></a>
## S06 — Time boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Clock trait or millisecond conversion changes | Preserve wall-clock source and pre-epoch/overflow saturation semantics | R04; `time.rs` | A concrete implementation, monotonic guarantee, or runtime clock wiring is required |

<a id="s07"></a>
## S07 — Handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static ownership work is complete | Report baseline, records changed, direct consumers, and unknowns | [maintenance](../../../docs/ownership/crates/corelink-core/MAINTENANCE.md#m06) | Present document checks as semantic approval, cold review, or runtime proof |

[Back to trigger](#s01)
