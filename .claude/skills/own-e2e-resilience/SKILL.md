---
name: own-e2e-resilience
description: >-
  Route source-static maintenance and review for the e2e-resilience Cargo
  package, its deterministic helpers, and declared resilience scenarios.
  Do not use this skill to certify production rate limiting, circuit breaking,
  back-pressure, CI execution, or provider behavior.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-resilience"
  manifest: "tests/e2e-resilience/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "w015-e2e-resilience-source-1177dad2"
---

# Ownership — e2e-resilience

[Scope](#s01) · [Authority](#s02) · [Read](#s03) · [Decide](#s04) ·
[Change flow](#s05) · [Stop](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Scope

| Use this skill for | Use the named contract owner for |
|---|---|
| `tests/e2e-resilience/src/lib.rs`, its fixture helpers, and the `scenarios` target | Rate-limit API changes: `corelink-ratelimit` |
| The harness's local response types and back-pressure model | Circuit breaker, audit, metrics, and header contracts: `corelink-rate-headers` |
| Static ownership records for this test package | Production wiring, operations, provider behavior, or runtime evidence: owner not established here |

<a id="s02"></a>
## S02 — Authority and five evidence axioms

This package owns its local clock, response types, queue model, scenario fixture, and assertions. It does not own the upstream limiter or circuit contracts, the production Tower composition, CI operation, or incident response. A direct dependency does not transfer implementation ownership.

Apply these five limits to every claim:

1. Manifest and source prove declarations and visible code, not resolved selection or execution.
2. Signatures and re-exports identify static contracts, not behavior beyond their implementation boundary.
3. Imports and trait composition show a source edge, not invocation or the complete consumer graph.
4. In-memory fixtures and test assertions prove neither that tests ran nor external/provider behavior.
5. The wave plan and verified [OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) are routing context; do not copy, redefine, or revalidate OKF policy.

<a id="s03"></a>
## S03 — Read only the needed evidence

| Question | Read |
|---|---|
| Package identity, source contracts, and unknowns | [Reference](../../../docs/ownership/crates/e2e-resilience/REFERENCE.md#r01) |
| Direct, CI, and outside-Cargo boundaries | [Blast radius](../../../docs/ownership/crates/e2e-resilience/BLAST_RADIUS.md#b01) |
| Safe checks and recovery limits | [Maintenance](../../../docs/ownership/crates/e2e-resilience/MAINTENANCE.md#m01) |
| Canonical domain policy | Verified OKF profile linked above; do not restate its rules here |

<a id="s04"></a>
## S04 — Decisions

| Condition | Action and evidence | Stop when |
|---|---|---|
| Change to `LogicalClock`, `HttpStatus`, `ResilienceResponse`, or `BackPressureQueue` | Preserve the falsifiable local predicates in [R04–R05](../../../docs/ownership/crates/e2e-resilience/REFERENCE.md#r04) and inspect the affected test assertions | A conclusion needs execution or a production equivalence claim |
| Change to rate-limit or circuit behavior | Coordinate with the named implementation crate and update only the observed relation in [B03](../../../docs/ownership/crates/e2e-resilience/BLAST_RADIUS.md#b03) | The package source does not identify the correct contract or runtime owner |
| Claim about state, data, or impacts | Keep dependency, data-flow, and impact directions separate in each REL | A resolved graph, runtime caller, or external effect is required |
| Question concerns a built-but-not-wired path, feature, target, or package consumer | Apply the canonical [`built-not-wired`](../built-not-wired/SKILL.md) routing rule and label implementation, wiring, and runtime verification separately | The requested conclusion needs resolved Cargo selection, CI history, composition-root evidence, or runtime observation |
| Question invokes OKF terminology or cross-cutting policy | Route to [`okf-context`](../okf-context/SKILL.md) and the frozen OKF profile; cite the canonical source rather than restating it | OKF/source/ADR/runtime evidence contradicts, or the requested action would redefine OKF |

<a id="s05"></a>
## S05 — Change flow

1. Confirm the manifest identity, pinned source, exact `scenarios` test target, and authorized paths. 2. Read the local API/INV and matching atomic REL; keep the upstream owner distinct. 3. Select the matching target and feature row in [M04](../../../docs/ownership/crates/e2e-resilience/MAINTENANCE.md#m04). 4. Record the baseline and recovery boundary before a source change. 5. Run only the checks authorized for that change; preserve exact output and label non-execution. 6. Update affected ownership claims and

send changed bytes for independent cold review; an author cannot approve their own bytes.

<a id="s06"></a>
## S06 — Stop conditions

Stop and escalate when the source pin or package identity drifts; a claim needs CI history, resolved dependencies, a production call path, live audit/metrics delivery, or provider evidence; or an operational recovery owner is unknown.

Record the missing fact as `UNKNOWN` and name the missing owner/authority. Refuse production dispatch, provider access, secret/customer-data handling, CI claims, or runtime approval from this package. Do not infer that comments, test names, in-memory fakes, a workspace command, or a re-export establish a run or production behavior.

<a id="s07"></a>
## S07 — Handoff

Report the objective, source and integration baselines, changed paths, affected API/INV/FLOW/REL/PROC IDs, commands and actual results, the five evidence axioms, five remaining unknowns, and the next verified package or review route. Include whether each material claim is implemented, wired, runtime-verified, or unknown. A structural checker is not a cold approval. No author self-approval.
