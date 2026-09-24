---
name: own-corelink-rotation-adapters
description: >-
  Maintain the SOURCE-only ownership record for corelink-rotation-adapters
  contracts and static adapter relations; never use it to assert a key,
  provider, rotation, build, test, or runtime operation.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-rotation-adapters"
  manifest: "crates/corelink-rotation-adapters/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "w009-rotation-adapters-source-static-20260920"
---

# Ownership — corelink-rotation-adapters

This S-profile guide is SOURCE-only. It covers checked-in declarations, types,
predicates, trait signatures, and local adapter implementations. It does not
certify a key, provider, rotation, build, test, target, deployment, or runtime
operation. The verified OKF lookup for the manifest returned no matching
concept; that absence is a route result, not permission to create policy.

[Trigger](#s01) · [Boundary](#s02) · [Reading](#s03) · [Invariants](#s04) · [Relations](#s05) · [Stops](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop when |
|---|---|---|---|
| A public type, predicate, trait signature, error variant, or local adapter declaration changes | Classify the named source contract through R02–R06 | Manifest and named `src/*.rs` text | The request asks whether it operated |

<a id="s02"></a>
## S02 — Ownership boundary

| Condition | Action | Evidence | Stop when |
|---|---|---|---|
| Scope is being set | Own this manifest, its ten local `src` modules, declared test target text, and these four artifacts | `Cargo.toml`, `src/`, `tests/prop_rotation_invariants.rs` | A consumer, provider, scheduler, storage system, or external key owner must decide |

<a id="s03"></a>
## S03 — Reading route

| Question | Read | Limit |
|---|---|---|
| Root export or schema declaration | R02 | An export is not a consumer result |
| Asset/state/type or predicate contract | R03–R04 | Source predicate only |
| Trait, adapter, or error declaration | R05–R06 | Declaration is not an operation |
| Change impact and recovery | B01–B06 and M01–M06 | Do not infer execution |

<a id="s04"></a>
## S04 — Invariant decision

| Condition | Required decision | Evidence | Stop when |
|---|---|---|---|
| One of AX-001 through AX-005 changes | Record its exact textual falsifier and every affected atomic relation | R02–R06; B01–B05 | A result beyond static source is required |

<a id="s05"></a>
## S05 — Static work flow

1. Confirm the pinned baseline, manifest, and four owned paths.
2. Read the affected root/module declaration and its direct local imports.
3. State a falsifiable source predicate and one directed static relation.
4. Preserve the five-axiom and five-unknown boundary in R07–R08.
5. Run only the four documentary checks and the baseline diff in M05.

<a id="s06"></a>
## S06 — Stop conditions

Stop and route the request when it needs key material, a provider, state-store
contents, a scheduler, a rotation outcome, target selection, Cargo/test
execution, network activity, deployment, or runtime observation. Source
comments, examples, fake stores, declared tests, and trait methods are not
operation evidence. Do not add an OKF concept: the verified manifest route is
currently no-match.

<a id="s07"></a>
## S07 — Handoff and definition of done

Report the baseline, exact changed paths, R/B/M identifiers, static relations,
documentary check exit status, the no-match OKF route, and five explicit
unknowns. Done means S01–S07, R01–R08, B01–B06, and M01–M06 are navigable; the
four profile-S checks and scope-only `git diff --check` pass; and no operation
or cold-review claim is made.

[Reference](../../../docs/ownership/crates/corelink-rotation-adapters/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-rotation-adapters/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-rotation-adapters/MAINTENANCE.md#m01) · [Back to trigger](#s01)
