---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-audit-chain
manifest: crates/corelink-audit-chain/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: H
state: author_validated
evidence_set: audit-chain-static-graph-20260920
---

# corelink-audit-chain — maintenance

Source-guided maintenance modes. No command was run or prescribed by this document; execution needs separately authorized environment evidence.

[Baseline](#m01) · [Core contract](#m02) · [Tests](#m03) · [Features](#m04) · [Recovery](#m05) · [Record](#m06).

<a id="m01"></a>
## M01 — Static baseline mode

**Mode:** source inspection. **Predicate:** package, module, or dependency question. **Action:** record manifest SHA and paths read. **Stop:** checkout or manifest differs from the declared baseline. **Recovery:** refresh the static map against the selected baseline. **Evidence:** git SHA, manifest, and exact paths; no execution result is implied.

<a id="m02"></a>
## M02 — Core contract mode

**Mode:** compatibility analysis. **Predicate:** event, chain, epoch, error, or public reexport changes. **Action:** trace `event.rs`, `chain.rs`, `epoch.rs`, `error.rs`, `lib.rs`, verifier/archive/export readers, and known consumers. **Stop:** N/N-1 data or reader policy is absent. **Recovery:** request a contract decision before changing representation. **Evidence:** symbol map, affected consumer list, and explicit unknowns.

<a id="m03"></a>
## M03 — Pure-test selection mode

**Mode:** test planning. **Predicate:** a pure logic surface changes. **Action:** select relevant module tests plus `tests/prop_audit_chain.rs` or `tests/mutation_kills.rs` when their surface is touched. **Stop:** target, command authorization, or environment is unavailable. **Recovery:** record unrun status and retain the selected test rationale. **Evidence:** named test files, target, exit status only if separately executed.

<a id="m04"></a>
## M04 — Feature/native integration mode

**Mode:** feature-boundary analysis. **Predicate:** `neon-real`, `live-pg`, Neon adapter, or tenant-region code changes. **Action:** inspect manifest and `neon_shadow/{native,real,real_tokio_pg,tenant_region}.rs`; the test module is native + `neon-real`, live cases remain ignored and require `NEON_TEST_DSN`, with Docker one optional backend route. **Stop:** native target, DSN, or backend route is not evidenced. **Recovery:** preserve source findings and hand off runtime validation. **Evidence:** feature declaration, cfg path, and declared test files—not a runtime claim.

<a id="m05"></a>
## M05 — External-effect recovery mode

**Mode:** boundary escalation. **Predicate:** requested result involves R2 put, cron verify, SIEM queue, object lock, credentials, retention, or deployed database. **Action:** separate crate contract from server composition and remote runtime ownership. **Stop:** durable state or operating evidence is required. **Recovery:** use the remote operator’s approved procedure; do not treat a source revert as external rollback. **Evidence:** operator-provided state and recovery record, which are absent here.

<a id="m06"></a>
## M06 — Handoff record mode

**Mode:** reporting. **Predicate:** an analysis or change is ready to hand off. **Action:** report baseline, scope, paths, symbols, static consumers, selected/unrun checks, stop condition, recovery route, and unknowns. **Stop:** a conclusion would exceed source/static evidence. **Recovery:** restate the claim at the correct boundary and request missing proof. **Evidence:** repository paths and SHA; author validation is not cold review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
