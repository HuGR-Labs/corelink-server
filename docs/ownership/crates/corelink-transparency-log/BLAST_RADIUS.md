---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-transparency-log
manifest: crates/corelink-transparency-log/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: transparency-log-static-source-20260920
---

# corelink-transparency-log — blast radius

Atomic static relations only. A relation names source values that must be read
together; it does not prove a network, audit, cryptographic, persistent, or
runtime effect. Canonical verified OKF material is only routed through [the OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md).

[Input digest](#b01) · [Wire encoding](#b02) · [Submit outcome](#b03) · [Witness parser](#b04) · [Fake/tests](#b05) · [Limits](#b06).

Stable relation keys use `repo:1232040291:boundary:corelink-transparency-log-<surface>`; local REL IDs are anchors only. Each B section below is one producer/consumer boundary; local effect and unknowns are stated in that section.

<a id="b01"></a>
## B01 — REL-001 Signed-input to digest relation

**Relation:** `SignedEntry.payload` feeds SHA-256 then lowercase hex in
`content_sha256_hex`; `signature_algorithm` does not feed the proposed entry.
**Impact:** a payload, digest, or field-use change alters the source wire model
and any caller reading it. **Falsifier:** changing input bytes or digest code
changes the returned 64-character source result. **Evidence:** `entry.rs`.
**Unknown:** upstream canonicalization, signing validity, stored-reader
compatibility, and remote acceptance.

<a id="b02"></a>
## B02 — REL-002 Digest/signature/key to wire relation

**Relation:** `to_rekor_hashedrekord` combines the B01 digest, base64 signature,
and base64 PEM under fixed `apiVersion`, `kind`, `data.hash`, and
`signature.publicKey` fields; `to_wire_json` serializes it. **Impact:** a
literal, serde rename, encoding, or nested-structure change changes callers and
submission implementers. **Falsifier:** a modified literal, rename, or encoding
expression changes the local wire JSON. **Evidence:** `entry.rs`. **Unknown:**
wire-schema acceptance, transmission, privacy outside this model, and version
compatibility.

<a id="b03"></a>
## B03 — REL-003 Submitter to fail-open outcome relation

**Relation:** `witness_or_degrade` maps `RekorSubmitter::submit` success to
`Witnessed` and both transient and non-transient errors to `Degraded`. **Impact:**
changing the trait, error taxonomy, or match arms changes caller control flow
and observability text. **Falsifier:** fake `Err(...)` from
`InMemoryRekor::submit` (`src/submit.rs:214-225`) is matched at
`src/submit.rs:101-117` and returns `WitnessOutcome::Degraded`; changing either
source segment falsifies this mapping. **Evidence:** `submit.rs`, `error.rs`.
**Unknown:** write-path placement, retry, queued work, service
availability, delivery, and durability.

<a id="b04"></a>
## B04 — REL-004 Response JSON to witness-record relation

**Relation:** the parser maps the first object member’s fields into
`RekorWitnessRecord` and `InclusionProof`. **Impact:** field names/types or
record shape changes affect submitter implementations and persisted consumers
if any exist. **Falsifier:** zero entries, missing required fields, or a wrong
expected JSON type returns the source parse error. **Evidence:** `witness.rs`,
`tests/adversarial.rs`. **Unknown:** exactly-one response policy, proof
correctness, checkpoint signature, Merkle arithmetic, response provenance,
timestamp semantics, and storage compatibility.

<a id="b05"></a>
## B05 — REL-005 Fake and test-source relation

**Relation:** `InMemoryRekor` models submitted-entry capture, sticky faults,
saturating index allocation, and synthetic proof-shaped JSON; property,
adversarial, module, and example source consume this local seam. **Impact:** a
fake API/state change can alter source test expectations and examples.
**Falsifier:** consecutive successful fake submissions need not yield expected
indices after a state transition change. **Evidence:** `submit.rs`,
`tests/prop_witness.rs`, `tests/adversarial.rs`, example source. **Unknown:**
test execution, coverage, real concurrency, a public log, and real time.

<a id="b06"></a>
## B06 — Coverage, non-relations, and review route

B01–B05 cover local digest construction, wire representation, trait/outcome
mapping, response parsing, and fake/test-source coupling. They exclude all
untraced callers and first-party integrations, resolved feature graph, real
HTTPS, Rekor behavior, cryptographic verification, queueing, persistence,
auditing, and operations. For a change, identify the affected atomic B section
and R07 axiom; if an excluded boundary is required, stop and request that
owner’s evidence rather than infer it from this source.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Return](#b01)
