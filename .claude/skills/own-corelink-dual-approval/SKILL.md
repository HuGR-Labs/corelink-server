---
name: own-corelink-dual-approval
description: Ownership routing for corelink-dual-approval; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-dual-approval
  manifest: crates/corelink-dual-approval/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-dual-approval-structural-normalization-20260921
---

# Own corelink-dual-approval

Use this skill for `crates/corelink-dual-approval`. It records checked-source and static-manifest facts only. It does not establish a real identity, MFA assertion, administrative authority, audit persistence or delivery, handler dispatch, D1 state, runtime behavior, or independent review.

[Baseline](#s01) · [Gate](#s02) · [HMAC](#s03) · [Nonce and collusion](#s04) · [Audit](#s05) · [Types and consumers](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the source baseline

**Condition:** beginning a change or receiving a diff. **Action:** record the revision, manifest, `src/lib.rs`, and every changed source module. **Evidence:** `crates/corelink-dual-approval/Cargo.toml`, `src/lib.rs`, and the changed-path inventory. **Stop:** the revision or path set is unavailable or differs. Do not substitute a newer checkout or a historical design document.

<a id="s02"></a>
## S02 — Preserve gate ordering and supplied-value boundaries

**Condition:** changing `DualApprovalGate`, `DualApprovalGateImpl`, role lookup, request validation, time bounds, or outcome mapping. **Action:** trace `verify` in order: absolute request/server skew must be at most 60 seconds; supplied `caller_mfa_ts_ms` age must be at most 30 minutes; caller UUID must differ from approver UUID; `AdminRoleStore::is_admin` must return true; HMAC must verify; destructive operations must pass collusion checking; `(caller UUID, nonce)` must be newly recorded; successful audit emission must return `Ok`; then

a destructive approval is recorded and `VerifiedApproval` is returned. **Evidence:** `src/gate.rs`, `src/types.rs`, `src/error.rs`. **Stop:** a conclusion needs authenticated identity, verification of an MFA ceremony, a real role source, a handler execution result, or HTTP status behavior.

<a id="s03"></a>
## S03 — Preserve the HMAC preimage and comparison seam

**Condition:** changing signing-key representation, payload canonicalization boundary, nonce, timestamp, tag, or signature comparison. **Action:** preserve the source preimage order `op_payload || nonce || ts_ms.to_be_bytes()` and the 32-byte HMAC-SHA-256 tag; trace `verify_hmac` to its `subtle::ConstantTimeEq` comparison. Keep `AdminSigningKey::test_zero` test-only. **Evidence:** `src/hmac_verify.rs`, `src/types.rs`. **Stop:** a decision requires key custody, key rotation, canonical-payload production enforcement, measured constant-time behavior, or a signature attributed to a real person.

<a id="s04"></a>
## S04 — Preserve nonce and collusion predicates

**Condition:** changing nonce state, destructive operation classification, tenant scope, collusion history, or retention window. **Action:** preserve nonce uniqueness per caller UUID in the in-memory store. For destructive operation types only, preserve the rejection predicate: proposed approver UUID is among the last at most two distinct approver UUIDs for that tenant whose recorded timestamp is not older than 24 hours. Non-destructive operation types do not enter the collusion history. **Evidence:** `src/nonce.rs`,

`src/collusion.rs`, `src/types.rs`. **Stop:** a claim requires D1 uniqueness, atomicity, cross-instance persistence, an exhaustive event history, or prevention of human collusion.

<a id="s05"></a>
## S05 — Preserve audit call semantics without claiming delivery

**Condition:** changing audit event types, fields, sink behavior, result ordering, or errors. **Action:** distinguish the success path, where `AdminOpAuditSink::emit` must succeed before `verify` returns `VerifiedApproval`, from denial paths, where `emit_denial` discards sink errors. Preserve source-level event selection (`executed` only for `Approved`, otherwise `denied`) and field construction. **Evidence:** `src/audit.rs`, `src/gate.rs`, `src/types.rs`. **Stop:** do not claim an event was durably stored, delivered, atomically batched, signed, or emitted before an external administrative operation; those facts are outside this crate's source.

<a id="s06"></a>
## S06 — Classify types and static consumers

**Condition:** changing exports, `AdminOpRequest`, operation/outcome/error variants, trait signatures, or manifest edges. **Action:** trace re-exports from `src/lib.rs`; record `AdminOpRequest` as an internal request type, `VerifiedApproval` as a returned context, and `AdminOpAuditSink`/`AdminRoleStore` as injected seams. Trace the direct `corelink-ops` pure pipeline: `AdminApiPipeline::process` imports the request/gate/result types, calls `gate.verify`, maps `DualApprovalError` to `AdminApiError::DualApprovalRejected`, then calls `dispatch_op`; handlers select a stub by `VerifiedApproval.op_type`. Inspect its manifest and source before making a compatibility statement. **Evidence:**

`src/lib.rs`, `src/types.rs`, `Cargo.toml`, `crates/corelink-ops/Cargo.toml`, `crates/corelink-ops/src/admin.rs`, and `crates/corelink-ops/src/admin/api/{middleware,handlers,error}.rs`. **Stop:** this static method chain does not establish an HTTP route, runtime invocation, feature-resolved consumer, generated/dynamic consumer, or complete reverse graph.

<a id="s07"></a>
## S07 — Hand off with falsifiable limits

**Condition:** static review is ready to hand off. **Action:** report baseline, changed paths, gate predicates, HMAC preimage, nonce/collusion scope, audit call ordering, the `corelink-ops` static verify-to-dispatch chain and error wrapper, documentary checks, and explicit unknowns. **Evidence:** exact source paths plus the three ownership documents. **Stop:** do not label source inspection or structural document checks as a build, test result, operational approval, audit evidence, HTTP exposure, or cold review.
