---
id: "RB-BYOK-REVOKE"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: "0.1.0"
superseded_by: null
tags: ["runbook", "p1", "byok", "enterprise", "kill-switch", "production-grade", "s14-ship-gate"]
---

# RB-BYOK-REVOKE — Customer BYOK CMK Revocation / Kill Switch (Production-Grade)

> **Trigger:** A customer enterprise tenant has revoked CoreLink's access to their CMK in AWS KMS,
> GCP Cloud KMS, Azure Key Vault, or HashiCorp Vault.
>
> **By design:** all tenant cache reads become unavailable within ≤ 6 min p99 (60s detection
> window + 5 min DEK cache TTL hard expiry). This is INV-BYOK-CRYPTO-SOVEREIGNTY.
>
> **Nature:** intentional customer-controlled kill switch, NOT an internal incident.
> **Severity rating:** SEV-2 (intentional) → SEV-1 if customer asserts revoke was accidental
> AND data access is needed for active customer-critical operations.
>
> **INVs:** INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL §3.12) · CTRL-KEY-011 (kill switch ≤ 6 min).

---

## §1. Alert Signatures

### 1.1 Prometheus alerts

| Alert | Threshold | Meaning |
|---|---|---|
| `ByokKmsAccessDenied` | `corelink_byok_kms_errors_total{error="access_denied"} > 3` in 2m | CMK revoke in progress |
| `ByokKmsCheckFailed` | `corelink_byok_kms_check_outcome_total{outcome="failed"} > 0` in 5m | Background check failure |
| `ByokKillSwitchActivated` | `corelink_byok_kill_switch_duration_seconds_bucket` present | Kill switch flow started |
| `ByokDekCacheEmpty` | `corelink_byok_dek_cache_size{tenant_id=X} == 0` post-revoke | DEK cache evicted |
| `ByokTenantDegraded` | `corelink_byok_tenant_status{status="degraded_read_only"} > 0` | Tenant access restricted |

### 1.2 KMS API error codes (by provider)

| Provider | Error code | Meaning |
|---|---|---|
| AWS KMS | `AccessDeniedException` / `KMSInvalidKeyUsageException` | CMK access revoked |
| GCP Cloud KMS | `PERMISSION_DENIED` / `FAILED_PRECONDITION` | Key IAM binding removed |
| Azure Key Vault | `Forbidden (403)` / `KeyNotFound` | Access policy revoked |
| HashiCorp Vault | `403 Forbidden` / `permission denied` | Token/policy revoked |

### 1.3 Audit events to verify

- `corelink.byok.cmk_revoked` — emitted by `RevocationDetector` at detection time.
- `corelink.byok.dek_cache_evicted` — emitted when all DEKs for tenant purged.
- `corelink.byok.tenant_degraded` — emitted when tenant flipped to `degraded_read_only`.

---

## §2. Severity Triage

| Condition | Severity | Escalation |
|---|---|---|
| Customer confirmed intentional revoke (rotation or offboarding) | SEV-2 | Customer Success + SRE on-call |
| Customer asserts revoke was accidental | SEV-1 | Security Lead + SRE + Crypto SME + Engineering |
| Revoke detected but not customer-initiated (potential compromise) | SEV-1 CRITICAL | Security Lead + CISO + Incident Commander |
| Kill switch p99 > 6 min (SLA breach) | SEV-1 | Engineering + Security Lead + SRE |

Page via PagerDuty policy `byok-kill-switch`. Slack: `#incidents-corelink`.

---

## §3. Detection Verification (≤ 2 min)

**Step 3.1:** Confirm tenant and provider from alert metadata.

```bash
# Query tenant status in D1
corelink-admin byok status --tenant-id <TENANT_ID>
# Expected output: status=degraded_read_only, kms_error=access_denied, provider=aws
```

**Step 3.2:** Verify KMS access state.

```bash
# AWS KMS probe
aws kms describe-key --key-id <CMK_ARN> --region <REGION>
# Expected: AccessDeniedException OR KeyMetadata.KeyState != Enabled

# GCP KMS probe
gcloud kms keys describe <KEY_NAME> --keyring=<RING> --location=<LOCATION> --project=<PROJECT>

# Azure Key Vault probe
az keyvault key show --vault-name <VAULT> --name <KEY_NAME>

# HashiCorp Vault probe
vault token lookup <VAULT_TOKEN>
```

**Step 3.3:** Verify audit event chain.

```bash
corelink-admin audit tail --tenant-id <TENANT_ID> --event-types byok.cmk_revoked,byok.dek_cache_evicted,byok.tenant_degraded --last 30m
```

**Step 3.4:** Measure kill switch timing.

```bash
# Time from first KMS error to tenant degraded status
corelink-admin byok kill-switch-timing --tenant-id <TENANT_ID>
# SLA: detection ≤ 60s, total ≤ 360s (6 min p99)
```

---

## §4. Immediate Containment (≤ 6 min from detection)

CoreLink's automatic kill switch path (no operator action required for the primary flow):

1. **RevocationDetector** (background, every 60s) calls `check_access(tenant_id)` → KMS returns `AccessDenied`.
2. **DEK cache eviction**: all in-memory DEKs for tenant purged; TTL hard-expired.
3. **Tenant state flip**: D1 `byok_configs.status = degraded_read_only`.
4. **Audit emit**: `corelink.byok.cmk_revoked` + `corelink.byok.dek_cache_evicted` + `corelink.byok.tenant_degraded`.
5. **All tenant reads return 503** with `COR_BYOK_CMK_REVOKED` error code.

**Operator verification actions (run in parallel with auto-path):**

```bash
# Verify DEK cache is empty
corelink-admin byok dek-cache status --tenant-id <TENANT_ID>
# Expected: cache_size=0, evicted_at=<timestamp>

# Verify tenant status
corelink-admin byok status --tenant-id <TENANT_ID>
# Expected: status=degraded_read_only

# Verify no in-flight writes are processing CMK material
corelink-admin byok in-flight-ops --tenant-id <TENANT_ID>
# Expected: active_wrap_ops=0, active_unwrap_ops=0
```

---

## §5. Communication Templates

### 5.1 Internal Slack notification (post to `#incidents-corelink`)

```
[BYOK KILL SWITCH] Tenant: <TENANT_ID> | Provider: <PROVIDER> | Status: degraded_read_only
Detection: <timestamp> | Duration: <N>s (SLA ≤ 360s)
CMK ARN: <ARN> | Nature: intentional/accidental (TBD)
Action: Customer Success notified | SRE monitoring
Runbook: specs/05_quality/runbooks/RB-BYOK-REVOKE.md
```

### 5.2 Customer notification — intentional revoke (send within 1h business hours)

```
Subject: CoreLink — BYOK Key Revocation Confirmed

We have detected that access to your Customer Master Key (CMK) for CoreLink has been revoked.

Effective: <timestamp UTC>
Tenant: <TENANT_DISPLAY_NAME>
Provider: <KMS_PROVIDER>
Status: Your cache data is now inaccessible (by design — BYOK kill switch active).

Next steps:
- CMK Rotation: Please re-grant CoreLink's service account access to your new CMK and notify
  support@corelink.io. We will coordinate re-wrapping of your Data Encryption Keys.
- Offboarding: Please contact your Customer Success Manager to initiate the formal DPA
  data erasure process per your agreement.

Evidence pack available on request under NDA (DPA §4.2).
```

### 5.3 Customer notification — accidental revoke (send immediately)

```
Subject: URGENT — CoreLink BYOK Access Disruption Detected

We have detected an unexpected loss of access to your Customer Master Key.

Effective: <timestamp UTC>
Tenant: <TENANT_DISPLAY_NAME>
Impact: Cache reads returning 503 (BYOK key inaccessible)

Recommended action: Please restore CoreLink's IAM/access policy access to your CMK immediately.
Our team is standing by: support@corelink.io | Emergency: +1-XXX-XXX-XXXX

Recovery: Once access is restored, CoreLink will automatically resume operations within 60s.
```

---

## §6. Intentional Revoke — Full Procedures

### Scenario A: Customer CMK Rotation

**Goal:** Coordinate re-wrap of all DEKs under new CMK.

1. **Verify new CMK is accessible** (operator):
   ```bash
   corelink-admin byok verify-access --tenant-id <TENANT_ID> --cmk-arn <NEW_CMK_ARN>
   ```

2. **Initiate DEK re-wrap**:
   ```bash
   corelink-admin byok rewrap-deks --tenant-id <TENANT_ID> \
     --old-cmk-arn <OLD_CMK_ARN> --new-cmk-arn <NEW_CMK_ARN>
   # Requires dual-approval for enterprise tier (CTRL-KEY-014)
   ```

3. **Verify re-wrap completion**:
   ```bash
   corelink-admin byok rewrap-status --tenant-id <TENANT_ID>
   # Expected: total_deks=N, rewrapped=N, failed=0
   ```

4. **Re-enable tenant**:
   ```bash
   corelink-admin byok enable-tenant --tenant-id <TENANT_ID>
   # Requires dual-approval (CTRL-KEY-014 + CTRL-ADMIN-002)
   ```

5. **Verify tenant active**:
   ```bash
   corelink-admin byok status --tenant-id <TENANT_ID>
   # Expected: status=active
   ```

6. **Audit chain verification**:
   ```bash
   corelink-admin audit verify-chain --tenant-id <TENANT_ID> --from <REVOKE_TS> --to <NOW>
   # Must confirm: cmk_revoked → dek_cache_evicted → tenant_degraded → dek_rewrapped → tenant_enabled
   ```

### Scenario B: Customer Offboarding (Erasure)

1. Obtain formal DPA acknowledgment from customer (legal team must sign off).
2. Initiate DSR erasure: `corelink-admin dsr initiate-erasure --tenant-id <TENANT_ID> --type BYOK_OFFBOARD`.
3. Per `RB-DSR-ERASURE-INCOMPLETE` and `privacy_model.md §6.2`: 12 backends erasure cascade.
4. Erasure attestation Ed25519-signed (INV-ERASURE-ATTESTATION-SIGNED; WI-S14-007).
5. Deliver erasure attestation to customer within SLA (CTRL-PRIV-031).
6. Archive DPA + attestation for 7y (NIST SP 800-88 Rev.1 evidence).

---

## §7. Accidental Revoke — Recovery Procedures

**Operator steps (customer has re-granted IAM access):**

1. **Wait for RevocationDetector to detect re-grant** (next cycle, ≤ 60s):
   ```bash
   # Monitor recovery
   watch -n 5 corelink-admin byok status --tenant-id <TENANT_ID>
   # Expected: status transitions degraded_read_only → active within 60s of re-grant
   ```

2. **If automatic recovery does not trigger within 2 min, force re-check**:
   ```bash
   corelink-admin byok force-check --tenant-id <TENANT_ID>
   ```

3. **Verify DEK cache repopulated**:
   ```bash
   corelink-admin byok dek-cache status --tenant-id <TENANT_ID>
   # Expected: cache_size > 0 (lazy-populated on first request)
   ```

4. **Smoke test**: issue a test read/write for tenant to confirm end-to-end.

5. **Audit chain verification** (required for post-incident report):
   ```bash
   corelink-admin audit verify-chain --tenant-id <TENANT_ID> --from <INCIDENT_START> --to <NOW>
   ```

6. **Customer notification** (accidental recovery template):
   ```
   Subject: CoreLink — BYOK Access Restored

   Access to your Customer Master Key has been restored.
   Effective: <timestamp UTC>
   Duration of disruption: <N> minutes
   Impact: Cache reads were unavailable during disruption window.
   Status: Fully operational.

   Please contact your Customer Success Manager if you have any questions.
   ```

---

## §8. Forensics & Post-Incident

### 8.1 Evidence collection

```bash
# Collect full incident timeline
corelink-admin audit export --tenant-id <TENANT_ID> \
  --from <INCIDENT_START_MINUS_5M> --to <INCIDENT_END_PLUS_5M> \
  --format json > /tmp/byok-incident-<TENANT_ID>-<DATE>.json

# Collect KMS access log (provider-specific)
# AWS: CloudTrail → KMS event history for CMK ARN
# GCP: Cloud Audit Logs → Cloud KMS activity
# Azure: Key Vault audit logs → Access denied events
# Vault: Vault audit log → deny events for key path
```

### 8.2 SLA verification

| Metric | SLA | Source |
|---|---|---|
| Detection latency | ≤ 60s | `corelink_byok_kms_check_interval_seconds` |
| DEK cache TTL expiry | ≤ 5 min | `corelink_byok_dek_cache_ttl_seconds` |
| Total kill switch duration | ≤ 6 min p99 | `corelink_byok_kill_switch_duration_seconds_bucket` |
| Customer notification | ≤ 1h business hours | Manual verification |

### 8.3 Post-incident report structure

Required within 24h (SEV-1) or 7d (SEV-2):

1. Timeline: first KMS error → detection → tenant degraded → resolution.
2. Duration: total downtime for tenant.
3. Cause: intentional / accidental / potential compromise (each requires different follow-up).
4. SLA compliance: kill switch timing vs ≤ 6 min SLA.
5. Audit chain: complete event trail attached.
6. Customer impact summary.
7. Action items (if SLA breached or process gap identified).

### 8.4 Regulatory notification check

- If revoke window > 72h AND tenant holds EU personal data: assess GDPR Art. 33 (72h breach notification) with Privacy Officer.
- If revoke was due to compromise (not customer action): Security Lead initiates breach assessment immediately.
- DPA §4 breach notification clause: notify customer within SLA per their DPA.

---

## §9. Prevention & Verification Cadence

| Activity | Cadence | Owner |
|---|---|---|
| Kill switch chaos drill (staging) | Weekly (per `.github/workflows/byok_kill_switch_drill_weekly.yml`) | SRE |
| Customer "probe revoke" in staging | Monthly | Customer Success + SRE |
| Full RB-BYOK-REVOKE dry-run | Semi-annual | Security Lead + SRE + Crypto SME |
| DEK cache TTL audit | Quarterly | Engineering |
| KMS access check interval audit | Quarterly | Engineering |

---

## §10. Controls Cross-Reference

| Control | Status | Evidence |
|---|---|---|
| CTRL-KEY-010 (BYOK multi-cloud) | ACTIVE | WI-S14-004/005 |
| CTRL-KEY-011 (kill switch ≤ 6 min) | ACTIVE | `RevocationDetector`; chaos drill weekly |
| CTRL-KEY-012 (BYOK audit export) | ACTIVE | `corelink.byok.*` audit events |
| CTRL-KEY-013 (DEK cache TTL hard 5 min) | ACTIVE | `ByokConfig.dek_cache_ttl_secs = 300` |
| CTRL-KEY-014 (DEK re-wrap dual-approval) | ACTIVE | WI-S13-002 dual-approval gate |
| CTRL-PRIV-031 (data residency) | ACTIVE | WI-S14-002 region pinning |
| INV-BYOK-CRYPTO-SOVEREIGNTY | VERIFIED | Chaos drill + property test 30k |
| INV-ERASURE-ATTESTATION-SIGNED | VERIFIED | WI-S14-007 Ed25519 |

---

## §11. Dry-Run Record

**Date:** 2026-05-14 | **Executor:** Gustavo Schneiter (via WI-S14-009 builder)
**Participants:** Security Lead (dual-hat) + SRE (dual-hat) + Crypto SME (dual-hat) + AppSec (dual-hat)
**Duration:** 2h drill + 1h debrief | **Result:** PASS | **Drift findings:** 0

Full dry-run report: `specs/_audits/2026-05-14-rb-byok-revoke-dry-run.md`.

Kill switch p99 validated at 2s (SLA ≤ 360s). Audit chain verified. Customer notification templates reviewed. No runbook drift identified. All §3–§8 steps executable in staging environment.

---

## References

- `specs/03_architecture/key_management.md §3.2.1` (BYOK architecture)
- `specs/03_architecture/invariant_registry.md §3.12` (INV-BYOK-CRYPTO-SOVEREIGNTY)
- `specs/04_sprints/S14/work_items/WI-S14-006-cmk-revocation-kill-switch-5min-chaos-drill.md`
- `specs/04_sprints/S14/work_items/WI-S14-009-tla-region-residency-rb-byok-revoke-pentest-prr.md`
- `specs/_audits/2026-05-14-rb-byok-revoke-dry-run.md`
- `scripts/byok_kill_switch_drill.sh`
- `.github/workflows/byok_kill_switch_drill_weekly.yml`
- NIST SP 800-57 Pt 1 Rev 5 §5.3 (key management)
- NIST SP 800-88 Rev.1 (crypto-erase)
