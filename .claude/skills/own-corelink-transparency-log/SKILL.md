---
name: own-corelink-transparency-log
description: >-
  Route static contract work for corelink-transparency-log's hashedrekord
  construction, witness parsing, submission trait, fail-open outcome, and fake.
  Excludes a Rekor service, HTTPS transport, queue, persistence, and runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-transparency-log"
  manifest: "crates/corelink-transparency-log/Cargo.toml"
  source-commit: "16d9f0303d849a1ab3df14688bd2c7cdbfee8140"
  evidence-set: "transparency-log-static-source-20260920"
---

# Ownership — corelink-transparency-log

Static source ownership only. It records Rust representations and seams, not a
submission, public witness, audit result, cryptographic verification, queue,
durable record, or running service. Canonical verified OKF material is routed
only through [the OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md).

[Use](#s01) · [Boundary](#s02) · [Read](#s03) · [Decide](#s04) · [Mode](#s05) · [Stop](#s06) · [Record](#s07).

<a id="s01"></a>
## S01 — Use this skill

Use for `SignedEntry`, `RekorHashedRekord`, witness-record parsing,
`RekorSubmitter`, `witness_or_degrade`, `WitnessOutcome`, error classification,
or `InMemoryRekor`. Do not use it to claim that Rekor accepted an entry, an
inclusion proof verifies, an upstream signature is valid, or a retry happened.

<a id="s02"></a>
## S02 — Boundary

The crate owns source-visible SHA-256/base64/JSON representation, an async
submission trait, parse structures, outcome mapping, and a mutex-backed fake.
Consumers own real transport, clock selection, post-hoc scheduling, retry queue,
record persistence, key selection, signature validation, and all external state.

<a id="s03"></a>
## S03 — Reading route

| Question | Read | Stop when |
|---|---|---|
| Wire shape or raw-payload claim | [R03](../../../docs/ownership/crates/corelink-transparency-log/REFERENCE.md#r03) | endpoint acceptance is required |
| Outcome or error behavior | [R04](../../../docs/ownership/crates/corelink-transparency-log/REFERENCE.md#r04) | a write-path/runtime assertion is required |
| Proof record/parser change | [R05](../../../docs/ownership/crates/corelink-transparency-log/REFERENCE.md#r05) | proof validity is required |
| Relation and consumer impact | [B01–B06](../../../docs/ownership/crates/corelink-transparency-log/BLAST_RADIUS.md#b01) | complete resolved graph is required |
| Permitted maintenance mode | [M01–M06](../../../docs/ownership/crates/corelink-transparency-log/MAINTENANCE.md#m01) | external operation is requested |

<a id="s04"></a>
## S04 — Decision rules

| Change | Record | Stop when |
|---|---|---|
| Digest/wire fields | exact bytes, algorithm, field names, and falsifier | compatibility or remote schema acceptance is needed |
| Submitter/outcome/error | trait signature and each source match arm | delivery, queue, or durability is asserted |
| Witness parser | required fields, first-entry behavior, and absent validation | a proof or checkpoint must be trusted |
| Fake/test | fake-only state and unexecuted assertion source | fake is described as Rekor or runtime evidence |
| Public export/dependency | changed symbol and B relation | every feature or consumer must be known |

**Source axioms:** raw payload is not assigned to the proposed-entry data field;
the builder fixes `hashedrekord`, `0.0.1`, and `sha256`; and the driver returns
`WitnessOutcome` for every submit result. Each axiom is falsifiable by its named
source expression in R07; none is an external cryptographic or service axiom.

<a id="s05"></a>
## S05 — Work mode

Use `STATIC_SEAM`: confirm the recorded manifest and baseline, read the changed
symbol plus its R/B/M route, state source evidence separately from unobserved
execution, and preserve an atomic falsifier. Do not run Cargo, tests, network,
or audit procedures under this ownership route.

<a id="s06"></a>
## S06 — Stop and escalate

Stop for an HTTPS client, endpoint/API behavior, public-log availability,
signature or Merkle/checkpoint verification, queue/retry, persistence, clock,
credentials, deployment, or audit conclusion. A trait, source comment,
`tracing` call, response parser, or in-memory fake cannot close those claims.

<a id="s07"></a>
## S07 — Handoff record

Report baseline, manifest, paths, symbols, affected atomic relation, source
axiom/falsifier, selected mode, documentary-check result, and unknowns. Done
means the static boundary is clear; it is not a build, test, audit, compatibility
approval, cryptographic verification, or runtime review.

[Reference](../../../docs/ownership/crates/corelink-transparency-log/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-transparency-log/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-transparency-log/MAINTENANCE.md#m01)
