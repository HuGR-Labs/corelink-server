---
schema: corelink-ownership/1.1
document: reference
package: corelink-dual-approval
manifest: crates/corelink-dual-approval/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dual-approval-structural-normalization-20260921
---

# corelink-dual-approval — reference

Static reference for `corelink-dual-approval` at the recorded commit. It covers manifest declarations and checked-in Rust/test source; it does not prove runtime wiring, a real identity or MFA assertion, administrative authority, D1 behavior, audit delivery, handler execution, or independent review.

[Identity](#r01) · [Gate](#r02) · [HMAC](#r03) · [Nonce](#r04) · [Collusion](#r05) · [Audit](#r06) · [Types](#r07) · [Limits](#r08)

<a id="r01"></a>
## R01 — Package identity and exported surfaces

`Cargo.toml` names the package `corelink-dual-approval`. `src/lib.rs` publicly exposes `audit`, `collusion`, `error`, `gate`, `hmac_verify`, `nonce`, and `types`, and re-exports their principal types and functions. The root forbids unsafe code and denies missing documentation. These declarations identify source ownership; they do not show an HTTP route or an administrative-operation binding. Evidence: `Cargo.toml:1-42`, `src/lib.rs:111-136`.

<a id="r02"></a>
## R02 — Gate acceptance and denial contract

`DualApprovalGate::verify` receives an `AdminOpRequest`, a caller-supplied MFA timestamp, and a supplied server-time argument. `DualApprovalGateImpl::verify` returns `VerifiedApproval` only after these ordered conditions: `abs_diff(now_ms, req.ts_ms) <= 60_000`; `now_ms.saturating_sub(caller_mfa_ts_ms) <= 1_800_000`; caller UUID differs from approver UUID; `AdminRoleStore::is_admin(approver UUID)` is true; HMAC verification succeeds; for destructive types, collusion verification succeeds; nonce recording succeeds; and success audit-sink emission succeeds.

The saturating subtraction maps a future `caller_mfa_ts_ms` to age zero, so this age predicate accepts a future supplied timestamp; it does not enforce that the timestamp is at or before `now_ms`. The clock-skew, MFA-age, caller-equality, role, HMAC, collusion, and nonce validation-denial branches each attempt a denial

audit before returning their corresponding error. By contrast, success-path audit emission failure returns `Internal`, and a post-emit `record_approval` failure propagates, without calling `emit_denial`; a successful return still requires a successful audit emission. Evidence: `src/gate.rs:121-148`, `src/gate.rs:159-314`, `src/error.rs:13-78`.

The supplied timestamp is only age-checked by this crate, using the predicate above; passing that check is not evidence of a genuine MFA ceremony. The role check is an injected trait call, and the crate offers an in-memory role store. Therefore the source does not establish a genuine MFA ceremony, a real identity, or authority in an external role system. Evidence: `src/gate.rs:38-63`, `src/gate.rs:72-86`, `src/gate.rs:184-195`.

<a id="r03"></a>
## R03 — HMAC contract

`AdminSigningKey` wraps exactly 32 bytes. `compute_hmac` computes HMAC-SHA-256 over `op_payload`, then the 16-byte nonce, then `ts_ms` in big-endian eight-byte form, and returns a 32-byte tag. `verify_hmac` recomputes that tag and compares it with `subtle::ConstantTimeEq`; mismatch returns `SignatureInvalid`. `test_zero` creates an all-zero key explicitly documented as test-only. Source review identifies the algorithm and comparison call, not key custody, rotation, payload canonicalization at a caller boundary, or measured timing behavior. Evidence: `src/hmac_verify.rs:21-94`, `src/types.rs:25-42`.

<a id="r04"></a>
## R04 — Nonce replay contract

`InMemoryNonceStore` stores a set of 16-byte nonces keyed by caller UUID. `check_and_record` accepts a first `(caller UUID, nonce)` occurrence and returns `NonceReplay` on a repeat; equal nonces for different caller UUIDs occupy separate sets. The `now_ms` argument is reported in the error but is not retained as a first-seen timestamp by the in-memory implementation. This is an in-memory source contract, not evidence of durable uniqueness, expiry, shared state, or cross-instance replay protection. Evidence: `src/nonce.rs:14-57`, `src/nonce.rs:78-106`.

<a id="r05"></a>
## R05 — Collusion-rotation contract

The destructive `AdminOpType` variants are `ConfigRollback`, `RetentionPolicyReduce`, `FeatureFlagDisable`, `SecretRotationStart`, and `TenantTombstone`; the two listed safe-direction variants are non-destructive. For a destructive request, the gate rejects when the proposed approver UUID appears in the last at most two distinct approver UUIDs recorded for the same tenant within a 24-hour window. The in-memory store walks newest to oldest, stops when an entry is older than `now_ms - 86_400_000`, and records only destructive

approvals after success audit emission. It does not record non-destructive operations. This predicate is a source-level history rule and does not prove durable query semantics, atomicity, complete history, or prevention of real-world collusion. Evidence: `src/types.rs:47-83`, `src/collusion.rs:28-151`, `src/gate.rs:240-304`.

<a id="r06"></a>
## R06 — Audit-envelope and sink contract

`AdminOpCloudEventBuilder` produces event type `corelink.admin.op.executed` only for `ApprovalOutcome::Approved`; every other outcome maps to `corelink.admin.op.denied`. The event contains source, timestamp string, UUID, caller/approver identity objects, supplied MFA timestamp, operation type formatting, SHA-256 payload and derived hashes, nonce, outcome, and a hex signature field. On a successful gate path, `audit_sink.emit(event)` must return `Ok` before the gate records destructive approval or returns `VerifiedApproval`. On denial paths, `emit_denial` intentionally ignores a sink error. The

successful gate currently supplies `[0; 32]` to the audit signature field and derives `prev_state_hash` as SHA-256 of the payload hash; neither is evidence of a signed audit chain or an observed previous state. Evidence: `src/audit.rs:17-184`, `src/gate.rs:119-157`, `src/gate.rs:271-314`, `src/types.rs:147-211`.

`AdminOpAuditSink` is an interface; the supplied implementations capture events in memory or always fail. Its comments describe a possible production sink but this static source cannot establish persistence, batch atomicity, outbox insertion, delivery, retention, or external audit consumption. Evidence: `src/audit.rs:109-184`.

<a id="r07"></a>
## R07 — Request, result, and error-type boundaries

`AdminOpRequest` is documented as an internal operation request rather than a published HTTP contract; it contains caller/approver UUIDs, a fixed-size HMAC tag, operation type, payload bytes, nonce, request timestamp, and tenant UUID. `VerifiedApproval` returns those two UUIDs, the supplied MFA timestamp, operation type, derived hash, and cloned payload. `ApprovalOutcome` maps to fixed string labels; `DualApprovalError` includes missing approver, signature, caller equality, collusion, MFA age, role, nonce, clock, unknown key, and

internal cases. The `AdminOpRequest` and `VerifiedApproval` field names do not by themselves authenticate a person or authorize an operation. Evidence: `src/types.rs:14-145`, `src/error.rs:13-78`.

<a id="r08"></a>
## R08 — Evidence limits and unknowns

The static `corelink-ops` source shows `AdminApiPipeline::process` schema-validates, calls `DualApprovalGate::verify`, maps its error to `AdminApiError::DualApprovalRejected`, and passes the resulting `VerifiedApproval` to `dispatch_op`; the dispatcher selects a handler stub by `AdminOpType`, and the error module wraps `DualApprovalError`. This is a pure source-level pipeline edge, not proof of HTTP exposure, runtime invocation, or an executed external administrative effect. The reviewed static set does not establish real user identity or MFA validation, role-store/D1 behavior,

durable nonce or collusion state, distributed atomicity, actual audit persistence or delivery, audit-signature integrity, key custody/rotation, HTTP response behavior, target builds, test execution, or full consumer reachability. Treat each as unknown pending separately recorded evidence. Evidence: `crates/corelink-ops/src/admin/api/middleware.rs:13-92`, `handlers.rs:12-92`, `error.rs:3-36`.

[Ownership guide](../../../../.claude/skills/own-corelink-dual-approval/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
