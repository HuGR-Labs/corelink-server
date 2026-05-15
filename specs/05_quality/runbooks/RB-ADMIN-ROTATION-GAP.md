---
id: "RB-ADMIN-ROTATION-GAP"
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
tags: ["runbook", "slo", "admin", "rotation", "key-management", "s13", "r6-2"]
---

# RB-ADMIN-ROTATION-GAP — Key Rotation Overlap Window Violation (SEV-1)

> **SLO covered:** SLO-ADMIN-ROTATION-OVERLAP (§4.16) — per-asset-class overlap window adherence (actual ÷ target ±10%).
> Targets: TDK 7d ±10%; PatSigning / AuditChain / AdminSigning 24h ±10%; BYOK 7d ±10% (canonical: `key_management.md §3.2.1`).
> **Invariants:** INV-KEY-NO-SKIP + INV-KEY-OVERLAP CRITICAL (S-13).
> **WI:** WI-S13-003.

## Symptom

- PagerDuty **SEV-1** page `corelink-slo-admin-rotation-overlap-breach` fires.
- Production failure: signing operations returning `KEY_UNKNOWN` for active token / event / commit (gap in key set during overlap).
- AuditChain append failing, PAT issuance failing, or AdminSigning gate failing — all symptomatic of missing key during overlap window.

## Detection

```promql
# Per-asset-class overlap ratio
corelink_key_rotation_overlap_ratio{asset_class="TDK"} < 0.9
  OR > 1.1  # ±10% canonical band

# Signing failures attributed to missing key
sum by (asset_class, reason) (
  rate(corelink_signing_fail_total{reason="key_not_in_set"}[5m])
) > 0
```

Audit:

```sql
SELECT asset_class, key_id, status, valid_from, valid_until
  FROM key_lifecycle
 WHERE status IN ('rotating', 'overlap')
 ORDER BY valid_from DESC;
```

## Immediate mitigation (≤ 5 min)

**SEV-1: signing outage. Customer-impacting.**

1. **Confirm scope**: which asset class? TDK / PatSigning / AuditChain / AdminSigning / BYOK?
2. **Identify the gap**:
   ```bash
   curl -s "https://corelink.dev/_admin/keys/active?asset_class=$ASSET" \
     -H "Authorization: Bearer $ADMIN_TOKEN"
   ```
   Compare returned `key_set` to expected overlap pair (old + new during overlap window).
3. **Restore prior key to active set** (emergency rollback):
   ```bash
   curl -X POST "https://corelink.dev/_admin/keys/emergency-restore" \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"asset_class":"'$ASSET'","key_id":"'$LAST_GOOD_KEY_ID'"}'
   ```
   This requires dual-approval (CTRL-ADMIN-002) — pre-approved emergency window: 15 min.
4. **If emergency restore fails**: page Security Lead + Architect immediately; consider degrade mode for affected asset:
   - AuditChain: pause `breach.declared.v1`-class destructive paths until restored.
   - AdminSigning: freeze admin destructive ops (`ADMIN_DESTRUCTIVE_OPS_FROZEN=true`).
   - PatSigning: existing PATs still work (verify side); pause new issuance.
   - TDK: tenant data still readable via cached TDK; new tenant data encrypt path may pause.

## Root-cause investigation (15–30 min)

1. **Rotation scheduler**: was a planned rotation in progress? Check `corelink_key_rotation_in_progress{asset_class=...}` series.
2. **Overlap window math**: actual `valid_until_old - valid_from_new` vs target window. Per `key_management.md §3.2.1`:
   - TDK / BYOK: 7d ±10% (i.e., 6.3–7.7 days).
   - PatSigning / AuditChain / AdminSigning: 24h ±10% (21.6–26.4 hours).
3. **Clock skew** (FM-350): if worker clock skewed, key may appear expired prematurely.
4. **HSM availability**: HSM outage during rotation? See `RB-HSM-UNAVAILABLE`.
5. **CTRL-KEY-001 verification**: did rotation orchestrator log overlap entry, OR did it skip directly old → new?
6. **TLA+ INV-KEY-NO-SKIP**: replay scenario; if model accepts, the spec has a hole — escalate to Architect.

## Rollback / recovery

| Cause                                      | Recovery                                                                          |
|--------------------------------------------|-----------------------------------------------------------------------------------|
| Premature removal of old key (config bug)  | Emergency restore via `_admin/keys/emergency-restore`; rotation orchestrator patch |
| Clock skew caused early expiry             | NTP sync; resign overlap window with corrected timestamps                          |
| HSM outage interrupted rotation midway     | Follow `RB-HSM-UNAVAILABLE`; resume rotation once HSM healthy                     |
| Manual ops mistake (admin deleted key)     | `RB-FM-205` admin mistake recovery; restore from HSM backup                       |
| Scheduler bug (skip detected, INV-KEY-NO-SKIP violation) | Patch + redeploy scheduler; conduct full asset-class rotation audit |
| BYOK key revoked during overlap by customer| Follow `RB-BYOK-REVOKE`; customer-driven not infra bug                            |

Verify recovery:

```promql
# Overlap ratio inside ±10% band
corelink_key_rotation_overlap_ratio{asset_class="TDK"} >= 0.9
  AND corelink_key_rotation_overlap_ratio{asset_class="TDK"} <= 1.1

# No signing failures
sum(rate(corelink_signing_fail_total{reason="key_not_in_set"}[5m])) == 0
```

## Escalation path

| Time elapsed | Who                                            | Criteria                                       |
|--------------|------------------------------------------------|------------------------------------------------|
| 0            | Primary SRE + Security Lead (auto-paged)       | SEV-1 fires                                    |
| 5 min        | Architect + Secondary SRE                      | MTTA breach or HSM suspect                     |
| 15 min       | VP Engineering + CISO                          | Restore not completed; signing outage continues|
| 30 min       | CEO + Comms Lead + Legal                       | Customer-visible OR audit chain integrity at risk |
| 60 min       | Compliance auditor + customers w/ BYOK SLA     | Compliance/audit-chain gap requires disclosure |

**Comms template (status page):**

```
[Investigating] Key management operations on the CoreLink admin plane
are temporarily limited. New tenant onboarding, PAT issuance, and signed
audit operations may be delayed. Existing customer data and operations are
not affected. Next update in 15 min.
```

## Post-incident

Capture:

- Full rotation timeline per asset class (planned vs actual `valid_from`/`valid_until`).
- INV-KEY-NO-SKIP + INV-KEY-OVERLAP: did invariant hold throughout? (If broken, mandatory TLA+ regression test.)
- HSM logs for the window.
- Audit chain continuity: any gap in `audit_chain.sequence`? (If yes, this is itself a SEV-1 follow-up.)
- Customer impact: which tenants saw failed operations?
- Update key_management.md §3.2.1 if overlap window targets need adjustment based on observed reality.
- Property test (S-13) coverage extended to caught case.

## Related

- **SLO:** SLO-ADMIN-ROTATION-OVERLAP (`slo_catalog.md §4.16`).
- **Invariants:** INV-KEY-NO-SKIP, INV-KEY-OVERLAP CRITICAL (S-13).
- **CTRLs:** CTRL-KEY-001 (rotation orchestrator), CTRL-KEY-005..006 (overlap policy), CTRL-CRED-003.
- **FMs:** FM-204 (secret rotation), FM-205 (admin mistake), FM-258 (insider).
- **Patterns:** PAT-ROLL-FORWARD-001.
- **Sister runbooks:** `RB-HSM-UNAVAILABLE`, `RB-KEY-COMPROMISE`, `RB-BYOK-REVOKE`, `RB-FM-205`, `RB-ADMIN-CONFIG-STALE`, `RB-ADMIN-DUAL-APPROVAL-BREACH`.
- **WI:** WI-S13-003.
- **ADR:** ADR-0018 (key overlap per asset).
- **Canonical:** `key_management.md §3.2.1`.
- **Drill cadence:** semestral (planned rotation + emergency restore dry-run).

---

**Fim RB-ADMIN-ROTATION-GAP.**
