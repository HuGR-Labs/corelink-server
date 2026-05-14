---
id: "WI-S13-002"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.1.0"
created: "2026-04-28"
updated: "2026-05-14"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-13"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s13", "admin-plane", "dual-approval", "collusion-rotation", "nist-ac-2-7", "audit", "high-risk"]
---

# WI-S13-002 — Admin API `POST /v1/admin/ops` + Dual-Approval Enforcement (PAT-DUAL-APPROVAL-001) + Collusion-Rotation Defense (NIST AC-2(7)) + CloudEvent Audit Emission Rich + Hard-Fail 403 sem 2 Signatures

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-13](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S13-002 |
| Título | Admin API endpoint `POST /v1/admin/ops` enforcing dual-approval workflow PAT-DUAL-APPROVAL-001: header `X-Dual-Approver: <user_id>` + `X-Approver-Signature: <hmac>` + caller MFA freshness ≤ 30 min; D1 separation-of-duties hard-check `caller ≠ approver`; **collusion-rotation defense** (NIST SP 800-53 AC-2(7)): new approver MUST NOT have approved any of the last 2 destructive ops in tenant 24h window (rolling 3-op windows com 3 distinct approvers; equivalent strengthening — oracle prévio LIMIT 3 prior tinha bypass A→B/B→A/A→B 3 ops, fix Lote 10.13 codex P0 strengthening); missing approver / sig invalid / collusion-rotation violation = 403 hard-fail (NÃO advisory mode); CloudEvent audit emission rich (`actor + mfa_ts + dual_approver + op_payload + prev_state_hash + signature` chain integrity); chaos test missing approver → blocked + audit |
| Sprint | S-13 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (dual-approval bypass = insider threat trivial; substrate cripto-load-bearing — admin signing key derivation) |

## 1. Intent

Implementar admin API `POST /v1/admin/ops` que serializa toda destructive admin operation com defense-in-depth: (1) **dual-approval enforcement** PAT-DUAL-APPROVAL-001 — request DEVE incluir header `X-Dual-Approver: <user_id>` + `X-Approver-Signature: <hmac>` (HMAC-SHA256 sobre `op_payload || nonce || ts` derivado de admin signing key per-region; **NÃO é PAT** — PAT format canonical hybrid `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` per S-03 cycle 9 SEAL decision (a) é separado); (2) **MFA freshness ≤ 30 min hard-check** caller (CTRL-AUTH-010 + INV-ADMIN-MFA-FRESHNESS); (3) **separation-of-duties** D1 query `SELECT user_id FROM admin_role WHERE user_id IN (caller, approver)` rejecting `caller = approver`; (4) **collusion-rotation defense** NIST AC-2(7) — canonical Lote 10.13 oracle: query `SELECT approver_user_id FROM admin_op_log WHERE tenant_id=$tenant AND op_type='destructive' AND created_at > now - 24h ORDER BY created_at DESC LIMIT 2`; if proposed approver IN {recent_approvers} = 403 (collusion-rotation violation; equivalente a forçar 3 distinct approvers em qualquer rolling window de 3 ops); oracle prévio com LIMIT 3 + count(distinct) < 3 tinha bypass A→B/B→A/A→B (3 ops bypass, 4ª falha) — substituído pela versão stronger que rejeita logo na 3ª op; (5) **hard-fail 403** em qualquer falha (NÃO advisory mode, NÃO grace, NÃO override); (6) **CloudEvent audit emission rich** per CAP-ADMIN-006 com `actor + mfa_ts + dual_approver + op_payload + prev_state_hash + signature` chain integrity + INV-AUDIT-APPEND-ONLY herdada.

```rust
// File: crates/corelink-dual-approval/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[async_trait]
pub trait DualApprovalGate: Send + Sync {
    /// Verify dual-approval pre-handler dispatch. Returns Ok(VerifiedApproval) or DualApprovalError.
    /// Hard-fail (no advisory mode); missing/invalid approver = 403.
    async fn verify(
        &self,
        req: &AdminOpRequest,
        caller_mfa_ts_ms: u64,
    ) -> Result<VerifiedApproval, DualApprovalError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminOpRequest {
    pub caller_user_id: Uuid,
    pub approver_user_id: Uuid,                 // from X-Dual-Approver header
    pub approver_signature: [u8; 32],           // HMAC-SHA256, from X-Approver-Signature header
    pub op_type: AdminOpType,                   // canonical enum (destructive vs non-destructive)
    pub op_payload: Vec<u8>,                    // canonical JSON serde_jcs RFC 8785
    pub nonce: [u8; 16],                        // approver-side replay protection
    pub ts_ms: u64,                             // request timestamp (clock-skew tolerance ≤ 60s)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdminOpType {
    // Destructive (gated)
    ConfigRollback,
    RetentionPolicyReduce,
    FeatureFlagDisable,
    SecretRotationStart,
    TenantTombstone,
    // Non-destructive (still audited; approver-optional but recommended)
    FeatureFlagToggleSafe,
    RateLimitAdjustUp,
}

#[derive(Debug, Clone)]
pub struct VerifiedApproval {
    pub caller_user_id: Uuid,
    pub approver_user_id: Uuid,
    pub mfa_ts_ms: u64,
    pub op_type: AdminOpType,
    pub prev_state_hash: [u8; 32],              // SHA-256 do prior state for rollback
}

#[derive(Debug, thiserror::Error)]
pub enum DualApprovalError {
    #[error("missing X-Dual-Approver header")]
    MissingApprover,
    #[error("approver signature HMAC invalid")]
    SignatureInvalid,
    #[error("caller equals approver (separation of duties violated)")]
    CallerEqualsApprover,
    #[error("collusion-rotation violation: proposed approver appeared in last 2 destructive ops in 24h (rolling 3-op window must have 3 distinct)")]
    CollusionRotation { recent_approver_uuids: Vec<[u8; 16]> },
    #[error("MFA stale (last MFA {age_min}min ago; max 30min)")]
    MfaStale { age_min: u32 },
    #[error("approver lacks admin role")]
    ApproverNotAdmin,
    #[error("nonce replay detected (last seen {seen_at_ms})")]
    NonceReplay { seen_at_ms: u64 },
    #[error("clock skew > 60s (request_ts={req_ms}, server_ts={srv_ms})")]
    ClockSkew { req_ms: u64, srv_ms: u64 },
}
```

D1 schema (extends WI-S13-001 audit foundation):
```sql
CREATE TABLE admin_op_log (
    op_id BLOB(16) PRIMARY KEY,                 -- UUID
    op_type TEXT NOT NULL,
    caller_user_id BLOB(16) NOT NULL,
    approver_user_id BLOB(16),                  -- NULL only for non-destructive
    op_payload_hash BLOB(32) NOT NULL,
    prev_state_hash BLOB(32) NOT NULL,
    mfa_ts_ms BIGINT NOT NULL,
    nonce BLOB(16) NOT NULL,
    ts_ms BIGINT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('approved', 'denied_missing', 'denied_sig', 'denied_caller_eq', 'denied_collusion', 'denied_mfa_stale')),
    created_at_ms BIGINT NOT NULL DEFAULT (unixepoch('subsec') * 1000),
    UNIQUE (caller_user_id, nonce)              -- replay protection
);
CREATE INDEX idx_admin_op_log_collusion ON admin_op_log(op_type, created_at_ms DESC) WHERE op_type LIKE 'destructive%';
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Dual-approval workflow é a última defesa contra **insider threat single-engineer** + **2-engineer reciprocal collusion**. Sem dual-approval, qualquer admin compromised = full destructive blast radius (delete tenant, disable feature global, reduce retention to 0). Com dual-approval simples (caller ≠ approver), 2 admins colludentes via "I approve your op, you approve mine" = bypass trivial. Defense-in-depth NIST SP 800-53 AC-2(7) requer **collusion-rotation tracking** — canonical Lote 10.13: new approver MUST NOT match approver of last 2 destructive ops em 24h window (equivale a forçar 3 distinct approvers em rolling 3-op windows; CoreLink: oracle stronger que LIMIT 3 + DISTINCT count, que tinha bypass A→B/B→A/A→B 3 ops). Substrate cripto-load-bearing porque admin signing key derivation cripto + audit chain integrity = bypass detect post-facto possível mas damage já feito.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Advisory mode regression**: dual-approval downgraded to "warning + log" sem 403; advisory bypassable by ignoring warning. Mitigação: hard-fail 403 enforced em código + property test 10k assert 0 advisory paths; CI gate forbids `advisory_mode` flag.

2. **Bypass via separation-of-duties skip**: caller==approver allowed in dev/staging "for testing". Mitigação: D1 hard-check enforced em todos environments; staging integration test asserts caller==approver = 403; no env-gated bypass.

3. **Collusion 2-engineer A↔B reciprocal**: admin A approves admin B's op; later admin B approves admin A's op; both bypass. Mitigação: collusion-rotation rolling defense (Lote 10.13 canonical) — D1 query last 2 destructive ops/24h por tenant; new approver IN recent set = 403. A→B/B→A/A→B detectado na 3ª op (não na 4ª como oracle prévio); audit emit `denied_collusion_rotation` + Slack alert.

4. **HMAC signing key compromise**: attacker steals admin signing key (per-region, derived from rotation worker WI-S13-003); forges valid `X-Approver-Signature` for arbitrary approver_user_id. Mitigação: signing key rotation 24h overlap (WI-S13-003 + key_management.md §3.2.1); HSM-equivalent storage (Cloudflare Workers Secrets); rotation worker restricts key access scope; audit chain detects post-rotation anomalies.

5. **Replay attack**: attacker replays valid request with old nonce; D1 UNIQUE `(caller_user_id, nonce)` rejects. Mitigação: nonce 128-bit random + UNIQUE constraint; clock-skew tolerance ≤ 60s prevents future-dated replays; audit emission tracks nonce reuse.

6. **MFA timestamp manipulation**: attacker forges `mfa_ts_ms` to bypass freshness check. Mitigação: `mfa_ts_ms` signed by Clerk IdP (JWT claim `auth_time`); WebAuthn UV=1 attestation embedded; clock-skew tolerance ≤ 60s; CTRL-AUTH-010 + INV-AUTH-CLOCK-SKEW-BOUND herdada S-03.

7. **Approver privilege drift**: approver had admin role at sig time but role revoked before request; D1 query checks current admin_role. Mitigação: query `SELECT 1 FROM admin_role WHERE user_id = approver AND active = true AND revoked_at IS NULL` runtime; revocation propagation ≤ 60s herdada S-03.

8. **Audit emission failure post-approval**: dual-approval succeeds but audit_outbox INSERT fails; op proceeds com chain break. Mitigação: D1 atomic batch [admin_op_log INSERT + audit_outbox INSERT]; failure rolls back; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER herdada S-03.

**Atacante adversarial scenarios**:

- **Compromised admin credentials + steal approver session**: attacker steals 2 sessions; tries A→approve A pattern; D1 separation check rejects (caller_eq violation).

- **Reciprocal A↔B attempts**: attacker compromises 2 admins; tries A approve B, then B approve A in 24h; collusion-rotation rolling defense detects na 3ª op (proposed approver IN last 2 distinct prior approvers = {A, B}; new approver=A ou B ∈ set → 403); 403 + alert. (Lote 10.13 canonical strengthening; oracle prévio detectava só na 4ª op.)

- **Forge HMAC signature with non-rotated key**: attacker tries old signing key post-rotation 24h overlap; verifier checks current+1 prev key_id; post-grace = KeyIdUnknown rejection.

- **Bypass via direct D1 write to admin_op_log**: attacker tries direct Postgres INSERT to bypass API; D1 access via Worker binding only (Cloudflare platform isolation); audit chain integrity catches via daily verifier.

- **Property test fuzz reaches edge case**: 10k random approver/signature/timing combinations explore corner cases (e.g., simultaneous MFA expiry + clock skew); 0 false-accepts mandatory.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: substrate cripto-load-bearing (admin signing key derivation cripto via rotation worker WI-S13-003); bypass = insider threat trivial.
- **Reversibility**: bypass detected post-facto via audit chain; mas destructive op already executed (e.g., tenant tombstone); recovery via rollback API (WI-S13-001) requires 7d retention window.

11 sign-offs canonical incl. Architect (Crypto SME specialization mandatory: HMAC signing key derivation + audit chain integration + collusion-rotation algorithm review + adversarial attestation forge tests) + Security Lead (NIST AC-2(7) compliance + threat model) + AppSec (admin API surface + privilege escalation review).

## 3. Customer Impact & Journey

**Persona 1 — SRE on-call em prospect enterprise (RFP evaluation)**:
- RFP question: "What controls prevent insider threat from a single admin?".
- Evidence: PAT-DUAL-APPROVAL-001 + collusion-rotation defense NIST AC-2(7); property test 10k 0 bypasses; chaos test missing approver → 403 verified.
- Diferenciador competitivo: BuildBuddy/NativeLink admin = single-engineer; CoreLink S-13 = NIST AC-2(7)-compliant + property tested.

**Persona 2 — Auditor SOC 2 Type II + ISO 27001**:
- Audit query: SOC 2 CC6.1 (logical access controls) + CC6.7 (change management); ISO 27001 A.5.15 (privileged access).
- Evidence: D1 `admin_op_log` queryable + `admin_op_log_collusion` index; CTRL-AUDIT-003 (MFA attestation embedded); 11 sign-offs canonical PRR doc.

**Persona 3 — Internal admin executing destructive op**:
- CLI flow: `corelink-admin tenant-tombstone --tenant-id X --approver bob@hugr` → CLI obtains approver session token → HMAC sig generated client-side → API call.
- Error UX: `denied_collusion_rotation` com explanation ("approver appeared in last 2 destructive ops in 24h; need a 3rd distinct admin"); admin must wait or recruit 3rd approver.

**SLA addendum**:
- Dual-approval verify latency: ≤ 50ms p99 (incl. D1 query for collusion).
- Collusion-rotation window: 24h (configurable via ADR if business case).
- Audit emission: synchronous within request lifecycle (atomic batch).
- Property test cadence: 10k iter PR + 100k iter nightly.

## 4. Capability Mapping

- **CAP-ADMIN-002** (Dual-approval workflow PAT-DUAL-APPROVAL-001) — IMPLEMENTA primary.
- **CAP-ADMIN-006** (Admin op audit-rich event) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.2 + §5.6` + `security_model.md §6.1 (CTRL-AUTH-010)` + `security_model.md §6.8 (CTRL-AUDIT-003)` + `resilience_patterns.md §3.7 (PAT-DUAL-APPROVAL-001)` + `invariant_registry.md §3.12 (INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS)`.

## 5. Tipo

Worker middleware + admin API + audit emission; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-dual-approval/`**:
   - `DualApprovalGate` trait + impl `DualApprovalGateImpl`.
   - `AdminOpRequest` + `VerifiedApproval` + `AdminOpType` enum + `DualApprovalError` enum.
   - HMAC-SHA256 signature verify via admin signing key (per-region, fetched from rotation worker WI-S13-003 cache).
   - Constant-time compare via `subtle::ConstantTimeEq`.
   - D1 separation-of-duties check.
   - D1 collusion-rotation 3-cycle check.
   - D1 nonce replay protection.
   - Clock-skew tolerance ≤ 60s (signed timestamp from Clerk IdP).

2. **Crate `corelink-admin-api/`**:
   - `POST /v1/admin/ops` endpoint + handler dispatcher per `AdminOpType`.
   - Tower middleware composition: auth → mfa_freshness → dual_approval → op_dispatch → audit_emit.
   - `AdminOpType` handlers (foundational; each op maps to existing CAP):
     - `ConfigRollback` → forwards to WI-S13-001 rollback (composed).
     - `RetentionPolicyReduce` → forwards to WI-S13-001 config update + audit.
     - `FeatureFlagDisable` → forwards to WI-S13-001 config update + audit.
     - `SecretRotationStart` → forwards to WI-S13-003 rotation worker.
     - `TenantTombstone` → forwards to S-11 erasure + audit.
   - Schema validation pre-dispatch.

3. **D1 schema migration**:
   - `admin_op_log` table (vide §1).
   - Index `idx_admin_op_log_collusion` for collusion-rotation query optimization.
   - UNIQUE `(caller_user_id, nonce)` for replay protection.

4. **CloudEvent audit emission rich** (per CAP-ADMIN-006):
   - Event type: `corelink.admin.op.executed` (success) | `corelink.admin.op.denied` (failure variants).
   - Payload (CloudEvents v1.0.2 envelope):
     ```json
     {
       "specversion": "1.0",
       "id": "<uuid>",
       "source": "/corelink/admin/{region}",
       "type": "corelink.admin.op.executed",
       "time": "<RFC 3339>",
       "data": {
         "actor": {"user_id": "<uuid>", "email_hash": "<sha256>"},
         "mfa_ts_ms": 0,
         "dual_approver": {"user_id": "<uuid>", "email_hash": "<sha256>"},
         "op_type": "<enum>",
         "op_payload_hash": "<sha256-hex>",
         "prev_state_hash": "<sha256-hex>",
         "nonce": "<hex>",
         "outcome": "approved",
         "signature": "<hmac-sha256-hex>"
       }
     }
     ```
   - Atomic batch with admin_op_log INSERT (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER herdada S-03).
   - Chain integrity hash (INV-AUDIT-APPEND-ONLY herdada S-09).

5. **Métricas underscored Prometheus** (per `observability_model.md §3.1 + §4.1`; label `plan` aplicável):
   - `corelink_admin_dual_approval_total{outcome,plan}` (outcome ∈ ok|missing_approver|sig_invalid|caller_eq_approver|collusion_rotation_violation|mfa_stale|approver_not_admin|nonce_replay|clock_skew).
   - `corelink_admin_dual_approval_verify_duration_seconds_bucket` (histogram p50/p95/p99).
   - `corelink_admin_op_executed_total{op_type,plan}`.
   - `corelink_admin_op_denied_total{op_type,reason,plan}`.
   - `corelink_admin_collusion_rotation_distinct_approvers_gauge{window="24h"}`.

6. **Observability** — trace span `admin.dual_approval.verify` + `admin.op.dispatch` + `admin.op.audit_emit` com attributes:
   - `admin.caller_user_id` (UUID).
   - `admin.approver_user_id` (UUID).
   - `admin.op_type` (enum).
   - `admin.outcome` (enum).
   - `admin.collusion_recent_approvers_uuids` (vec<uuid>; last 2 distinct destructive approvers in 24h window — proposed approver MUST NOT be in this set).
   - `result` (enum).

7. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_dual_approval_missing_approver_rejected`: 10k random requests sem `X-Dual-Approver`; assert 100% 403 MissingApprover.
   - `prop_dual_approval_sig_invalid_rejected`: 10k random byte mutations em signature; assert 100% rejection.
   - `prop_dual_approval_caller_eq_approver_rejected`: 10k requests com caller == approver; assert 100% 403 CallerEqualsApprover.
   - **`prop_dual_approval_collusion_rotation_a_b_a_b`** (NIST AC-2(7) primary, Lote 10.13 canonical strengthening): synthesize 3-op sequence A→approve_B, B→approve_A, A→approve_B; assert 1st + 2nd ops succeed; **3rd op REJECTS** (proposed approver=B ∈ {A, B} = last 2 distinct prior approvers); 10k variations of timing + ordering. Oracle prévio (LIMIT 3 prior + count distinct < 3) detectava só na 4ª op — current implementation rejects logo na 3ª op.
   - `prop_dual_approval_collusion_rotation_3_distinct_passes`: 10k 3-op sequences A→approve_B, B→approve_C, C→approve_A (3 distinct approvers em rolling window); assert 100% pass.
   - `prop_dual_approval_mfa_stale_rejected`: 10k requests with mfa_ts > 30 min ago; 100% 401.
   - `prop_dual_approval_nonce_replay_rejected`: 10k replay same nonce; 1st accept, 2nd+ reject.
   - `prop_dual_approval_clock_skew_rejected`: 10k requests with ts > 60s skew; 100% rejection.

8. **Adversarial regression tests** (5+ scenarios — INCLUDING collusion-rotation A→B/B→A canonical):
   - **Collusion A→approve B / B→approve A / A→approve B (3-cycle, Lote 10.13 canonical strengthening)**: synthesize sequence; ops 1-2 succeed; 3rd op (approver=B) REJECTED via collusion-rotation rolling oracle (proposed approver=B ∈ {A, B} = last 2 distinct prior approvers; rule: rolling 3-op window must have 3 distinct approvers — equivalente a `proposed approver ∉ last 2 distinct approvers in 24h window`).
   - HMAC signature forge (random byte) → rejected via constant-time compare.
   - Direct D1 INSERT to admin_op_log bypass attempt → rejected (Worker binding isolation).
   - Approver privilege revoked between sig + verify → rejected via runtime admin_role check.
   - Replay attack with old nonce → rejected via D1 UNIQUE constraint.
   - Clock skew injection > 60s → rejected.
   - MFA timestamp forge (unsigned) → rejected (Clerk JWT claim verified).

9. **Integration test E2E**:
   - Real admin op via API: 2 distinct admins (caller + approver); dual-approval succeeds; audit emission verified; D1 admin_op_log row + audit_outbox row + chain hash unbroken.
   - Negative: missing approver → 403 + audit.denied event.
   - Negative: caller==approver → 403 + audit.denied event.
   - Negative: collusion-rotation A↔B 3-cycle → 3rd op rejected (proposed approver IN last 2 distinct approvers; oracle Lote 10.13 canonical).

10. **Chaos test** (S-13 ship gate also covered em WI-S13-006):
    - Missing approver injected; verify 403 + audit + Slack alert.
    - HMAC signing key rotation in-flight (24h overlap); verify both old + new keys accepted; post-grace old rejected.

### 6.2 Out-of-scope (deferred)

- **DO config-singleton + rollback API**: WI-S13-001 (composed; this WI consumes).
- **Secret rotation worker (admin signing key)**: WI-S13-003 (this WI consumes signing key from rotation cache).
- **Terraform drift detection**: WI-S13-004.
- **Progressive rollout controller**: WI-S13-005.
- **Property tests 10k full + RB dry-runs + PRR doc**: WI-S13-006.
- **Admin UI panel** (web): S-16.
- **SCIM/SAML enterprise SSO**: S-19.
- **Approver delegation** (e.g., A delegates approval right to B): pós-GA enterprise.
- **Multi-tier approval** (e.g., 3-of-5 for highest risk ops): pós-GA enterprise; current = 2 distinct.

## 7. Anti-Scope

- Advisory mode / "warn but allow" path (anti-pattern; never).
- Skip caller≠approver D1 hard-check.
- Skip collusion-rotation defense (NIST AC-2(7) compliance baseline).
- Env-gated bypass for staging/dev (production-equivalent enforcement).
- HMAC signing key as PAT (PAT format canonical hybrid per S-03; admin sig is separate signing key from rotation worker).
- Long-lived admin sessions (MFA freshness 30 min hard-check).
- Skip nonce replay protection.
- Skip clock-skew tolerance bound (≤ 60s).
- Direct D1 INSERT to admin_op_log (Worker binding only).
- Skip audit emission fail-closed (atomic batch mandatory).
- Approve op pendente (no async approval workflow at GA; sync only).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Admin API + dual-approval enforcement + collusion-rotation defense

  Background:
    Given admin role + WebAuthn step-up + MFA fresh ≤ 30 min for caller
    Given admin signing key (per-region) fetched from rotation worker
    Given D1 admin_op_log + audit_outbox tables operational

  Scenario: Successful destructive op with valid dual-approval
    Given caller A + approver B (distinct + admin role + active)
    Given last 3 ops/24h had 3 distinct approvers (or fewer than 3 ops)
    When POST /v1/admin/ops body={op_type: ConfigRollback, op_payload: ...}
      And X-Dual-Approver: B
      And X-Approver-Signature: <valid HMAC>
    Then response 200 (op executed)
    And metric corelink_admin_dual_approval_total{outcome="ok"} incremented
    And D1 admin_op_log row inserted (outcome="approved")
    And audit_outbox row inserted (admin.op.executed event)
    And chain hash unbroken

  Scenario: Missing X-Dual-Approver header rejected
    When POST /v1/admin/ops sem X-Dual-Approver header
    Then response 403 DualApprovalError::MissingApprover
    And metric corelink_admin_dual_approval_total{outcome="missing_approver"} incremented
    And audit "admin.op.denied_missing" emitted

  Scenario: Approver signature HMAC invalid rejected
    When POST /v1/admin/ops with X-Approver-Signature mutated 1 byte
    Then response 403 DualApprovalError::SignatureInvalid
    And metric corelink_admin_dual_approval_total{outcome="sig_invalid"} incremented
    And audit "admin.op.denied_sig" emitted

  Scenario: Caller equals approver rejected (separation of duties)
    Given caller A
    When POST /v1/admin/ops with X-Dual-Approver: A
    Then response 403 DualApprovalError::CallerEqualsApprover
    And metric corelink_admin_dual_approval_total{outcome="caller_eq_approver"} incremented
    And audit "admin.op.denied_caller_eq" emitted

  Scenario: Collusion-rotation A→B/B→A defense (NIST AC-2(7))
    Given last 3 destructive ops/24h: A→approve_B, B→approve_A, A→approve_B (count distinct = 2: {A, B})
    When admin B submits op with X-Dual-Approver: A (4th op)
    Then response 403 DualApprovalError::CollusionRotation { distinct: 2 }
    And metric corelink_admin_dual_approval_total{outcome="collusion_rotation_violation"} incremented
    And audit "admin.op.denied_collusion" emitted
    And SEV-2 alert fires

  Scenario: Collusion-rotation 3 distinct approvers passes
    Given last 3 destructive ops/24h: A→approve_B, C→approve_D, E→approve_F (count distinct = 3)
    When admin G submits op with X-Dual-Approver: H
    Then response 200 (op executed, no collusion violation)

  Scenario: MFA stale rejected (>30 min)
    Given caller MFA timestamp 31 min ago
    When POST /v1/admin/ops
    Then response 401 DualApprovalError::MfaStale
    And metric corelink_admin_dual_approval_total{outcome="mfa_stale"} incremented
    And client must force re-MFA

  Scenario: Approver lacks admin role rejected
    Given approver B revoked admin role 5 min ago
    When POST /v1/admin/ops with X-Dual-Approver: B
    Then response 403 DualApprovalError::ApproverNotAdmin
    And metric corelink_admin_dual_approval_total{outcome="approver_not_admin"} incremented

  Scenario: Nonce replay rejected
    Given previous request with nonce N succeeded
    When same caller submits new request with same nonce N
    Then response 403 DualApprovalError::NonceReplay
    And metric corelink_admin_dual_approval_total{outcome="nonce_replay"} incremented

  Scenario: Clock skew > 60s rejected
    Given request ts_ms diverges from server now by > 60s
    When POST /v1/admin/ops
    Then response 403 DualApprovalError::ClockSkew
    And metric corelink_admin_dual_approval_total{outcome="clock_skew"} incremented

  Scenario: HMAC signing key rotation overlap accepted
    Given key rotation in-flight (24h overlap window per key_management.md §3.2.1)
    Given approver signature using old key (still in overlap)
    When POST /v1/admin/ops
    Then response 200 (verify accepts old + new keys)
    And metric corelink_admin_dual_approval_total{outcome="ok"} incremented

  Scenario: Audit emission fail-closed (D1 atomic batch)
    Given D1 audit_outbox INSERT injected failure
    When POST /v1/admin/ops with valid dual-approval
    Then admin_op_log INSERT rolled back
    And response 503 (op not executed)
    And no audit chain break

  Scenario: Property test collusion-rotation 10k green
    Given prop_dual_approval_collusion_rotation_a_b_a_b 10k iter
    When test runs nightly
    Then 0 false-accepts
    And 0 panics
```

## 9. Design Decisions

### 9.1 Why HMAC-SHA256 admin signing key (NÃO PAT format)

- PAT format canonical hybrid `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` per S-03 cycle 9 SEAL decision (a) — purpose-built for customer/tenant authentication.
- Admin signing key é separate purpose: dual-approval HMAC sig over `op_payload || nonce || ts`.
- Per-region key (rotation 24h overlap per `key_management.md §3.2.1`); managed by rotation worker WI-S13-003.
- Constant-time compare via `subtle::ConstantTimeEq` herdada S-03 pattern.

### 9.2 Why hard-fail 403 (NÃO advisory)

- Advisory mode = bypassable by ignoring warning; defeats purpose.
- 403 hard-fail = forced mitigation pre-execution.
- INV-ADMIN-DUAL-APPROVAL CRITICAL severity em registry §3.12.

### 9.3 Why collusion-rotation 3-cycle (N=3, M=24h, K=3)

- NIST SP 800-53 AC-2(7) recommends "rotation tracking" but doesn't specify N/M/K.
- N=3, K=3: minimum to detect A↔B reciprocal pattern (count distinct = 2 in 3 ops).
- M=24h: balance operational tempo (ops cluster) vs detection window.
- Configurable via ADR if business case (e.g., enterprise customer requiring 5-cycle).

### 9.4 Why 30 min MFA freshness window

- CTRL-AUTH-010 baseline (`security_model.md §6.1`).
- 30 min balances UX (admin frequent ops) vs security (stale MFA = persistent session hijack vector).
- Hard-check enforced em middleware; expiry = 401 force re-MFA (no grace).

### 9.5 Why nonce replay protection via D1 UNIQUE

- Replay attack = trivial without nonce; each request has 128-bit random nonce.
- D1 UNIQUE `(caller_user_id, nonce)` rejects 2nd insert.
- Storage cost bounded: ~100 ops/dia × 30d × 16 bytes = 48 KB; negligible.

### 9.6 Why CloudEvents v1.0.2 envelope

- Industry standard; consumed by audit chain processor S-09.
- Self-describing schema; envelope versioned via `specversion`.
- Forward-compatible com S-16 admin UI dashboard.

### 9.7 Why D1 atomic batch (admin_op_log + audit_outbox)

- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER herdada S-03 — both succeed or both fail.
- Preserves INV-AUDIT-APPEND-ONLY chain integrity.
- Failure mode: response 503 (op not executed); no orphan state.

### 9.8 Why no async approval workflow at GA

- Sync 2-engineer presence required = stronger guarantee.
- Async ("approver clicks email link 5 min later") = slack window; future ADR if business case.
- GA tier: solo + team; both compatible with sync 2-engineer in-room/Slack.

### 9.9 Why ADR potencial?

- Sim — **ADR-XXXX**: "Dual-approval enforcement + collusion-rotation defense PAT-DUAL-APPROVAL-001 NIST AC-2(7) ratification". Decisão arquitetural foundational; reuse pattern em S-14 (BYOK rotation customer-trigger) + S-19 (enterprise multi-tier approval).
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s13.002.1** Property test 10k iter (PR) + 100k iter (nightly) `prop_dual_approval_collusion_rotation_a_b_a_b` → 0 false-accepts (EVT-002).
- [ ] **10.s13.002.2** Property test 7 props (missing/sig/caller_eq/collusion/mfa/nonce/clock_skew) 10k iter green (EVT-002).
- [ ] **10.s13.002.3** Adversarial test 7+ scenarios (collusion 3-cycle + HMAC forge + direct D1 bypass + privilege drift + replay + clock skew + MFA forge) — 100% rejection (EVT-040).
- [ ] **10.s13.002.4** E2E test: 2 admins distinct succeeds; collusion A↔B/B↔A rejects 4th op (EVT-018).
- [ ] **10.s13.002.5** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) em `corelink-dual-approval` + `corelink-admin-api` crates (EVT-002).
- [ ] **10.s13.002.6** SLO-ADMIN-DUAL-APPROVAL-LATENCY ≤ 50ms p99 sustained 30d staging *(GA Evidence Gate D+45)*.
- [ ] **10.s13.002.7** INV-ADMIN-DUAL-APPROVAL ratified em registry §3.12 (already present); CI gate ativo (EVT-022).
- [ ] **10.s13.002.8** OWASP ASVS V4 (auth) + V5 (validation) + V14 (configuration) 100% checklist pass (EVT-002).
- [ ] **10.s13.002.9** NIST SP 800-53 AC-2(7) compliance attestation em PRR doc (EVT-031).
- [ ] **10.s13.002.10** Cost regression gate: D1 admin_op_log + audit_outbox writes ≤ $10/mês.

## 11. DoD

- [ ] Crates `corelink-dual-approval` + `corelink-admin-api` compilam (workspace).
- [ ] `DualApprovalGate` trait implemented; HMAC verify via `subtle::ConstantTimeEq`.
- [ ] All 12 Gherkin scenarios green em integration test.
- [ ] Property tests 7 props × 10k iter green em PR; 100k iter green em nightly.
- [ ] Adversarial regression tests 7+ scenarios green.
- [ ] E2E test 2-admin succeed + collusion A↔B reject green.
- [ ] D1 schema migration applied (`admin_op_log` + index + UNIQUE).
- [ ] CloudEvent audit emission rich verified em DASH-ADMIN.
- [ ] Métricas emitidas (5 listadas §6.1.5).
- [ ] Trace spans `admin.dual_approval.verify|op.dispatch|op.audit_emit` em OTel pipeline.
- [ ] rustdoc + 3 examples (basic 2-admin op, collusion-rotation example, audit emission inspection).
- [ ] ADR-XXXX (dual-approval + collusion-rotation NIST AC-2(7)) escrito + ratificado.
- [ ] Code review (Architect + Crypto SME folded + Security Lead + AppSec).
- [ ] PRR Architect mini-sign-off (ship gate é WI-S13-006).
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-ADMIN-DUAL-APPROVAL** (HIGH — registry §3.12; ALREADY PRESENT): este WI implementa primary; property test 10k green incluindo collusion-rotation 3-cycle scenario.
- **INV-ADMIN-MFA-FRESHNESS** (HIGH — registry §3.12; ALREADY PRESENT): middleware enforce em todos paths; expiry test green.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — registry §3.6 herdada S-09): admin op CloudEvent append-only; chain hash unbroken.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada S-03): D1 atomic batch [admin_op_log + audit_outbox] fail-closed.
- **INV-AUTH-CLOCK-SKEW-BOUND** (HIGH — registry §3.14 herdada S-03): clock-skew tolerance ≤ 60s reused.
- **INV-AUDIT-NO-RAW-PII** (CRITICAL — registry §3.14 herdada S-03): email_hash em audit; raw email never persisted.

### Novas

Nenhuma (INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS já presentes em registry §3.12 do spec contract sprint setup; este WI implementa primary).

TLA+ alignment: registry §4.2 indica `key_lifecycle.tla` (PLANNED S-13) covers `InvCallerNeqApprover` action — escopo S-13 mas implementação TLA spec é S-09 ou S-12 forward; integration test cross-validates.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DualApprovalGate trait + impl | `crates/corelink-dual-approval/src/lib.rs` | Rust |
| AdminOpRequest + types | `crates/corelink-dual-approval/src/types.rs` | Rust |
| DualApprovalError enum | `crates/corelink-dual-approval/src/error.rs` | Rust |
| HMAC verify (subtle ct compare) | `crates/corelink-dual-approval/src/hmac_verify.rs` | Rust |
| Collusion-rotation D1 query | `crates/corelink-dual-approval/src/collusion.rs` | Rust |
| Admin API endpoint + dispatcher | `crates/corelink-admin-api/src/handlers.rs` | Rust |
| Tower middleware composition | `crates/corelink-admin-api/src/middleware/dual_approval.rs` | Rust |
| D1 migration `admin_op_log` | `migrations/0XX_admin_op_log.sql` | SQL |
| Property tests | `crates/corelink-dual-approval/tests/prop_dual_approval.rs` | Rust |
| Adversarial regression tests | `crates/corelink-dual-approval/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_admin_dual_approval.rs` | Rust |
| ADR-XXXX (dual-approval + collusion-rotation) | `specs/03_architecture/adrs/ADR-XXXX-dual-approval-collusion-rotation.md` | Markdown |
| Examples | `crates/corelink-admin-api/examples/` (basic_2admin.rs, collusion_demo.rs, audit_inspection.rs) | Rust |

## 14. Quality Standards SOTA

- **14.s13.002.1** Zero `unsafe`; zero `unwrap` em src/ (allow em tests).
- **14.s13.002.2** rustdoc 100% public API + 3 examples.
- **14.s13.002.3** Test coverage ≥ 90% (`cargo tarpaulin`).
- **14.s13.002.4** Latência: dual-approval verify ≤ 50ms p99 (incl. D1 query for collusion); HMAC compare ≤ 100µs (constant-time).
- **14.s13.002.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s13.002.6** Métricas RED + per-outcome breakdown.
- **14.s13.002.7** Runbook RB-FM-205 (admin mistake) referenciado WI-S13-006.
- **14.s13.002.8** Breaking changes em `AdminOpType` enum = bump major + ADR + migration plan.
- **14.s13.002.9** Memory: bounded em verify path; constant-time enforced.
- **14.s13.002.10** Cost regression gate em CI.
- **14.s13.002.11** NIST SP 800-53 AC-2(7) attestation em PRR doc.
- **14.s13.002.12** Constant-time HMAC compare via `subtle::ConstantTimeEq`.

## 15. Chaos Experiments

1. **Collusion A→B/B→A 3-cycle synthesis**: red team synthesizes 4-op sequence; assert 4th rejected via collusion-rotation; metric + audit emit verified.

2. **HMAC signing key rotation in-flight**: simulate key rotation 24h overlap (key_management.md §3.2.1); verify both old + new keys accepted; post-grace old rejected.

3. **Approver privilege revoked between sig + verify**: race condition — approver had role at sig time, revoked before request lands; runtime admin_role check rejects.

4. **Direct D1 INSERT bypass attempt**: red team tries to write admin_op_log directly bypassing API; verify Worker binding isolation rejects.

5. **MFA timestamp forge**: attacker forges unsigned `mfa_ts_ms`; verifier checks Clerk JWT claim signed; forge rejected.

6. **Replay attack with old nonce**: capture valid request; replay; D1 UNIQUE rejects.

7. **Audit emission fail-closed**: inject D1 audit_outbox INSERT failure; verify admin_op_log INSERT rolled back; response 503.

8. **Property test fuzz reaching 100k iter**: nightly job runs 100k iter; 0 false-accepts, 0 panics.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-13 ship gate é WI-S13-006; este WI passa por mini-PRR Architect + Crypto SME + Security Lead + AppSec review):

- [ ] All 12 Gherkin scenarios green.
- [ ] Property tests + adversarial tests green (incl. collusion A↔B 3-cycle).
- [ ] E2E 2-admin succeed + collusion reject green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-ADMIN.
- [ ] ADR-XXXX (dual-approval + collusion-rotation) published.
- [ ] Crypto SME review (HMAC signing key derivation + constant-time compare + audit chain).
- [ ] AppSec review (admin API surface + privilege escalation + insider threat model).
- [ ] Architect approval (composition with WI-S13-001 rollback gate).
- [ ] OWASP ASVS V4 + V5 + V14 100% pass.
- [ ] NIST SP 800-53 AC-2(7) attestation documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate `corelink-dual-approval/` scaffold + types | 1.5h |
| ST-002 | HMAC verify (subtle constant-time) + clock-skew tolerance | 2h |
| ST-003 | D1 separation-of-duties check | 1h |
| ST-004 | D1 collusion-rotation 3-cycle query + index | 2h |
| ST-005 | D1 nonce replay protection (UNIQUE) | 1h |
| ST-006 | Crate `corelink-admin-api/` endpoint + dispatcher | 2h |
| ST-007 | Tower middleware composition (auth → mfa → dual_approval → dispatch → audit) | 1.5h |
| ST-008 | CloudEvent audit emission rich + atomic batch | 2h |
| ST-009 | D1 migration + index + UNIQUE | 1h |
| ST-010 | Métricas emit (5 metrics) + trace spans | 1.5h |
| ST-011 | Property tests 7 props × 10k iter | 3h |
| ST-012 | Adversarial regression tests 7+ scenarios | 2h |
| ST-013 | E2E integration test 2-admin + collusion | 2h |
| ST-014 | Chaos test (collusion synthesis + key rotation overlap) | 1.5h |
| ST-015 | rustdoc + 3 examples | 1h |
| ST-016 | ADR-XXXX redação | 1.5h |
| ST-017 | Code review (Architect + Crypto SME + Security Lead + AppSec) iteration | 2h |

**Total Optimistic**: ~28h. **PERT** (O=12h, M=18h, P=30h, per spec contract §12): **19.0h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **WI-S13-001 SEALED** (DO config-singleton + rollback API endpoint; this WI adds dual-approval middleware on top).
- **S-03 SEALED** (admin role + WebAuthn + MFA freshness Clerk JWT claim signed; CTRL-AUTH-010 enforced; INV-AUTH-CLOCK-SKEW-BOUND herdada).
- **S-09 SEALED** (audit chain hash + atomic batch pattern + INV-AUDIT-APPEND-ONLY).

### Soft blockers

- **WI-S13-003** (admin signing key rotation 24h overlap) — soft because this WI initially uses static key for dev; production deploy requires rotation worker operational.

### Outbound

- **WI-S13-001** rollback API requires this WI's dual-approval middleware (composed).
- **WI-S13-003** secret rotation start operation = AdminOpType (composed).
- **WI-S13-005** progressive rollout pause/resume = AdminOpType (composed).
- **WI-S13-006** PRR ship gate gates S-13 close.

## 19. Effort PERT

O: 12h, M: 18h, P: 30h → PERT **19.0h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit**. If exceeded → escalation: split em sub-WI (verify gate vs admin API + audit emit).

## 21. Observability

5 métricas listadas §6.1.5. Trace spans `admin.dual_approval.verify|op.dispatch|op.audit_emit` com attributes:
- `admin.caller_user_id` (UUID)
- `admin.approver_user_id` (UUID)
- `admin.op_type` (enum)
- `admin.outcome` (enum)
- `admin.collusion_distinct_count` (u8)

Logs structured JSON; nivel INFO em ok, WARN em denied (any reason), ERROR em audit emission failure.

Dashboard widget DASH-ADMIN:
- Dual-approval pass/block ratio + reason breakdown.
- Collusion-rotation distinct approvers gauge (24h window).
- Op executed/denied per op_type.
- Verify latency p99.

## 22. Cost Analysis

- D1 admin_op_log: ~100 ops/dia × 30 = 3000 rows × 200 bytes = 600 KB/mês; ~$1/mês.
- audit_outbox emit: shared with S-09 audit chain processor.
- Worker invocations: ~100/mês × $0.15/M = negligible.
- D1 query overhead (collusion 3-cycle): ~10ms × 100/dia = bounded.
- **Total custo direto WI-S13-002**: ~$3/mês = $36/yr. Negligível vs alternative (external IAM service ~$200/mês).

## 23. API Contract

`DualApprovalGate` é internal Rust trait. HTTP API público:
- `POST /v1/admin/ops` body=`AdminOpRequest` → 200 (op executed) ou 403 (denied variants) ou 401 (mfa_stale) ou 503 (audit fail).
- Headers required: `Authorization: Bearer <admin-jwt>` + `X-Dual-Approver: <user_id>` + `X-Approver-Signature: <hmac-hex>` + `X-Approver-Nonce: <hex>` + `X-Request-Ts-Ms: <ms>`.

API semver stable post v1.0; breaking changes em `AdminOpType` enum = bump major + migration plan.

## 24. Post-mortem Hooks

- Dual-approval bypass detected (any bypass attempt) → CRITICAL post-mortem + Security incident response.
- Collusion-rotation violation in production (real ops, not chaos test) → CRITICAL + access review.
- Audit emission failure rate > 0.1% sustained → SEV-2 + ops post-mortem.
- HMAC signing key compromise suspected → CRITICAL + emergency rotation.
- MFA bypass via timestamp manipulation detected → CRITICAL + auth model review.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker (dual-approval middleware disabled = ALL admin ops 403; rollback = re-enable).
- D1 schema rollback: only additive (no DROP); INV-AUTH-MIGRATION-ADDITIVE herdada S-03.
- Approval session recovery: nonce TTL bound 5 min; expired nonces auto-cleaned via D1 cron.
- RTO: ≤ 10 min (revert deploy).
- RPO: 0 (audit chain unbroken).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: HMAC signing key (rotated 24h overlap) + caller MFA fresh + approver admin role active runtime check.
- **Tampering**: signature HMAC-SHA256 over `op_payload || nonce || ts`; D1 atomic batch fail-closed; chain hash unbroken.
- **Repudiation**: CloudEvent rich (`actor + mfa_ts + dual_approver + op_payload + prev_state_hash + signature`) — forensic-grade evidence.
- **Information disclosure**: email_hash em audit (not raw email); op_payload pseudo-public (not secrets).
- **DoS**: HMAC verify ≤ 100µs constant-time; D1 query bounded.
- **Elevation of privilege**: dual-approval gate + collusion-rotation defense + privilege drift runtime check.

**LINDDUN delta**:
- **Linkability**: caller + approver em audit é necessário (compliance accountability).
- **Identifiability**: user_id em D1 + audit (intentional CTRL-AUDIT-002).
- **Non-repudiation**: HMAC signature + WebAuthn attestation embedded em audit.
- **Detectability**: collusion-rotation = anomaly detection compute; DASH-ADMIN dashboard surface.
- **Disclosure of information**: op_payload pseudo-public; secrets em separate path.
- **Unawareness**: admin onboarding training documents dual-approval flow + collusion-rotation explanation.
- **Non-compliance**: SOC 2 CC6.1/CC6.7 + ISO 27001 A.5.15/A.5.16 + NIST SP 800-53 AC-2(1) + AC-2(7) + LGPD Art. 38 + GDPR Art. 32 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-dual-approval/README.md` — overview + integration pattern.
- ADR-XXXX — dual-approval + collusion-rotation NIST AC-2(7) ratification.
- Doc `docs/internal/admin-plane.md` (extends WI-S13-001) — sequence diagram dual-approval flow + collusion-rotation algorithm.
- Workshop interno (1.5h) com Crypto SME (folded Architect) + Security Lead + AppSec pós-merge.
- Onboarding test (5 questions): caller≠approver, collusion-rotation 3-cycle math, MFA freshness window, HMAC verify, audit chain integrity.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Dual-approval bypassed via bug | L | M | CRITICAL (insider threat) | M | LOW | Hard-check enforced + property test 10k + chaos test + audit detect |
| R-002 | Collusion A↔B reciprocal | L | H | CRITICAL | M | LOW | Collusion-rotation 3-cycle defense NIST AC-2(7) + property test |
| R-003 | HMAC signing key compromise | L | H | CRITICAL | M | LOW | Rotation 24h overlap (WI-S13-003) + Cloudflare Workers Secrets storage |
| R-004 | MFA bypass via timestamp manipulation | L | H | CRITICAL | M | LOW | Clerk JWT claim signed + clock-skew ≤ 60s + INV-AUTH-CLOCK-SKEW-BOUND |
| R-005 | Approver privilege drift (runtime) | M | M | HIGH | M | LOW | Runtime admin_role check + revocation propagation ≤ 60s herdada S-03 |
| R-006 | Replay attack | L | M | MEDIUM | L | LOW | Nonce 128-bit + D1 UNIQUE + clock-skew bound |
| R-007 | Direct D1 INSERT bypass | L | H | CRITICAL | L | LOW | Cloudflare platform binding isolation; Worker-only access |
| R-008 | Audit emission failure orphan | L | M | HIGH | L | LOW | D1 atomic batch fail-closed + chaos test |
| R-009 | Latency regression (collusion query) | L | L | LOW | L | LOW | D1 index optimized + benchmark + cost regression gate |
| R-010 | Property test flakiness | M | L | LOW | L | LOW | Deterministic seeds + retry policy |
| R-011 | NIST AC-2(7) thresholds drift (N/M/K) | L | L | MEDIUM | L | LOW | ADR review cadence quarterly + business case driven |
| R-012 | Async approval workflow demand (post-GA) | M | L | LOW | L | LOW | ADR forward-compatible; sync-only at GA |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME folded review HMAC verify + collusion-rotation algorithm.
2. **Code (D+1)**: peer review + Crypto SME pair-program adversarial tests.
3. **Security (D+1)**: Security Lead review threat model + privilege escalation paths.
4. **AppSec (D+2)**: AppSec review admin API surface + insider threat scenarios.
5. **Property test (pre-merge D+2)**: 10k iter green em PR; 100k iter green em nightly.
6. **Adversarial (pre-merge D+3)**: red team session — collusion synthesis + HMAC forge + privilege drift.
7. **PRR mini (D+3)**: Architect + Security Lead + AppSec sign-off (gates inclusion em WI-S13-006 ship gate).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization mandatory (HMAC signing key derivation + constant-time compare + audit chain integration + collusion-rotation algorithm review)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — NIST AC-2(7) compliance + insider threat model + privilege escalation_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — verify SLO ≤ 50ms + audit emission fail-closed_ | _pending_ | _pending_ |
| 6 | Engineer (S-13 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.1/CC6.7 + ISO 27001 A.5.15/A.5.16 + NIST AC-2(1)/AC-2(7) attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 38 + GDPR Art. 32 audit log review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — admin API surface threat model + insider threat scenarios + adversarial review_ | _pending_ | _pending_ |

> Crypto SME (HMAC signing key + constant-time compare + audit chain) folds into Architect role specialization mandatory. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S13-002 (cycle 12.S13.0). |

## 32. Anti-patterns evitados

- Advisory mode / "warn but allow" path.
- Skip caller≠approver hard-check.
- Skip collusion-rotation defense (NIST AC-2(7) baseline).
- Env-gated bypass for dev/staging.
- HMAC signing key as PAT (separate purpose; PAT format canonical hybrid per S-03).
- Long-lived admin sessions (MFA freshness 30 min hard).
- Skip nonce replay protection.
- Skip clock-skew tolerance bound.
- Direct D1 INSERT bypass.
- Skip audit emission fail-closed.
- Async approval workflow at GA (sync only; forward-compatible).
- Override flag em production.

---

**Fim WI-S13-002.** Próximo: WI-S13-003 (secret rotation worker — TDK + PAT signing keys + audit chain key + BYOK overlap canonical per asset).
