---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-reapi
manifest: crates/corelink-reapi/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: reapi-static-6ed297f5b
---

# corelink-reapi — maintenance guide

Candidate maintenance procedures at the pinned source commit. Source review is `READ_ONLY`; Rust validation uses `LOCAL_ISOLATED`. PROC-006 is reviewed but has no reproducible execution record for this candidate. No gRPC/storage/runtime certification follows.

[Preparation](#m01) · [Procedure index](#m02) · [Change review](#m03) · [Evidence](#m04) · [Recovery](#m05) · [Escalation](#m06)

<a id="m01"></a>
## M01 — Preparation

Confirm the intended source commit, package manifest, selected target/features and limited file scope before review. The record pin can differ from worktree HEAD; compare cited source with `git show 6ed297f5b2b64cf97447985111a2ecbbaa9536bb:<path>` and mark drift explicitly.

For isolated Rust validation, use a disposable local target directory: `REAPI_TARGET_DIR=$(mktemp -d)`; confirm it is nonempty and outside a shared build tree before using the M04 commands. Cargo offline cache failure is `BLOCKED_ENVIRONMENT`; do not change the lockfile merely to pass a documentation review. Source reading cannot certify build, network, deploy, R2/D1 state or production behavior.

<a id="m02"></a>
## M02 — Selection and procedure index

[PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005) · [PROC-006](#proc-006)

| Change trigger | Select | Acceptance scope |
|---|---|---|
| Source baseline, feature or manifest changes | PROC-001, M04 manifest/feature rows | Required for a selected source change |
| Write order, hash, R2 or metadata result changes | PROC-002, M04 write rows | Required for a selected write change |
| Proto, transport, capability or path changes | PROC-003, M04 wire rows | Required for a selected wire change |
| PAT or find-missing changes | PROC-004, M04 auth/discovery rows | Required for a selected auth/discovery change |
| Audit event, ID or outbox shape changes | PROC-005, M04 audit rows | Required for a selected audit change |
| Any ownership-document handoff | PROC-006, M04 documentation rows | Required for this candidate handoff |

<a id="m03"></a>
## M03 — Static procedures

<a id="proc-001"></a>
### PROC-001 — Baseline and surface review

**Mode/environment:** `READ_ONLY`, local source checkout; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`, `required_for_acceptance = false` for this document-only repair (true when its source trigger is selected). **Result:** source steps unexecuted as a procedure.
**Trigger/result:** before a REAPI source change; baseline, manifest, and gated modules are identified.
1. Compare the intended source commit with the checkout under review.
2. Read `Cargo.toml` features and `src/lib.rs` conditional exports.
3. Record the exact source paths and the selected R01 citation.
**Evidence:** baseline SHA, file paths, feature text, and reviewer notes.
**Stop/recovery:** baseline mismatch means stop; obtain the correct baseline rather than rewriting evidence.
[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Write-path contract review

**Mode/environment:** `READ_ONLY`, local source checkout; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`, `required_for_acceptance = false` for this document-only repair (true for a write-path change). **Result:** no write-path validation executed.
**Trigger/result:** changing `commit_put`, write handler, or body/digest inputs; ordered seams are recorded.
1. Trace verification, envelope serialization, writer put, and meta commit.
2. Compare R02/R03 with B01/B02/B03 and name changed owners.
3. Record any new failure branch and its unproven recovery assumption.
**Evidence:** cited line ranges and static call-order note.
**Stop/recovery:** unknown rollback owner means stop and escalate; do not claim stored-object recovery.
[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Wire and handler review

**Mode/environment:** `READ_ONLY`, local source checkout; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`, `required_for_acceptance = false` for this document-only repair (true for a wire change). **Result:** no wire validation executed.
**Trigger/result:** proto, CAS, ByteStream, or capability edits; affected wire/handler boundaries are named.
1. Trace public exports and `host-server` gates.
2. Inspect handler imports, request types, and error mapping edges.
3. Record R01/R04/R05 and B04/B06 impacts.
**Evidence:** source locations plus a changed-symbol list.
**Stop/recovery:** missing wire compatibility owner means stop; restore the prior source contract pending decision.
[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Authorization and discovery review

**Mode/environment:** `READ_ONLY`, local source checkout; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`, `required_for_acceptance = false` for this document-only repair (true for an auth/discovery change). **Result:** no auth/discovery validation executed.
**Trigger/result:** PAT scope or find-missing edits; exact-scope behavior and discovery seam are reviewed.
1. Trace `AuthScope`, `require_scope`, and handler scope calls.
2. Compare the change against R06/R07 and B05.
3. Record whether any policy decision is external to this crate.
**Evidence:** cited predicates, handler call sites, and explicit unknowns.
**Stop/recovery:** policy ambiguity means stop; do not infer a scope hierarchy from source comments.
[Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Audit-shape review

**Mode/environment:** `READ_ONLY`, local source checkout; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`, `required_for_acceptance = false` for this document-only repair (true for an audit-shape change). **Result:** no audit validation executed.
**Trigger/result:** envelope, request-id, or meta audit changes; static payload shape and persistence seam are traced.
1. Inspect builder fields and JSON serialization.
2. Trace the `CommitPutRequest` construction to `MetaStore::commit_put`.
3. Record R08/B03 and distinguish shape evidence from delivery evidence.
**Evidence:** source citations, field delta, and downstream-owner request.
**Stop/recovery:** unknown consumer/schema compatibility means stop; preserve existing field shape pending coordinated change.
[Procedure index](#m02)

<a id="proc-006"></a>

### PROC-006 — Handoff and independent verification

**Mode/environment:** `READ_ONLY`, local checkout; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`, `required_for_acceptance = true`. **Result:** the procedure is selected for this handoff, but no checkout-specific, per-command dated environment and literal output record supports `EXECUTED_LOCAL`; Rust checks also remain unexecuted.
**Trigger/result:** documentation/change handoff; reviewers receive falsifiable static evidence and gaps.
1. List changed R/B/M records and all source citations.
2. Name hash, worker/CAS, meta, and wire owners when their seam changes.
3. Run the three REAPI documentation checkers, adapter-host blast checker and `git diff --check` from M04; preserve commands, exit codes and output.
4. Separate source-only results from Rust gates and request independent review of changed bytes.
**Expected predicate:** all four checker verdicts `IMPLEMENTED_CHECKS_PASS` and diff check exit 0; this is structural evidence only. **Evidence:** checker output and diff result recorded in M04, source diff, baseline, and unknowns.
**Stop/recovery:** checker failure requires a scoped documentation fix and rerun; no independent reviewer or execution owner leaves candidate unapproved.
[Procedure index](#m02)


<a id="m04"></a>
## M04 — Tests and validation matrix

Run from repository root. Documentation commands are `READ_ONLY`. Rust commands are `LOCAL_ISOLATED`, use `CARGO_TARGET_DIR="$REAPI_TARGET_DIR"` prepared in M01, and require a Rust executor with offline dependencies; no Rust command below was run for this repair. `--features host-server` selects the package's declared host transport; `--no-default-features` selects pure logic. Do not infer a deployed feature set from either. Each predicate is assertion/terminal state, not merely process startup.

| Change / suite | Exact command | Predicate and owner | Status |
|---|---|---|---|
| REAPI reference structure | `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-reapi/REFERENCE.md` | `IMPLEMENTED_CHECKS_PASS`; document structure only; doc owner | `READ_ONLY`, PROC-006 |
| REAPI blast structure | `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-reapi/BLAST_RADIUS.md` | Same; doc owner | `READ_ONLY`, PROC-006 |
| REAPI maintenance structure | `python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-reapi/MAINTENANCE.md` | Same; doc owner | `READ_ONLY`, PROC-006 |
| Reciprocal adapter blast structure | `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile H --root . docs/ownership/crates/corelink-adapter-host/BLAST_RADIUS.md` | Same; adapter doc owner | `READ_ONLY`, PROC-006 |
| Patch whitespace | `git diff --check` | Exit 0; doc owner | `READ_ONLY`, PROC-006 |
| Pure write and idempotency, `prop_cas`/`prop_idempotency` | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-reapi --no-default-features --test prop_cas --test prop_idempotency` | Digest mismatch leaves no write; retry preserves one logical row/outbox key; REAPI/hash/meta owners | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |
| Pure read/discovery, `prop_cas_read`/`prop_cross_tenant_read`/`prop_find_missing_batch` | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-reapi --no-default-features --test prop_cas_read --test prop_cross_tenant_read --test prop_find_missing_batch` | Metadata gate precedes R2, tombstone/cross-tenant miss stays uniform, duplicate-size slots remain independent; REAPI/meta owners | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |
| Audit vectors, `canonical_vectors` | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-reapi --no-default-features --test canonical_vectors` | Event types, UUID, subject, zero-size/privacy and dedup vectors match; REAPI/meta owners | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |
| Host capabilities, `capabilities` | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-reapi --features host-server --test capabilities` | BLAKE3, version and limits equal source contract; REAPI/wire owner | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |
| Host RPC/HTTP, `handler_e2e`/`read_handler_e2e`/`find_missing_handler_e2e`/`batch_read_blobs_e2e`/`timing_padding_grpc_e2e` | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-reapi --features host-server --test handler_e2e --test read_handler_e2e --test find_missing_handler_e2e --test batch_read_blobs_e2e --test timing_padding_grpc_e2e` | Per-RPC status, body/size, missing slots and padding assertions pass in test harness; REAPI/host owner | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |
| Bit rot integration, `integration_bit_rot` | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-reapi --features host-server --test integration_bit_rot` | Corrupt/missing backing object yields declared failure without unauthorized read; REAPI/CAS owner | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |
| Adapter PAT source calls, package tests | `CARGO_TARGET_DIR="$REAPI_TARGET_DIR" cargo test --locked --offline -p corelink-adapter-host --lib` | Five family resolver paths preserve tenant and typed invalid/other failure mapping; adapter-host/PAT owners | `LOCAL_ISOLATED`, REVIEWED_NOT_EXECUTED |

PROC-006 evidence status: `REVIEWED_NOT_EXECUTED` for this candidate record. The earlier checker summary lacks a per-command dated checkout identity, execution environment and literal stdout/stderr, so it cannot establish an `EXECUTED_LOCAL` procedure result. A later executor must attach those fields and exit codes for each exact M04 documentation command before changing the status. The checker explicitly does not certify external links, semantic completeness, runtime, cold review, profile eligibility or cross-file navigation. All M04 Cargo rows remain `REVIEWED_NOT_EXECUTED`.

If an exact test target requires a different feature in the selected checkout, record the compile failure and reconcile its declared `[[test]]`/`required-features` before changing the command. Do not silently add `--all-features` or claim an unrun result.

<a id="m05"></a>
## M05 — Recovery and compatibility boundary

For a changed API, trait, error mapping, proto, HTTP path or capability value, compare version N against the N-1 caller and artifact at each REL in B03, including adapter-host PAT bridges and generated clients. A source-compatible type name does not prove wire compatibility or that both versions can coexist. The host/wire owner must record selected feature/target, old and new request/response fixtures, rollout order and whether an N-1 caller can use N during a staged deployment; absent evidence remains UNKNOWN.

For a fresh R2 PUT followed by `MetaStore::commit_put` error, `commit_put` returns `Meta` after awaiting the reconciler. Current `R2DeleteReconciler` only warns; the newly written R2 object can survive with no confirmed metadata/outbox commit. A duplicate R2 outcome does not invoke compensation and must never trigger deletion of a preexisting object.

Preserve the digest, tenant, region, request/outbox key, `Fresh/Duplicate` outcome and original error. Worker/R2 and meta/D1 owners inspect actual object, row and outbox state, including concurrent attempts, before deletion, replay or reconciliation. The provider/GC owner chooses compensation or roll-forward; this document cannot certify GC timing or physical deletion.

For an audit idempotency conflict, compare `(request_id,event_type)` and stored payload under the meta owner before any retry. For a code regression, roll back or roll forward the reviewed artifact via normal change control after checking N/N-1 compatibility and generated proto/client artifacts. Reverting source does not reverse R2 objects, metadata rows, outbox rows or delivered events. Rebuild affected artifacts and rerun the selected M04 suites; production migration/deployment is owned by the composition root, not this library.

<a id="m06"></a>
## M06 — Escalation and unknowns

Record each selected PROC's `review_status`, `execution_status`, `required_for_acceptance`, environment, exact command/result and limitation in the handoff. Escalate hash verification to `corelink-hash`, writer/backend behavior to worker/CAS ownership, metadata/outbox behavior to `corelink-meta`, PAT bridge effects to adapter-host and wire compatibility to host/client owners. Unknowns remain: compiled/deployed features, gRPC serving, concrete PAT provider, R2/D1 effects, audit delivery, deployment and production health. A structural checker pass does not approve the candidate without independent review.
