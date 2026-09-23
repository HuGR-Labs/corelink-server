# Ownership wave 014 — chaos and BYOK/DSR harnesses

Source/static-only package documentation. The verified OKF remains the
designated canonical routing reference; package authors must not copy,
revalidate or redefine OKF policy. The package manifest description is
declared intent, not evidence that a scenario ran or that runtime behavior
was observed.

## Fixed baseline and disjoint ownership

Planning input baseline: `0c412c611`. The plan was committed as `cb94e251c`,
which adds only this wave-plan file; that commit was the exact parent of all
six author branches and is the source/evidence baseline recorded by their
artifacts. Each author may create or edit only the four artifacts for the
package assigned below. No shared index, registry, standard, rollout, status,
manifest, source, test, CI, or another package's ownership path may be changed.
The lead owns review, integration and status.

| Package | Manifest | Profile | Static package boundary |
|---|---|---|---|
| `chaos-campaign` | `tests/chaos/Cargo.toml` | S | library plus 11 declared test targets; default feature set is empty and `chaos` is opt-in |
| `e2e-byok-revoke` | `tests/e2e-byok-revoke/Cargo.toml` | S | library plus 8 declared tests; direct first-party dependencies `corelink-byok`, `corelink-ops`; `proptest` dev dependency |
| `e2e-chaos` | `tests/e2e-chaos/Cargo.toml` | S | library plus 12 declared tests; direct first-party dependency `corelink-chaos-scheduler`; `proptest` dev dependency |
| `e2e-dsr` | `tests/e2e-dsr/Cargo.toml` | S | library plus 12 declared tests; direct first-party dependencies `corelink-dsr`, `corelink-privacy-erasure-worker`; `proptest` dev dependency |
| `e2e-failover-router` | `tests/e2e-failover-router/Cargo.toml` | S | library plus one declared `scenarios` test target; direct first-party dependencies `corelink-failover-router`, `corelink-replica-worker` |
| `e2e-pilot-onboarding` | `tests/e2e-pilot-onboarding/Cargo.toml` | S | library plus 5 declared tests; no direct first-party Cargo dependency is declared |

These identities and target/dependency counts were read from the pinned
manifests; authors must refine module and relation inventories from source.
Harness-local fakes, property tests and environment gates must be distinguished
from their production counterparts. In particular, never run live-provider
tests or imply BYOK, DSR, failover, chaos, onboarding, audit, storage, alert or
customer behavior was executed or observed.

## Required deliverables and gates

Each package owns exactly:

1. `.claude/skills/own-<package>/SKILL.md` with S01–S07.
2. `docs/ownership/crates/<package>/REFERENCE.md` with R01–R08.
3. `docs/ownership/crates/<package>/BLAST_RADIUS.md` with B01–B06 and atomic
   relations.
4. `docs/ownership/crates/<package>/MAINTENANCE.md` with M01–M06 and bounded
   procedures.

Success criteria: a maintainer can route a package task, locate its source
contracts, trace material declared/source-level relationships, and choose a
safe documentary maintenance procedure without inferring execution.

Completeness criteria: exact Cargo identity/targets/features, implementation
and test modules, public/local contracts, falsifiable invariants, dependencies,
source-level consumers and composition boundaries are recorded; unresolved
runtime, CI selection and external-operation facts remain explicit unknowns.

Quality standards: every source fact has a pinned path/anchor; each blast
record has one producer, consumer, surface, activation and failure boundary;
cross-package implementation and contract ownership stay distinct; navigation
works across the four artifacts; size profile S remains within the candidate
caps.

Definition of Done: exactly four owned paths changed; all S01–S07/R01–R08/
B01–B06/M01–M06 sections and five axioms are present; four S-profile structural
checks and `git diff --check` pass; an independent cold reviewer gives a
separate `APPROVE` for each artifact; integration reproduces scope and checks.

Invariants: package identity comes from `[package].name`, not directory name;
manifest declaration does not prove resolved selection; workspace presence,
test names, fakes and feature gates do not prove runtime reachability or
behavior; production/provider execution is out of scope; any changed artifact
bytes require a fresh independent review.

## Fixed execution limits

Authors use source/static evidence only. They do not run Cargo, tests, fuzzers,
live-provider paths, network calls, GitHub operations, deployment or production
actions. Local document checks are structural evidence only and may not emit
`APPROVE`. A failed or unavailable check is recorded as blocked, not bypassed.
