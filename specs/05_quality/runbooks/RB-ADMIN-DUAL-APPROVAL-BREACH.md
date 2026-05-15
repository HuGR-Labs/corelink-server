---
id: "RB-ADMIN-DUAL-APPROVAL-BREACH"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "slo", "admin", "dual-approval", "security", "s13", "r6-2"]
---

# RB-ADMIN-DUAL-APPROVAL-BREACH — Dual-Approval Gate Latency Breach

> **SLO covered:** SLO-ADMIN-DUAL-APPROVAL-LATENCY (§4.15) — target ≤ 30s p95 for dual-approval gate handling (caller submit → approver sign → admin_op_log).
> **Invariant:** INV-ADMIN-DUAL-APPROVAL CRITICAL (S-13).
> **WI:** WI-S13-002.

## Symptom

- PagerDuty SEV-2 page `corelink-slo-admin-dual-approval-latency` fires.
- Operator dashboard: dual-approval gate showing requests waiting > 30s.
- Admin user complaint: "I submitted a destructive op 2 min ago and it hasn't been approved/rejected" — could indicate gate stuck OR collusion oracle blocking.

## Detection

```promql
# p95 gate handling time
histogram_quantile(0.95,
  sum by (le) (rate(corelink_admin_dual_approval_gate_seconds_bucket[5m])))
  > 30

# Pending approvals beyond SLO
count(corelink_admin_dual_approval_pending{age_seconds > 30})
```

Audit query:

```sql
SELECT id, caller, approver, submitted_at, decided_at,
       (julianday(decided_at)-julianday(submitted_at))*86400 AS latency_s
  FROM admin_dual_approval_log
 WHERE submitted_at > datetime('now', '-30 minutes')
   AND (decided_at IS NULL OR
        (julianday(decided_at)-julianday(submitted_at))*86400 > 30);
```

## Immediate mitigation (≤ 5 min)

1. **Confirm signal**: are pending requests stuck because of:
   - (a) MFA freshness check failing for approver (CTRL-AUTH-010)?
   - (b) HMAC verify failing (signing key issue)?
   - (c) Collusion oracle blocking (CTRL-ADMIN-002 detected same-identity caller=approver)?
   - (d) Network/queue between caller submit and approver notification slow?
2. **For stuck requests** with legitimate signature: page secondary approver via PagerDuty escalation.
3. **DO NOT bypass dual-approval** under any circumstance. SLO breach is annoying; bypassing is security-catastrophic.
4. **If the gate itself is broken** (process bug, not approver lag): freeze destructive admin ops globally:
   ```bash
   wrangler secret put ADMIN_DESTRUCTIVE_OPS_FROZEN --env prod  # value: "true"
   ```
   This stops new destructive ops from queueing; existing pending ops still need explicit decision.

## Root-cause investigation (15–30 min)

1. **HMAC verify failures**:
   ```promql
   sum by (reason) (rate(corelink_admin_dual_approval_signature_fail_total[5m]))
   ```
   Top reasons: `clock_skew`, `key_unknown`, `signature_invalid`, `mfa_expired`.
2. **Clock skew** (FM-350): `corelink_clock_skew_seconds` on caller / approver / gate worker.
3. **MFA freshness**: approver session expired? Step-up token revoked recently?
4. **Collusion oracle**: query rejected requests with `reason='caller_equals_approver'` — anyone trying to self-approve?
5. **Signing key health**: rotation in progress? If yes → see `RB-ADMIN-ROTATION-GAP`.
6. **Approver availability**: was the on-call approver paged? Did they ack? `pd-cli incidents -p`.
7. **Submit-to-notify latency**: Slack/PagerDuty webhook for approver delivery time.

## Rollback / recovery

| Cause                                      | Recovery                                                                          |
|--------------------------------------------|-----------------------------------------------------------------------------------|
| Approver MFA expired                       | Approver re-authenticates via WebAuthn step-up; pending request retried           |
| HMAC clock-skew                            | Sync NTP on affected hosts; recompute HMAC                                        |
| AdminSigning key in rotation overlap window| Verify `key_set` includes both old + new keys during overlap; see `RB-ADMIN-ROTATION-GAP` |
| Approver unavailable (vacation, etc)       | Page secondary approver in PagerDuty rotation                                     |
| Webhook delivery failure (Slack outage)    | Direct PagerDuty page bypassing Slack; document for FM-153                        |
| Process bug (gate stuck even when valid)   | Rollback last deploy touching admin path; re-issue pending requests post-rollback |

Verify recovery:

```promql
# Gate latency returns to ≤ 30s p95
histogram_quantile(0.95,
  sum by (le) (rate(corelink_admin_dual_approval_gate_seconds_bucket[5m]))) < 30

# No pending > 30s
count(corelink_admin_dual_approval_pending{age_seconds > 30}) == 0
```

## Escalation path

| Time elapsed | Who                                | Criteria                                       |
|--------------|------------------------------------|------------------------------------------------|
| 0            | Primary SRE + Primary approver      | SEV-2 fires                                    |
| 5 min        | Secondary approver                  | Primary approver hasn't ack'd (PD escalation)  |
| 15 min       | Architect + Security Lead           | Suspect signing infrastructure issue           |
| 30 min       | VP Engineering                      | Sustained breach OR collusion oracle anomalies |
| 60 min       | CEO + Legal                         | Suspected attack on admin plane (insider/compromise) |

**Comms template (internal `#admin-ops`):**

```
SEV-2 — Dual-approval gate latency breach
Pending requests > 30s: {count}.
Suspected cause: {MFA / HMAC / clock-skew / collusion / approver lag}.
Approver primary: @{handle}; secondary paged.
ABSOLUTELY NO BYPASS of dual-approval. Will page Security Lead at +15 min.
Next update: +10 min.
```

## Post-incident

Capture:

- Per-request latency histogram for the window.
- Was any request actually attacked (collusion attempt logged)?
- Was MFA / signing infrastructure root cause?
- INV-ADMIN-DUAL-APPROVAL holds? (No request decided without two distinct signers; no bypass occurred.)
- If signing key issue: schedule rotation review with Security.
- If approver UX issue (e.g., Slack webhook unreliable): file UX work item.
- Update collusion review with any flagged event.

## Related

- **SLO:** SLO-ADMIN-DUAL-APPROVAL-LATENCY (`slo_catalog.md §4.15`).
- **Invariant:** INV-ADMIN-DUAL-APPROVAL CRITICAL (S-13).
- **CTRLs:** CTRL-ADMIN-002 (dual-approval gate), CTRL-AUTH-010 (MFA step-up), CTRL-AUDIT-003 (audit chain).
- **FMs:** FM-205 (admin mistake), FM-258 (insider exfil), FM-350 (clock skew).
- **Patterns:** PAT-DUAL-APPROVAL-001.
- **Sister runbooks:** `RB-FM-205`, `RB-FM-258`, `RB-ADMIN-CONFIG-STALE`, `RB-ADMIN-ROTATION-GAP`, `RB-KEY-COMPROMISE`.
- **WI:** WI-S13-002.
- **ADR:** ADR-S11-001 (MFA step-up destructive arms only).
- **Drill cadence:** semestral tabletop (insider scenario + collusion test).

---

**Fim RB-ADMIN-DUAL-APPROVAL-BREACH.**
