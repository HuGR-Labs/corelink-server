---
schema: corelink-ownership/1.1
document: reference
package: corelink-audit-chain
manifest: crates/corelink-audit-chain/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: H
state: author_validated
evidence_set: audit-chain-static-graph-20260920
---

# corelink-audit-chain — ownership reference

H profile: observable module, feature, binary, and test surface recorded from source and static graph. This is not evidence of a running R2 writer, cron, SIEM queue, object lock, deployment, or independent review.

[Identity](#r01) · [Boundary](#r02) · [Modules](#r03) · [Contracts](#r04) · [Archive/export](#r05) · [Features/tests](#r06) · [Consumers](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Identity and evidence

| Field | Static observation |
|---|---|
| Package / manifest | `corelink-audit-chain` / `crates/corelink-audit-chain/Cargo.toml` |
| Source baseline | `59c76cf260bcdeb5246772f70821ac8b7e8a9780` |
| Declared role | CloudEvents audit emitter, per-region BLAKE3 chain, daily verification primitive, and archive/export structures |
| Targets | library modules, `src/bin/verifier.rs`, integration/property/mutation tests, fuzz package, and two benches |
| Evidence method | source files, manifest, and repository text matches; no command execution or runtime observation |

<a id="r02"></a>
## R02 — Ownership boundary

The crate owns its observable Rust contracts: event/hash state, traits, pure verification, in-memory fakes, archive/export structures, Neon-shadow abstractions, and the verifier binary source. `corelink-container` and other callers compose those contracts. A remote-runtime operator owns bucket configuration, credentials, schedules, queue delivery, retention, and deployed state.

The `corelink-analytics::Region` type is reexported, but remains defined by its dependency. Cryptographic and serialization dependencies supply their own implementations. A source declaration of a deferred production path is not a claim that path exists.

<a id="r03"></a>
## R03 — Module map

| Module / target | Observable surface | Boundary |
|---|---|---|
| `event.rs`, `chain.rs`, `epoch.rs`, `error.rs` | CloudEvents-shaped events, `ChainHash`, canonical bytes, links, epochs, and errors | crate contract |
| `audit.rs`, `sink.rs`, `verifier.rs` | audit-sink traits/fakes, R2-shaped sink traits/fakes, verifier/outcome | crate contract; no remote adapter observed |
| `sealed_archive.rs`, `archive_producer.rs`, `exporter.rs` | sealed lines, chunking, producer/sink traits, export and inclusion checks | crate contract |
| `neon_shadow.rs`, `neon_shadow/{native,real,real_tokio_pg,tenant_region}.rs` | shadow traits, resolver/executor abstractions, native and feature-gated adapter paths | composition/runtime binding remains external |
| `src/bin/verifier.rs` | binary target source | invocation/scheduling not observed |
| `lib.rs` | public module declarations and reexports | authoritative package surface entry |

<a id="r04"></a>
## R04 — Core observable contracts

| Surface | Source-observable contract | Limit |
|---|---|---|
| Audit event | `AuditEvent`, `AuditEventKind`, `ChainHash`, CloudEvents constants, genesis constants | external schema acceptance was not checked |
| Link chain | canonicalization and BLAKE3 link helpers; `HashChainBuilder` state | cross-version stored-data compatibility unknown |
| Epoch/keying | `ChainEpoch`, keyring, keyed/unkeyed algorithm constants | key custody and rotation operation unknown |
| Audit/R2 sink | `AuditChainAuditSink`, `R2AuditSink`, captured, failing, and in-memory implementations | names do not prove a remote R2 operation |
| Verification | `ChainVerifier`, `VerifyOutcome`, fail/success types in source | daily invocation is not observed |
| Errors | `AuditChainError`, audit-sink and R2-sink error types | caller mapping is consumer-owned |

<a id="r05"></a>
## R05 — Archive and export surface

`sealed_archive.rs` exports line schemas, chunk key generation, prefix splitting, verification, serialization, and chunk splitting. `archive_producer.rs` exposes flush policy, receipts, an `ArchiveSink`, an in-memory capture, and a failing sink. `exporter.rs` exposes time windows, manifests, exported events, inclusion proofs, an `AuditExporter`, an in-memory exporter, and result verification.

These structures make dependency and flow relationships inspectable. They do not demonstrate that an archive is uploaded, retained, exported to a customer, or accepted by a SIEM.

<a id="r06"></a>
## R06 — Features, tests, and build witnesses

| Item | Source-observable meaning | Limit |
|---|---|---|
| default feature | pure logic, in-memory fakes, and `RealNeonShadowSink` orchestration/traits are built | does not establish production behavior |
| `neon-real` | optional native dependencies enable the Tokio Postgres adapter path | a native build witness, not runtime evidence |
| `live-pg` / `neon_shadow_real` | module is native + `neon-real`; live cases remain ignored and require `NEON_TEST_DSN`; Docker is one optional backend route | DSN, backend, and results are unknown |
| tests | `prop_audit_chain`, `mutation_kills`, `neon_shadow`, `neon_shadow_real`, module tests | none were run for this artifact |
| fuzz / benches | `fuzz/`, `merkle_append`, `jcs_canonicalize` targets | execution and coverage results unknown |

<a id="r07"></a>
## R07 — Static consumer map

Actual inverse manifests identify `corelink-audit`, `corelink-container`, `corelink-clerk-cf`, `corelink-billing-stripe-materializer`, `tools/cli`, and `corelink-audit-chain/fuzz`. This map is an entry point for impact analysis, not a complete Cargo-resolved reverse-dependency graph.

`corelink-container` is the server composition boundary for routes and handler wiring. `corelink-clerk-cf` contains visible tenant-region wiring. Audit and the Stripe materializer consume related audit-chain surfaces. CLI and fuzz code exercise interfaces; they do not establish a deployed operational path. Repository comment matches, including `corelink-billing-aggregator`, are non-consumer references.

<a id="r08"></a>
## R08 — Explicit unknowns and deferred claims

Unknown: human owner/reviewer; complete consumer graph; reader compatibility across releases; configuration and secret sources; actual remote bucket, object-lock, cron, queue, SIEM, database, or deployment state; binary invocation; test execution results; and whether static callers are live.

The manifest and source comments explicitly defer production R2 put, scheduled cron verification, SIEM queue fan-out, and object-lock retention. Those are remote-runtime/operator concerns and are not closed by this reference.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
