---
id: "ADR-S13-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s13", "admin-plane", "dual-approval", "collusion-rotation", "nist-ac-2-7", "high-risk"]
---

# ADR-S13-001: Dual-Approval Enforcement + Collusion-Rotation Defense (PAT-DUAL-APPROVAL-001, NIST AC-2(7))

## Status

ACTIVE — WI-S13-002 SEALED.

## Context

CoreLink admin plane handles destructive operations (tenant tombstone, retention
policy reduction, secret rotation start, feature flag disable, config rollback)
with global blast radius. Single-admin bypass = insider threat trivial. Two-admin
reciprocal collusion (A approves B's op, B approves A's op, repeat) bypasses
naive dual-approval.

NIST SP 800-53 AC-2(7) requires "privileged accounts managed with special
scrutiny" including rotation of privileged access and monitoring for unusual
activity patterns.

## Decision

Implement `PAT-DUAL-APPROVAL-001` with collusion-rotation defense (Lote 10.13
canonical oracle):

1. **Dual-approval hard-fail 403**: every destructive op requires
   `X-Dual-Approver: <uuid>` + `X-Approver-Signature: <hmac>` headers;
   missing or invalid = 403 (no advisory mode, no override, no env bypass).

2. **Separation of duties**: `caller != approver` enforced via D1 hard-check;
   no exception for dev/staging environments.

3. **Collusion-rotation oracle** (Lote 10.13 canonical strengthening):
   - Query: last 2 distinct `approver_user_id` values for approved destructive
     ops in tenant 24h window (D1 `admin_op_log`).
   - Rule: proposed approver MUST NOT be in that set.
   - Equivalent constraint: any rolling window of 3 destructive ops must have
     3 distinct approvers.
   - Stronger than prior LIMIT 3 + count(distinct) < 3 oracle that allowed
     A→B/B→A/A→B to succeed at op 3 (detected only at op 4).

4. **MFA freshness**: caller `auth_time` Clerk JWT claim ≤ 30 min; stale = 401
   force re-MFA (CTRL-AUTH-010).

5. **HMAC-SHA256 constant-time**: approver signature over
   `op_payload || nonce || ts_ms_be`; `subtle::ConstantTimeEq`; per-region
   signing key (WI-S13-003).

6. **Nonce replay**: D1 UNIQUE `(caller_user_id, nonce)` prevents replay.

7. **Clock-skew tolerance**: ≤ 60s deviation from server time (inherited from
   INV-AUTH-CLOCK-SKEW-BOUND, S-03).

8. **Audit fail-CLOSED**: D1 atomic batch `[admin_op_log INSERT +
   audit_outbox INSERT]`; failure → 503, op not executed
   (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).

## Rationale

### Why hard-fail 403 (not advisory)

Advisory mode is bypassable by ignoring warnings. Hard-fail forces mitigation
pre-execution. INV-ADMIN-DUAL-APPROVAL CRITICAL severity.

### Why collusion-rotation with N=2 recent approvers (not N=3)

LIMIT 2 = the minimum to detect A↔B reciprocal. With LIMIT 2, the 3rd op
proposal by A (who appeared as recent[0]) is rejected immediately. Prior
LIMIT 3 + count-distinct oracle allowed A→B/B→A/A→B to succeed (3 ops, 3
entries, count-distinct = 2 fails only after 3rd succeeds).

### Why 24h window (M=24h)

Balances operational tempo (multiple destructive ops may legitimately cluster
in a maintenance window) vs detection window for insider collusion attempts.
Configurable via ADR if enterprise customer requires shorter/longer window.

### Why per-region signing key (not workspace-wide)

Defense in depth: compromise of one region's key does not enable forged
signatures for other regions. Rotation worker (WI-S13-003) manages 24h overlap.

## Reuse pattern

- S-14 (BYOK rotation customer-trigger): same dual-approval gate as middleware.
- S-19 (enterprise multi-tier approval): extends N from 2 to K (configurable)
  via ADR; backward-compatible.

## Consequences

- Admin op latency: ≤ 50ms p99 (incl. D1 collusion query with index).
- Operational UX: `denied_collusion_rotation` error surfaces need-3rd-admin
  message to CLI operator.
- Rollout: sync 2-engineer presence required (no async approval at GA).
- Audit trail: every op (approved + denied) emitted as CloudEvent with rich
  chain-integrity payload (CAP-ADMIN-006).

## References

- WI-S13-002 §1–§9 (full spec).
- `invariant_registry.md §3.12` (INV-ADMIN-DUAL-APPROVAL, INV-ADMIN-MFA-FRESHNESS).
- `resilience_patterns.md §3.7` (PAT-DUAL-APPROVAL-001).
- `security_model.md §6.1` (CTRL-AUTH-010).
- NIST SP 800-53r5 AC-2(7).
- Lote 10.13 codex P0 collusion-rotation strengthening rationale.
