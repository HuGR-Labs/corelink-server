---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dual-approval
manifest: crates/corelink-dual-approval/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dual-approval-structural-normalization-20260921
---

# corelink-dual-approval — blast radius

Static relations only. “Dependency” identifies a declared or source edge, “flow” identifies a source-level value path, and “impact” identifies the consequence suggested by that evidence. No relation proves runtime invocation, administrative authority, audit delivery, or deployment reachability.

[Gate](#b01) · [HMAC](#b02) · [Nonce](#b03) · [Collusion](#b04) · [Audit](#b05) · [Consumer](#b06)

<a id="b01"></a>
## B01 — Request-to-gate relation

Dependency: `DualApprovalGateImpl` depends on `AdminRoleStore`, `InMemoryCollusionStore`, `InMemoryNonceStore`, `AdminOpAuditSink`, and an `AdminSigningKey`. Flow: `AdminOpRequest` plus supplied MFA/server timestamps enters the ordered checks and, only on their success, returns `VerifiedApproval`.

Impact: changing check order, thresholds, role predicate, or error routing can change which requests the pure gate accepts. In particular, `now_ms.saturating_sub(caller_mfa_ts_ms)` accepts a future supplied MFA timestamp as age zero, so this is only a supplied-value age predicate and does not establish a genuine MFA ceremony. The source has no handler invocation or operation-execution edge. Evidence: `src/gate.rs:38-114`, `src/gate.rs:159-314`, `src/types.rs:14-145`.

<a id="b02"></a>
## B02 — Payload/nonce/timestamp-to-signature relation

Dependency: HMAC verification consumes the signing key and request payload, nonce, timestamp, and signature. Flow: `op_payload || nonce || ts_ms.to_be_bytes()` → HMAC-SHA-256 32-byte tag → constant-time equality result. Impact: changing field order, byte representation, tag size, key type, or comparator changes signature compatibility and may change gate acceptance. This source relation does not attribute a signature to a person or establish production payload canonicalization or key control. Evidence: `src/hmac_verify.rs:21-94`, `src/gate.rs:218-238`, `src/types.rs:25-42`.

<a id="b03"></a>
## B03 — Caller/nonce-to-replay relation

Dependency: the gate calls `InMemoryNonceStore::check_and_record`. Flow: caller UUID and 16-byte nonce → per-caller hash set → first occurrence succeeds or repeat returns `NonceReplay`. Impact: changing the key scope or recording point changes source-level replay rejection; sharing, persistence, and D1 uniqueness are not established by the in-memory store. Evidence: `src/nonce.rs:14-57`, `src/gate.rs:255-269`.

<a id="b04"></a>
## B04 — Tenant/destructive-history-to-collusion relation

Dependency: destructive operation classification feeds the in-memory collusion store. Flow: tenant UUID plus proposed approver UUID → newest-first scan of the tenant's recorded destructive approvals within 24 hours → last at most two distinct approver UUIDs → accept or `CollusionRotation`; after successful audit emission, a destructive approval is recorded. Impact: changing operation taxonomy, tenant key, time window, distinctness, limit, or record order changes the rolling predicate. The relation is not

evidence of a complete shared history or prevention of real human collusion. Evidence: `src/types.rs:47-83`, `src/collusion.rs:28-151`, `src/gate.rs:240-304`.

<a id="b05"></a>
## B05 — Gate-outcome-to-audit-call relation

Dependency: the gate builds `AdminOpCloudEvent` values through `AdminOpCloudEventBuilder` and calls the injected audit sink. Flow: each local denial branch constructs a denied event and ignores sink errors; the success path constructs an executed event, requires `emit` success, then records destructive approval and returns. Impact: changing builder fields, outcome mapping, or emit ordering changes what the local sink receives and whether a sink failure blocks a successful gate return. It cannot

establish durable delivery, a real audit signature, a preceding external operation, or atomic storage. Evidence: `src/gate.rs:119-157`, `src/gate.rs:159-314`, `src/audit.rs:17-184`.

<a id="b06"></a>
## B06 — Known static consumer relation

Dependency: workspace dependency configuration names `corelink-dual-approval`, and `corelink-ops` directly depends on it. Flow: `corelink-ops::admin::dual_approval` re-exports the public API. Separately, `AdminApiPipeline::process` imports `AdminOpRequest`, `DualApprovalGate`, and `VerifiedApproval`; after schema validation it calls `gate.verify`, maps `DualApprovalError` to `AdminApiError::DualApprovalRejected`, and passes the result to `dispatch_op`. The dispatcher consumes `VerifiedApproval`, matches `AdminOpType` to a handler stub, and returns a dispatch error for an unknown variant. Impact: an exported request/gate/result/error/type change needs `corelink-ops` manifest, middleware, handler,

and error-source review. This is a direct static pure-pipeline edge, not proof that an HTTP endpoint invokes it, that the method runs, or that a handler produces an external administrative effect; reverse-dependency reachability remains incomplete. Evidence: root `Cargo.toml:136,630`, `crates/corelink-ops/Cargo.toml:19`, `crates/corelink-ops/src/admin.rs:16-34`, `crates/corelink-ops/src/admin/api/middleware.rs:13-92`, `handlers.rs:12-92`, `error.rs:3-36`.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01).
