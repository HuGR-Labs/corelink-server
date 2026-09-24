# `corelink-hash` remaining peer census — 2026-09-23

Read-only comparison of the listed hash relations against package BLAST docs,
their pinned source, and the campaign snapshot checkout. This is source/document
reconciliation only: no Cargo resolution, tests, runtime, deployment, peer-owner
confirmation, or review approval is claimed.

## Results

| Hash relation | Source fact | Peer document | Classification |
|---|---|---|---|
| REL-020 | S20: `corelink-ac/src/ac_core/types.rs` imports `Digest`; `digest_serde` serializes/parses it for `ActionDigest` and output digest types | `corelink-ac` BLAST has no hash relation | Source boundary verified; peer record absent |
| REL-021 | S21: `BlobMetaKey` stores `Digest`; canonical text is used in metadata addressing | `corelink-meta` BLAST B01 summarizes key → row, no hash endpoint/key | Related summary, no shared atomic identity |
| REL-022 | S22: `migration_sql_blake3_hex` calls `Digest::compute` over embedded migration SQL | `corelink-meta` BLAST B02 summarizes embedded SQL → tests, no hash endpoint/key | Related summary, no shared atomic identity |
| REL-023 | S23: `CasWriteOrchestrator::commit_put` calls `VerifiedBody::new` before writer | `corelink-reapi` BLAST REL-001 documents the same source seam | Counterpart located; IDs/keys differ, bilateral reconciliation absent |
| REL-024 | S24/S25: bridge max is sourced from `CACHE_ENTRY_MAX_BYTES`; bridge digest itself is SHA-256 | `corelink-bazel-bridge` BLAST B05 documents both distinctions | Counterpart prose located; no peer atomic relation/key |
| REL-028 | S29: CAS manifest declares `corelink-hash`; searched package source has no semantic import/use in this boundary | `corelink-cas` BLAST has no hash relation | Manifest edge confirmed; semantic use not found in inspected source, not proof of absence |
| REL-032 | M01-M03: both independent meta fuzz targets use `Digest::from_hex` | `corelink-meta-fuzz` BLAST REL-002 records both bins and the provider dependency | Surface counterpart located; no shared identity key |
| REL-037 | `adapter_cache.rs` calls `Digest::compute(bytes).to_hex()` | `corelink-server` BLAST REL-034 aggregates hash dependency; no atomic counterpart | Source consumer located; peer aggregation only |
| REL-038 | `byok_control_transition.rs` hashes wrapped TCS bytes | `corelink-server` BLAST REL-034 aggregates BYOK/hash; no atomic counterpart | Source consumer located; peer aggregation only |
| REL-039 | `byok_control_transition.rs` hashes canonical transition bytes | `corelink-server` BLAST REL-034 aggregates BYOK/hash; no atomic counterpart | Source consumer located; peer aggregation only |
| REL-040 | At `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `BillingIngest::payload_hash` formats stable usage-record fields separated by US, then calls `Digest::compute(image.as_bytes()).to_hex()` at lines 255–278 | `corelink-server` BLAST REL-034 aggregates hash callers and REL-019 describes the ingest route/store contract, but neither shares this hash boundary identity | Current-main source recaptured; semantic fact corrected from “media image” to “canonical billing payload fingerprint”; peer identity and bilateral effects still unreconciled |
| REL-041 | `cas_helpers.rs` verifies BLAKE3 claims with `Digest::{compute,from_hex}` | `corelink-server` REL-051 identifies multipart surface, but not this hash boundary | Related surface only; no shared atomic identity |
| REL-042 | S-source: client verifier reexports hash digest/error/length; C header declares width constants | `corelink-client-verify` BLAST B02 covers Rust imports only; FFI is B05 but not hash peer relation | Rust counterpart located; ABI/use remains unproven |

The source files for REL-037 and REL-040 differ between the hash pin
`cacc44fc43ee2f481e933419ba88b9a19ac6c8e8` and campaign baseline
`cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. REL-040 was re-read directly from
current `main` at `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`; its current semantics
are the canonical billing-payload fingerprint stated above. REL-037 remains
historical to its pin unless separately recaptured. The other cited symbols
were found at the campaign checkout, but the peer artifacts use separate source
pins and are not thereby current-main evidence.

The provisional local REL-040 key changed from the earlier `billing-image`
label to `billing-payload` after source recapture corrected the contract. This
is not a shared peer fingerprint. The candidate ledger keeps
`peer_review=not_reconciled` for all 13 listed relations. REL-028 remains a
declared dependency without a semantic consumer established by this census.
