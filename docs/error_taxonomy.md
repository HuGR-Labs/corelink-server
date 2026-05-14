---
id: "ERROR-TAXONOMY"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
tags: ["error-taxonomy", "cli", "cor-codes", "doctor"]
---

# CoreLink Error Taxonomy — COR_* Codes

Canonical error codes referenced by `corelink doctor` checks, CLI error messages,
and server API responses. All codes use the `COR_` prefix (namespace: CoreLink).

---

## CLI-surface codes (doctor checks)

### COR_NET_UNREACHABLE

**Check**: doctor #1 (Network)
**Meaning**: The CoreLink cluster endpoint is not reachable from this host.
**Remediation**:
1. Verify network connectivity to `corelink.dev` (ping, curl).
2. Check firewall rules — outbound HTTPS (port 443) must be allowed.
3. Check DNS resolution: `dig corelink.dev` should return valid IPs.
4. If behind a corporate proxy, set `HTTPS_PROXY` env var.

---

### COR_AUTH_INVALID

**Check**: doctor #2 (Auth)
**Meaning**: The PAT is malformed, expired, or does not have permission for the requested tenant scope.
**Remediation**:
1. Verify `CORELINK_PAT` env var is set and not empty.
2. Check PAT format: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`.
3. Regenerate PAT in CoreLink admin UI if expired.
4. Ensure PAT has the required tenant scope for the operation.

---

### COR_STORAGE_WRITE_DENIED

**Check**: doctor #3 (Storage write)
**Meaning**: A test blob upload failed. Possible causes: quota exceeded, plan limit, or storage write permission denied.
**Remediation**:
1. Run `corelink doctor --json | jq '.[] | select(.check == "quota")'` to check quota status.
2. Verify tenant plan limits in CoreLink admin UI.
3. Contact support if write is denied despite quota availability.

---

### COR_STORAGE_READ_FAIL

**Check**: doctor #4 (Storage read)
**Meaning**: A test blob download failed or the BLAKE3 hash of the downloaded bytes did not match the expected digest.
**Remediation**:
1. If BYOK is enabled, verify KMS access (see `COR_BYOK_REVOKED`).
2. Check tenant primary region matches the connected endpoint (see `COR_REGION_MISMATCH`).
3. Retry — a transient read failure may recover after FM-150 backoff.
4. If hash mismatch persists, file a support ticket with the digest and region.

---

### COR_BYOK_REVOKED

**Check**: doctor #5 (BYOK)
**Meaning**: The Customer Managed Key (CMK) is not accessible. The key may be disabled, rotated out of the expected state, or the KMS credentials are invalid.
**Remediation**:
1. Verify CMK status in your KMS provider (AWS KMS / GCP Cloud KMS / Azure Key Vault / HashiCorp Vault).
2. Ensure the CoreLink service principal has `kms:Decrypt` and `kms:GenerateDataKey` permissions on the CMK.
3. If the key was rotated, update `[auth].byok_key_id` in `~/.corelink/config.toml`.
4. See S-14 BYOK runbook for full rotation procedure.

---

### COR_REGION_MISMATCH

**Check**: doctor #6 (Region)
**Meaning**: The tenant's `primary_region` does not match the region of the connected endpoint.
**Remediation**:
1. Log in to CoreLink admin UI and verify the `primary_region` setting for your tenant.
2. Use the region-specific endpoint: `<tenant>.<region>.corelink.dev`.
3. Update `CORELINK_BASE_URL` env var to point to the correct regional endpoint.

---

### COR_QUOTA_EXCEEDED

**Check**: doctor #7 (Quota)
**Meaning**: Tenant storage or operations quota is at or above the hard threshold.
**Remediation**:
1. Check current usage in CoreLink admin UI under Plan & Billing.
2. Evict unused blobs via `corelink gc` (when available, WI-S06-*).
3. Contact sales to upgrade plan: sales@corelink.dev.
4. Soft threshold (80%) generates a warning; hard threshold (100%) blocks writes.

---

### COR_CLIENT_VERIFY_DISABLED

**Check**: doctor #8 (Client verify)
**Meaning**: Client-side BLAKE3 content verification is disabled. This violates CTRL-CAS-002 (client verify default-on invariant).
**Remediation**:
1. Do NOT disable client verify except in explicit dev/test environments.
2. Check FFI wrapper configuration — verify `client_verify_enabled = true` in SDK config.
3. Re-enable via `corelink config set defaults.client_verify true` (when available).
4. File a security ticket if this was disabled without authorization.

---

## General API codes

| Code | Meaning | HTTP status |
|---|---|---|
| `COR_NET_UNREACHABLE` | Cluster unreachable | — |
| `COR_AUTH_INVALID` | Invalid or expired PAT | 401 |
| `COR_AUTH_FORBIDDEN` | PAT lacks required scope | 403 |
| `COR_STORAGE_WRITE_DENIED` | Write denied (quota or plan) | 403 / 429 |
| `COR_STORAGE_READ_FAIL` | Read failed or hash mismatch | 500 / 404 |
| `COR_BYOK_REVOKED` | CMK not accessible | 503 |
| `COR_REGION_MISMATCH` | Tenant region mismatch | 400 |
| `COR_QUOTA_EXCEEDED` | Quota hard threshold reached | 429 |
| `COR_CLIENT_VERIFY_DISABLED` | Client verify off | — |
| `COR_RATE_LIMITED` | Rate limit exceeded | 429 |
| `COR_TENANT_NOT_FOUND` | Tenant does not exist | 404 |
| `COR_DIGEST_MISMATCH` | BLAKE3 hash mismatch server-side | 400 |
| `COR_INTERNAL_ERROR` | Unexpected server error | 500 |
