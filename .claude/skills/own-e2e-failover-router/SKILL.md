---
name: own-e2e-failover-router
description: Route source-static ownership, review, and maintenance work for the e2e-failover-router Cargo package without inferring execution or operational failover.
metadata:
  evidence-set: e2e-failover-router-static-20260921
  source-commit: cb94e251c0f17382565bf863f517945cbb2a84d6
  manifest: tests/e2e-failover-router/Cargo.toml
  package: e2e-failover-router
  version: "1.0"
  evidence: "source-static"
---

# e2e-failover-router ownership

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

<a id="s01"></a>
## S01 — Package and boundary

The package identity is `[package].name = e2e-failover-router` in `tests/e2e-failover-router/Cargo.toml`. Its static boundary is the library and one declared `scenarios` integration-test target. Use the [reference](../../../docs/ownership/crates/e2e-failover-router/REFERENCE.md#r01) for its contracts and the [blast map](../../../docs/ownership/crates/e2e-failover-router/BLAST_RADIUS.md#b01) for one-arrow source relations.

The package is a test harness. Its description and test labels state intent; neither proves that a test, drill, probe, failover, audit delivery, deploy, or production path ran.

<a id="s02"></a>
## S02 — Static evidence axioms

Apply these five limits to every claim:

1. Manifest and source prove declarations and visible code only, not resolved selection or execution.
2. Re-exports and signatures identify static contracts, not behavior beyond their implementation boundary.
3. Imports and trait composition show a source edge, not an invocation or complete consumer graph.
4. In-memory fixtures and test assertions prove neither that tests ran nor external/provider behavior.
5. The wave plan and verified [OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) are routing context only; do not copy, redefine, or revalidate OKF policy.

<a id="s03"></a>
## S03 — Source map

Inspect `tests/e2e-failover-router/src/lib.rs` for the local logical clock, lease ledger, and error enum. Inspect `tests/e2e-failover-router/tests/scenarios.rs` for fixture wiring and eight test functions: five named scenario arms plus residency-graph and two harness/error checks. The exact symbols and assertions are in [R03–R05](../../../docs/ownership/crates/e2e-failover-router/REFERENCE.md#r03).

<a id="s04"></a>
## S04 — Contract ownership

This package owns its clock and append-only test ledger. `corelink-failover-router` owns router and probe contracts; `corelink-replica-worker` owns the re-exported `Region`, `ResidencyGraph`, and audit types. A direct manifest dependency does not transfer those contracts. See [B02–B06](../../../docs/ownership/crates/e2e-failover-router/BLAST_RADIUS.md#b02).

<a id="s05"></a>
## S05 — Change routing

For a local clock, ledger, fixture, or assertion change, update the affected source references and only the corresponding ownership records. For a router, region, graph, or audit contract change, coordinate with that contract's crate owner and revise only the observed relation. Route any canonical policy question through OKF; this package does not adjudicate it.

<a id="s06"></a>
## S06 — Validation and operational limits

Use the supplied profile-S checker for each artifact and `git diff --check` for documentary structure. These checks cannot grant semantic approval. Do not run Cargo, tests, live probes, drill scripts, network calls, GitHub operations, deployment, or production actions under this source/static task. Follow [maintenance](../../../docs/ownership/crates/e2e-failover-router/MAINTENANCE.md#m01).

<a id="s07"></a>
## S07 — Stop and handoff

Stop a documentary conclusion when it needs test execution, a resolved dependency graph, CI target selection, runtime caller discovery, staging/production evidence, or an independent review. Record the missing evidence as unknown and route the work to the relevant package or operations owner. Handoff must identify changed paths, static checks, direct relations, and remaining unknowns; it must not call a structural checker result an approval.
