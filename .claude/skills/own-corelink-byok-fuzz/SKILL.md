---
name: own-corelink-byok-fuzz
description: >-
  Route static ownership work for the corelink-byok-fuzz manifest and its two
  envelope fuzz targets; do not infer target execution or provider activity.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-byok-fuzz"
  manifest: "crates/corelink-byok/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "w016-byok-fuzz-source-1177dad2"
---

# Ownership — corelink-byok-fuzz

[Trigger](#s01) · [Authority](#s02) · [Read](#s03) · [Decisions](#s04) ·
[Workflow](#s05) · [Stop](#s06) · [Output](#s07).

<a id="s01"></a>
## S01 — Trigger

| Use for | Route elsewhere when |
|---|---|
| Static review of `crates/corelink-byok/fuzz/Cargo.toml`, `wrapped_dek_parse`, or `envelope_roundtrip` | A change belongs to the `corelink-byok` implementation, a workflow owner, persisted data, or an operational environment |

<a id="s02"></a>
## S02 — Territory and authority

This package owns its separate fuzz manifest and the two target sources. The
`corelink-byok` package owns the implementation and contracts consumed by the
targets; see its [reference](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#r02).
The in-process `StubKms` is harness code, not a real provider or composition root.

Five invariants guide every conclusion:

1. Cargo's `package.name` is the identity; directory placement is not.
2. A declared target is not evidence that it ran.
3. Fuzz placement does not transfer implementation ownership from `corelink-byok`.
4. Source reachability and workflow references do not establish runtime behavior.
5. Input shapes, stubs, and the harness `unsafe_code` declaration are package-specific; do not claim provider, customer, secret, or runtime facts. A changed artifact byte invalidates its prior review.

The composition root, operational owner, review authority, and escalation route
are UNKNOWN in the inspected source. This skill grants no additional authority.

<a id="s03"></a>
## S03 — Read only what applies

| Question | Read |
|---|---|
| What does each target assert? | [Reference](../../../docs/ownership/crates/corelink-byok-fuzz/REFERENCE.md#r03) |
| What can a change affect? | [Relations](../../../docs/ownership/crates/corelink-byok-fuzz/BLAST_RADIUS.md#b03) |
| How do I assess a static change? | [Maintenance](../../../docs/ownership/crates/corelink-byok-fuzz/MAINTENANCE.md#m01) |
| What is the canonical envelope concept? | [BYOK envelope encryption at rest](../../../docs/knowledge/storage/byok-envelope-encryption.md) |

<a id="s04"></a>
## S04 — Decisions

| Condition | Action and evidence | Stop when |
|---|---|---|
| Target or manifest changes | Reconcile both target declarations, target source, direct dependency keys, and the exact outside-Cargo references in B02 | Package identity, target set, or source pin is unresolved |
| Parent API or serialized shape changes | Route contract decisions to `corelink-byok`; keep this package's assertions separate from persisted-data compatibility | A wire compatibility or runtime claim is required |
| A workflow or script reference changes | Record the static caller and activation condition in BLAST_RADIUS.md | An operator, schedule, or execution result is being inferred |

<a id="s05"></a>
## S05 — Work sequence

1. Confirm the manifest name, source pin, and assigned file scope.
2. Read both target sources and the parent API documentation they call.
3. Reconcile direct dependencies, target declarations, and bounded static callers.
4. Mark unobserved resolution, execution, persistence, and ownership as UNKNOWN.
5. Run only the authorized document checks; report their exact results and limits.

<a id="s06"></a>
## S06 — Stop conditions

Stop on source drift, a changed parent contract without its owner, unresolved
workflow ownership, or any need for Cargo, Rust, fuzzing, provider, network,
production, database, or storage activity. No escalation route was verified;
do not invent one or proceed by implication.

<a id="s07"></a>
## S07 — Handoff

Report baseline and source pin, exact changed paths, target/API/REL/PROC IDs,
static checks with results, source-diff status, unexecuted operations, and
unknown owners or compatibility questions. Author checks are not cold-review
approval or execution evidence.

[Back to trigger](#s01)
