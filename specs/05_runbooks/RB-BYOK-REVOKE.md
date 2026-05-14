---
id: "RB-BYOK-REVOKE"
type: "runbook"
wi: "WI-S14-006"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "byok", "kill-switch", "cmk-revocation", "s14", "inv-byok-crypto-sovereignty"]
---

# RB-BYOK-REVOKE — BYOK CMK Revocation Kill Switch Runbook

## 1. Summary

Customer revokes CMK (or CMK becomes inaccessible) → CoreLink kill switch
must propagate ≤ 5 min p99 global. This runbook covers detection,
investigation, customer communication, and recovery.

**SLA**: detection ≤ 60s + eviction + degrade + alert ≤ 300s total.
**INV-BYOK-CRYPTO-SOVEREIGNTY**: NO operator override. NO advisory mode.
This is NOT waivable (spec contract §19).

---

## 2. Trigger Conditions

| Trigger | Signal |
|---|---|
| Kill switch fired | `corelink.byok.cmk_revoked` audit event in audit_outbox |
| SLA violation | `corelink_byok_kill_switch_sla_violation_total > 0` — SEV-1 |
| KMS access check fail sustained | `corelink_byok_cmk_access_check_total{outcome="api_error"}` sustained 3 cycles |
| Customer complaint: access lost | Support ticket + dashboard alert |

---

## 3. Detection

### 3.1 Confirm kill switch fired

```bash
# Query D1 staging/production audit_outbox for revocation events.
# Replace $TENANT_ID and $DATE with actuals.
wrangler d1 execute corelink-db --command \
  "SELECT * FROM audit_outbox
   WHERE event_type = 'corelink.byok.cmk_revoked'
   AND created_at > datetime('now', '-1 hour')
   ORDER BY created_at DESC LIMIT 10;"
```

### 3.2 Check tenant byok_status

```bash
wrangler d1 execute corelink-db --command \
  "SELECT tenant_id, byok_status, byok_revoked_at_ms,
          byok_revoked_provider, byok_revoked_kms_key_id
   FROM tenants
   WHERE byok_status != 'active'
   ORDER BY byok_revoked_at_ms DESC LIMIT 20;"
```

### 3.3 Check kill switch SLA metric

```bash
# Check Prometheus metric via Grafana DASH-BYOK panel or:
# corelink_byok_kill_switch_duration_seconds_bucket — p99 should be ≤ 360s.
# corelink_byok_kill_switch_sla_violation_total — must be 0.
```

---

## 4. Investigation

### 4.1 Customer-side CMK status

| Provider | Check CMK Status |
|---|---|
| AWS | `aws kms describe-key --key-id $KEY_ID` (check `KeyState`) |
| GCP | `gcloud kms keys describe $KEY_NAME --location $LOC --keyring $RING` |
| Azure | `az keyvault key show --vault-name $VAULT --name $KEY_NAME` |
| Vault | `vault read auth/token/lookup-self` + `vault kv get $PATH` |

### 4.2 CoreLink-side state

```bash
# DEK cache eviction count from audit event payload:
wrangler d1 execute corelink-db --command \
  "SELECT json_extract(payload, '$.evicted_dek_count'),
          json_extract(payload, '$.kill_switch_duration_ms')
   FROM audit_outbox
   WHERE event_type = 'corelink.byok.cmk_revoked'
   ORDER BY created_at DESC LIMIT 5;"
```

### 4.3 Customer alert delivery

```bash
# Check customer_alerts table for delivery confirmation:
wrangler d1 execute corelink-db --command \
  "SELECT channel, status, delivered_at
   FROM customer_alerts
   WHERE tenant_id = '$TENANT_ID'
   ORDER BY created_at DESC LIMIT 10;"
```

---

## 5. Customer Communication

**Template: Revocation notification** (auto-dispatched by kill switch; manual verification):

```
Subject: [CoreLink] BYOK CMK Access Revoked — Action Required

Your Customer-Managed Key (CMK) for CoreLink BYOK has been revoked or
is no longer accessible.

Provider: {{provider}}
Detected: {{detected_at}} UTC
Impact: All CoreLink content access for your tenant has been suspended
        pending CMK restoration.

Recovery:
1. Re-enable or restore your CMK in {{provider}} console.
2. CoreLink will automatically detect CMK restoration within 60 seconds.
3. Your dashboard will show "BYOK Active" when access is restored.

If you believe this is in error, please verify your CMK status in
{{provider}} and contact CoreLink support.

— CoreLink Security Team
```

**Template: Recovery notification** (auto-dispatched on restoration):

```
Subject: [CoreLink] BYOK CMK Access Restored

Your Customer-Managed Key (CMK) has been detected as accessible again.
CoreLink BYOK access has been restored for your tenant.

Provider: {{provider}}
Restored: {{restored_at}} UTC

— CoreLink Security Team
```

---

## 6. Recovery

### 6.1 Customer re-enables CMK

1. Customer re-enables CMK in their provider console.
2. CoreLink `RevocationDetector` checks every 60 s.
3. Next cycle returning `KmsAccessStatus::Ok` triggers:
   - `TenantStatusStore::restore_active` → D1 `byok_status = 'active'`
   - `corelink.byok.cmk_restored` audit event emitted
   - Recovery alert dispatched to customer (dashboard + email + in-app)

### 6.2 Verify recovery

```bash
wrangler d1 execute corelink-db --command \
  "SELECT byok_status FROM tenants WHERE tenant_id = '$TENANT_ID';"
# Expected: 'active'
```

### 6.3 DEK cache restoration

DEK cache repopulates automatically on next customer read (unwrap from KMS
on cache miss). No manual intervention required.

---

## 7. SLA Violation Response (SEV-1)

If `corelink_byok_kill_switch_sla_violation_total > 0`:

1. **Page SRE on-call + Crypto SME immediately**.
2. Identify root cause (detection delay? eviction delay? D1 batch failure?).
3. Emit CRITICAL post-mortem within 24h.
4. INV-BYOK-CRYPTO-SOVEREIGNTY review: if violated, Compliance Officer notified.
5. Customer breach notification if data was accessible post-revocation window.

---

## 8. Escalation Matrix

| Condition | Escalate To | Timeline |
|---|---|---|
| Kill switch SLA miss | SRE Lead + Crypto SME + Compliance | Immediate SEV-1 |
| DEK cache TTL bypass detected | Security Lead + Architect | Immediate CRITICAL |
| Operator override detected in code | Security incident | Immediate |
| Customer alert delivery failure sustained 1h | SRE Lead + Customer Success | SEV-2 |
| False-positive revocation (network partition) | SRE + Customer | SEV-3; customer communication |

---

## 9. Drift Assessment Checklist

Run quarterly (or after any kill-switch event):

- [ ] Runbook commands execute without error against current D1 schema.
- [ ] DASH-BYOK panels show kill switch duration p99 + recent events.
- [ ] Customer alert templates match current product language.
- [ ] Recovery flow verified in staging.
- [ ] SLA metric `corelink_byok_kill_switch_duration_seconds_bucket` operational.
- [ ] Chaos drill weekly cron green (last 4 weeks).

---

## 10. Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Initial runbook WI-S14-006. |
