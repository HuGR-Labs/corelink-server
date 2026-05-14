---
audience: customer
classification: public (post-NDA)
wi: WI-S14-006
version: "1.0.0"
updated: "2026-05-14"
---

# CoreLink BYOK — Customer Kill Switch

## Overview

As a CoreLink BYOK customer, you have **full crypto sovereignty** over your
data. You can revoke access to your Customer-Managed Key (CMK) at any time
and CoreLink will automatically cease all access within **≤ 5 minutes**.

This document explains the kill switch, how to use it, SLA guarantees, and
recovery procedures.

---

## How the Kill Switch Works

1. **You revoke or disable your CMK** in your KMS provider (AWS / GCP /
   Azure / Vault).
2. **CoreLink detects the revocation** within 60 seconds via our background
   KMS access check.
3. **Your data becomes inaccessible**:
   - DEK cache is immediately cleared (all cached decryption keys removed).
   - Your tenant is switched to read-only degraded mode.
4. **You are alerted** via dashboard + email + in-app notification.
5. **Total time from CMK revocation to complete data inaccessibility**:
   ≤ 5 minutes (p99 global across all 6 CoreLink regions).

---

## SLA Guarantees

| Component | Guarantee |
|---|---|
| Detection: CMK revoked → CoreLink detects | ≤ 60 seconds |
| Cache eviction: all in-flight decryption keys cleared | ≤ 5 minutes |
| Customer notification: dashboard + email + in-app | ≤ 5 minutes |
| Total kill switch SLA (detection + eviction + alert) | ≤ 5 minutes p99 |
| Chaos drill cadence (verified in staging) | Weekly, per provider |

---

## How to Revoke Your CMK

### AWS KMS

```bash
# Disable key (reversible)
aws kms disable-key --key-id YOUR_KEY_ID

# Or schedule deletion (irreversible; 7-30 day minimum)
aws kms schedule-key-deletion --key-id YOUR_KEY_ID --pending-window-in-days 7
```

### Google Cloud KMS

```bash
gcloud kms keys versions destroy YOUR_KEY_VERSION \
    --location YOUR_LOCATION \
    --keyring YOUR_KEYRING \
    --key YOUR_KEY_NAME
```

### Azure Key Vault

```bash
az keyvault key disable \
    --vault-name YOUR_VAULT \
    --name YOUR_KEY_NAME
```

### HashiCorp Vault

```bash
vault write auth/token/revoke token=YOUR_TOKEN
```

---

## Multi-Channel Alert Delivery

When your CMK is revoked, you receive:

- **Dashboard alert**: visible immediately in your CoreLink admin dashboard.
- **Email notification**: sent to your registered security contact.
- **In-app notification**: visible to all admin users in your organization.
- **Slack webhook** (optional): configure in Settings → Integrations → Slack.

---

## Configuring Slack Webhook (Optional)

1. Navigate to Settings → Security → BYOK → Alert Channels.
2. Enter your Slack webhook URL.
3. Click "Test Alert" to verify delivery.

---

## Recovery: Re-enabling Access

If you re-enable your CMK (e.g., after an accidental revocation):

1. Re-enable or restore your CMK in your provider console.
2. CoreLink automatically detects CMK accessibility within **60 seconds**.
3. Your tenant is restored to full access.
4. You receive a recovery notification via dashboard + email.

**No CoreLink intervention required.** Recovery is fully automated.

---

## Frequently Asked Questions

**Q: Is there a "cancel" option before the kill switch fires?**
A: No. When you revoke your CMK, the kill switch fires immediately. This is
   intentional — your revocation is your instruction to CoreLink. Recovery
   is fast (re-enable CMK → access restored within 60 seconds).

**Q: Can CoreLink staff bypass the kill switch?**
A: No. The kill switch is enforced at the code level with no operator
   override. CoreLink staff cannot bypass or defer the kill switch.

**Q: What happens to data in flight during the kill switch?**
A: Reads that began before the kill switch will complete only if their
   decryption key is still in cache (maximum 5 minutes from wrap time).
   New reads after CMK revocation will fail immediately with an access error.

**Q: What if CoreLink temporarily cannot reach my KMS provider?**
A: After 3 consecutive failed checks (3 minutes), CoreLink conservatively
   degrades your tenant to read-only as a safety precaution. You will be
   notified. If your CMK is accessible, re-enabling or verifying CMK
   accessibility will restore access on the next check.

---

## Compliance References

| Standard | Control |
|---|---|
| SOC 2 Type II | CC6.1 (logical access controls) |
| GDPR | Art. 17 (right to erasure), Art. 32 (security of processing) |
| LGPD | Art. 38 (data security) |
| NIST SP 800-57 | Key management + revocation |

---

*CoreLink BYOK kill switch — WI-S14-006. SLA audited weekly in staging.*
