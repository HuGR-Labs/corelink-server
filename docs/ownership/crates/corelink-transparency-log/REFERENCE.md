---
schema: corelink-ownership/1.1
document: reference
package: corelink-transparency-log
manifest: crates/corelink-transparency-log/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: transparency-log-static-source-20260920
---

# corelink-transparency-log — ownership reference

SOURCE/static reference at the recorded revision. It covers local entry shape,
submission seam, parser, outcome mapping, and fake. It does not establish an
external log, audit fact, accepted request, valid signature, valid proof, queue,
persistence, availability, or runtime. Canonical verified OKF material is only
routed through [the OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md).

[Identity](#r01) · [Boundary](#r02) · [Entry](#r03) · [Submission](#r04) · [Witness](#r05) · [Evidence](#r06) · [Axioms](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Identity and scope

`crates/corelink-transparency-log/Cargo.toml` names this package and declares
`base64`, `sha2`, `hex`, `serde`, `serde_json`, `thiserror`, `tracing`, and
`async-trait`; dev dependencies are `proptest` and `tokio`. `lib.rs` forbids
unsafe code, exports four modules, re-exports the public seam, and returns
schema version `1`. This is source evidence, not a selected build or dependency
resolution.

<a id="r02"></a>
## R02 — Ownership boundary

This crate owns the representation built from caller-supplied `SignedEntry`,
the `RekorSubmitter` trait, `WitnessOutcome`, response-model parser,
`TransparencyLogError`, and `InMemoryRekor`. It does not define an HTTPS
implementation, upstream signing/key validation, an inclusion-proof verifier,
a retry worker, record storage, or caller composition. Source comments naming a
public service or a post-hoc workflow are deferred-intent evidence only.

<a id="r03"></a>
## R03 — Signed entry and proposed-entry surface

`SignedEntry` holds `payload`, `signature_algorithm`, `public_key_pem`, and
`signature`. `content_sha256_hex` computes lowercase hexadecimal SHA-256 of
`payload`. `to_rekor_hashedrekord` fixes `apiVersion` to `0.0.1`, `kind` to
`hashedrekord`, and the hash algorithm to `sha256`; it base64-encodes the
signature and PEM into `spec.signature`. `signature_algorithm` is carried but
is not used by the builder.

`RekorHashedRekord::to_wire_json` serializes this owned structure. The local
proposed-entry model contains the digest rather than a raw-payload field. That
does not prove a consumer sends this JSON, that a remote schema accepts it, or
that data is never exposed by another path.

**Public contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

<a id="api-001"></a>
### API-001 — Entry construction and wire representation
**Symbols:** `SignedEntry::{new,content_sha256_hex,to_rekor_hashedrekord}`, `RekorHashedRekord::to_wire_json`. **Input:** caller bytes, algorithm label, PEM and detached signature. **Output/errors:** proposed-entry value or serialization error. **Effects:** SHA-256 and base64 are computed locally; proposed entry contains the digest rather than raw payload. **Compatibility:** JSON shape changes affect submitter contract; REL-001/002. **Evidence:** `src/entry.rs`.
[Index](#r03)

<a id="r04"></a>
## R04 — Submission and outcome surface

`RekorSubmitter: Debug + Send + Sync` has async `submit(&RekorHashedRekord) ->
Result<RekorWitnessRecord, TransparencyLogError>`. `witness_or_degrade` builds
the proposed entry, calls that trait, and returns `Witnessed(record)` on `Ok` or
`Degraded(error text)` for every observed `Err`; the transport/non-transport
match changes logging, not the result variant. It therefore cannot return a
`Result` to its caller in this revision.

`TransparencyLogError` has `Serialization`, `ResponseParse`, `Rejected`, and
`Transport`; only `Transport` is `is_transient()`. Error comments are not a
delivery, retry, or durability guarantee. `tracing` events are source calls, not
an observed telemetry stream.

<a id="api-002"></a>
### API-002 — Submit and degrade seam
**Symbols:** `RekorSubmitter::submit`, `witness_or_degrade`, `WitnessOutcome`. **Input:** borrowed proposed entry and injected submitter. **Output/errors:** `Witnessed(record)` or `Degraded(reason)` for every submit error; helper returns no `Result`. **Effects:** one trait call; no retry queue is implemented. **Compatibility:** outcome variants and error mapping affect callers; REL-003. **Evidence:** `src/submit.rs`.
[Index](#r04)

<a id="r05"></a>
## R05 — Witness record and fake surface

`RekorWitnessRecord` contains `entry_uuid`, `log_index`, `log_id`,
`inclusion_proof`, and `witnessed_at_ms`. `InclusionProof` contains
`checkpoint_log_index`, `tree_size`, `root_hash`, `hashes`, and `checkpoint`.
`from_rekor_response_json` accepts an object, selects its first iteration entry,
and requires `logIndex`, `logID`, and all five nested proof fields at their
expected JSON types. It does not require exactly one entry or validate hashes,
tree arithmetic, signatures, checkpoint authenticity, correspondence to the
submitted digest, or time.

`InMemoryRekor` is a mutex-protected fake with sticky armed faults, a submitted
vector, saturating indices, and a synthetic response. Its record timestamp is
zero. It is a local contract/test double, not a public log or proof generator.

<a id="api-003"></a>
### API-003 — Witness response parser
**Symbols:** `RekorWitnessRecord::from_rekor_response_json`, `InclusionProof`. **Input:** JSON object whose first entry contains `logIndex`, `logID`, and proof fields at expected types. **Output/errors:** parsed record or `ResponseParse`. **Effects:** parsing only; no proof, signature, or checkpoint validation. **Compatibility:** field requirements affect response adapters; REL-004. **Evidence:** `src/witness.rs`.
[Index](#r05)

<a id="r06"></a>
## R06 — Static evidence and graph

Inspected source is `src/{lib,entry,submit,witness,error}.rs`, the manifest,
`tests/prop_witness.rs`, `tests/adversarial.rs`, and
`examples/witness_attestation.rs`. The manifest has no declared `corelink-*`
dependency. Repository inverse-manifest text at this revision identifies no
package that declares this crate as a dependency; the example is internal.
Neither result is a resolved reverse graph or execution evidence. Test and
example source describe assertions and fake usage but were not executed.

<a id="r07"></a>
## R07 — Falsifiable source axioms

| Axiom | Falsifier and exact source location |
|---|---|
| AX-001: builder identity | changing literals at `src/entry.rs:71-90` falsifies `hashedrekord`/`0.0.1`/`sha256` construction |
| AX-002: digest binding | changing `Sha256::digest(&self.payload)` or hex at `src/entry.rs:63-66` falsifies payload-to-digest binding |
| AX-003: encoded fields | assigning raw signature/PEM instead of `STANDARD.encode` at `src/entry.rs:83-87` falsifies encoded fields |
| AX-004: fail-open shape | replacing either `Err` arm at `src/submit.rs:101-117` falsifies total `WitnessOutcome` return |
| AX-005: parser minimum | removing required lookups/types at `src/witness.rs:69-102` or `114-162` falsifies missing-field rejection |
| AX-006: fake ordering | changing assignment/increment at `src/submit.rs:228-230` falsifies consecutive fake indices |

These are falsifiable source statements, not axioms about cryptography, Rekor,
or a deployed system.

<a id="r08"></a>
## R08 — Explicit unknowns

Unknown: real transport, endpoint/API compatibility, service identity and
availability, actual submission, signature/key validity, proof/checkpoint
validation, log consistency, response provenance, caller clock, queue/retry,
storage and retention, upstream attestation/audit contents, complete consumers,
features/targets, compatibility, secrets, deployment, runtime, audit outcome,
and cold review. No unknown is closed by comments, trait names, fake behavior,
or unexecuted tests.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Return](#r01)
