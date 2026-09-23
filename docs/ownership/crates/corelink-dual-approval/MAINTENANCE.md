---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dual-approval
manifest: crates/corelink-dual-approval/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dual-approval-structural-normalization-20260921
---

# corelink-dual-approval — maintenance

Procedures are static source and graph review modes. They do not authorize Cargo execution, tests, data writes, secrets, identity/MFA checks, D1 operations, administrative changes, audit delivery, deployment, or publication.

[Baseline](#m01) · [Gate](#m02) · [HMAC](#m03) · [State rules](#m04) · [Audit and types](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Confirm the review baseline

Mode: STATIC_SOURCE. Predicate: a requested change names this package and a commit is known. Procedure: compare the requested SHA, manifest, `src/lib.rs`, and changed paths; retain the exact source-path inventory. Stop: the SHA or tree identity is unavailable or differs. Recovery: obtain a reconciled source snapshot; do not substitute another revision. Evidence: SHA and inspected paths.

<a id="m02"></a>
## M02 — Review gate predicates and ordering

Mode: STATIC_SOURCE. Predicate: `verify`, timestamps, role lookup, operation type, success/denial outcome, or error routing changes. Procedure: trace every return in `DualApprovalGateImpl::verify`; confirm the inclusive 60-second request/server skew and 30-minute supplied-timestamp age predicates, distinct caller/approver UUIDs, positive role-store result, HMAC result, destructive-only collusion result, nonce record, successful audit emit, and post-emit destructive history record.

Record the edge case that `now_ms.saturating_sub(caller_mfa_ts_ms)` yields zero for a future supplied timestamp, which therefore passes the age predicate; passing this check does not validate a genuine MFA ceremony. Stop: a decision requires identity verification, MFA ceremony validation, real administrative role authority, handler dispatch, or HTTP

behavior. Recovery: state that the gate operates on supplied values and injected seams; route the required fact to the integration owner. Evidence: source conditions, constants, error variants, and call order.

<a id="m03"></a>
## M03 — Review HMAC changes

Mode: STATIC_SOURCE. Predicate: signing key, payload, nonce, timestamp, tag, comparison, or signature-facing type changes. Procedure: compare `hmac_verify.rs` and request fields together; preserve or deliberately version the exact preimage ordering and big-endian timestamp representation, fixed 32-byte tag, and `ConstantTimeEq` use. Stop: the decision requires a secret value, custody/rotation state, caller-side canonicalization proof, or measured timing result. Recovery: name the missing operations evidence rather than deriving it from the implementation. Evidence: symbols, byte-flow declarations, and static consumer references.

<a id="m04"></a>
## M04 — Review nonce and collusion state rules

Mode: STATIC_SOURCE. Predicate: nonce scope, operation classification, tenant scope, history ordering, time window, distinctness, or record point changes. Procedure: trace `AdminOpType::is_destructive`, `check_and_record`, `recent_approvers`, `check_collusion`, and `record_approval`; retain the source predicate that only destructive approvals are recorded and only the last at most two distinct tenant approvers within 24 hours deny a proposed match. Stop: a claim requires D1 constraints, shared/durable state, distributed atomicity, complete history, or real-world anti-collusion efficacy. Recovery:

escalate with the exact predicate and state scope that require integration evidence. Evidence: constants, collection keying, branch conditions, and error path.

<a id="m05"></a>
## M05 — Review audit and public-contract changes

Mode: STATIC_SOURCE. Predicate: sink trait, CloudEvent builder, outcome/type mapping, audit fields, request/result/error types, re-exports, or manifest edge changes. Procedure: distinguish validation-denial audit attempts, whose sink errors are ignored, from the success-path audit emission, whose failure returns `Internal`, and post-emit destructive-history recording, whose failure propagates without `emit_denial`. A successful gate return requires successful audit emission. Review `AdminOpRequest`, `VerifiedApproval`, `ApprovalOutcome`, and `DualApprovalError` as public static contracts. Inspect the direct `corelink-ops` manifest/re-export and

pure-pipeline references: middleware imports request/gate/result, invokes `verify`, maps `DualApprovalError`, then calls `dispatch_op`; handlers consume `VerifiedApproval` by `AdminOpType`. Stop: an assertion needs audit persistence/delivery, signature-chain validity, actual prior state, HTTP/runtime invocation, route reachability, external handler effect, or a full reverse dependency graph. Recovery: retain a source-level call-order and pipeline statement and request the appropriate external evidence. Evidence: builder/sink/gate source, public declarations, manifests, and `crates/corelink-ops/src/admin/api/{middleware,handlers,error}.rs`.

<a id="m06"></a>
## M06 — Perform documentary validation and hand off

Mode: STATIC_HANDOFF. Prerequisites: only the four assigned ownership artifacts changed, a known baseline, and an available S-profile structural checker. Procedure: run the checker once for each of the skill, reference, blast-radius, and maintenance artifacts; run `git diff --check`; inspect the changed-path list. Predicate: each document declares the S profile and source baseline, all material claims cite source evidence, and the diff has no whitespace or scope violation. Stop: a structural

checker, scope, or baseline is unavailable. Recovery: report the missing evidence and do not call documentary checks an approval, build, test, runtime result, or cold review. Evidence: commands, exit statuses, SHA, paths, and explicit unknowns.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01).
