---
name: own-corelink-region
description: Maintain source-only ownership records for corelink-region types, provisioning and migration contracts, and metrics contracts.
metadata:
  evidence-set: corelink-region-static-source-20260920
  source-commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
  manifest: crates/corelink-region/Cargo.toml
  package: corelink-region
  profile: S
  evidence: static-source-and-manifest-only
---

# Own corelink-region

Use this skill only for the ownership records of `corelink-region`. The
evidence boundary is checked source and manifest declarations, not execution,
provider state, deployed regions, replicas, migrations, or metric export.

[Success](#s01) · [Completeness](#s02) · [Quality](#s03) · [Done](#s04) · [Invariants](#s05) · [Unknowns](#s06) · [Method](#s07).

[Reference](../../../docs/ownership/crates/corelink-region/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-region/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-region/MAINTENANCE.md#m01).

Activation: region vocabulary/jurisdiction, region-derived names, region
events/audit interfaces, metric/probe contracts, or migration report changes.
Do not activate for provider provisioning, region selection, replica operations,
or migration execution. For those tasks route to the named operator and stop
until separately scoped runtime authority/evidence exists.

<a id="s01"></a>
## S01 — Success criteria

The package has one ownership skill and three ownership documents. Each names
`crates/corelink-region/Cargo.toml`, uses profile S, records only source-backed
claims, and distinguishes a declaration or fixture from runtime evidence.

<a id="s02"></a>
## S02 — Completeness criteria

Cover the crate root and its region, event, metrics, migration, error, audit,
and probe modules. Record the six `Region` values, jurisdiction mapping,
event/audit contracts, local metric accumulator, migration report taxonomy,
and every absent runtime proof as an unknown.

<a id="s03"></a>
## S03 — Quality standards

Give every material claim a file and symbol. Use “declares”, “constructs”, or
“stores locally” for source facts. Never turn comments, trait names, test
fixtures, or constants into proof of a provider resource, replica, rollout,
migration, alert, or emitted metric.

<a id="s04"></a>
## S04 — Definition of Done

Condition: all four artifacts are present with required navigation, frontmatter,
and no unresolved placeholder. Action: run the supplied profile-S checker and
the assigned-baseline whitespace diff check. Required evidence: four PASS
outputs and a clean diff result. Stop when any checker or diff fails; repair
only the reported artifact and rerun it. Candidate status never means approval
or operational certification.

<a id="s05"></a>
## S05 — Invariants

Condition: a source change touches region vocabulary, jurisdiction, metrics, or
migration data. Action: compare the named symbols in the reference and blast
records with source. Required evidence: `Region::ALL`,
`DoJurisdiction::expected_for_region`, `RegionMetrics`, and `MigrationReport`
still match their documented relations. Stop when a relation is absent or
ambiguous; record it as unknown and seek separately scoped evidence. These
invariants do not establish live behavior.

<a id="s06"></a>
## S06 — Unknowns

Unknown: provider resources and locations, active region routing, replicas,
probe-worker wiring, metric export and alerting, audit delivery, and whether
any migration was run or rolled back. Obtain those only from separately scoped
runtime or operational evidence.

<a id="s07"></a>
## S07 — Method

For a change, read `REFERENCE.md` first, then trace its linked `REL` records and
select a `PROC` in `MAINTENANCE.md`. Read manifest/source statically; run only
the documentary checker and `git diff --check`. Record exact paths and outputs.
Stop if the claim needs a provider, runtime, deployment, credential, migration,
or test result; preserve it as unknown and escalate to that authority.
