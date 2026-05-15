---
id: "RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "S-17"
parent_wi: "WI-S09-008"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-AUDIT-EXPORT-INTEGRITY"
  - "ONCALL-ESCALATION-MATRIX"
  - "security_model"
tags: ["runbook", "audit", "audit-export", "security", "sev-1", "soc2", "cc7.2", "gdpr", "lgpd", "tenant-isolation", "fail-closed", "wave-17"]
---

# RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT — Cross-tenant audit-export attempt (SEV-1)

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Scope:** operator response when the PagerDuty rule
> `AuditExport_CrossTenantAttempt` fires. The rule counts emits of
> the audit event `corelink.security.audit_export_cross_tenant_attempt.v1`
> from the customer audit-export endpoint
> (`GET /v1/audit/export`, wave-16; `apps/server/src/routes/audit_export.rs`).
>
> **Severity:** **SEV-1**. The endpoint already rejected the request
> with `403 Forbidden` BEFORE the page-out (fail-CLOSED ordering;
> `audit_export.rs` §3 cross-tenant attempt check); this runbook
> exists to lock down the source before a pivot — NOT to recover
> data the attacker tried to read.
>
> **MTTA target:** 5 minutes (24/7 per `observability_model.md` §9.2).
>
> **MTTR target:** **24 hours** end-to-end (per S-17 ops maturity
> baseline). Drift > 2× → fitness-function regression
> (`corelink-runbook-tracker`).
>
> **Companion docs / refs:**
> - `dashboards/alerts/dash-audit-export-alerts.yml` (the PD alert rule)
> - `apps/server/src/routes/audit_export.rs` (the cross-tenant attempt emit point)
> - `specs/03_architecture/observability_model.md` §9 (severity + clock semantics)
> - `specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md` (sibling: customer-reported verify failure)
> - `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`
> - `RB-AUDIT-EXPORT-VERIFY-FAILED.md` (sibling SEV-0 — if both fire concurrently, treat as a coordinated attack — escalate to Tier-3 immediately)

---

## 1. Detect (≤ 5 minutes)

The alert rule `AuditExport_CrossTenantAttempt` fires when

```promql
sum by (tenant_id_hex8) (
  increase(corelink_audit_export_cross_tenant_attempts_total[5m])
) > 0
```

The PagerDuty incident body carries the **pseudonymised tenant id**
(`tenant_id_hex8` = first 8 hex chars of BLAKE3(tenant_id); per
INV-AUTH-AUDIT-PSEUDONYMIZATION + CTRL-PRIV-001). The raw tenant_id
is **NOT** in the page — pull it from the admin audit-viewer with
dual-approval (`WI-S16-005`).

**Step 1.1 — Acknowledge the page within 5 min.** From PagerDuty
mobile or `/ack` in `#corelink-alerts` Slack thread.

**Step 1.2 — Pull the offending audit row(s).** The route emits one
`corelink.security.audit_export_cross_tenant_attempt.v1` event per
rejected request. Query the audit-chain D1 mirror for the last 15
minutes:

```bash
wrangler d1 execute corelink-audit \
  --command "SELECT
    event_id,
    occurred_at,
    authenticated_tenant_blake3_hex8,
    attempted_tenant_blake3_hex8,
    source_ip_country,
    source_asn,
    correlation_id
  FROM audit_events_index
  WHERE event_type = 'corelink.security.audit_export_cross_tenant_attempt.v1'
    AND occurred_at >= datetime('now', '-15 minutes')
  ORDER BY occurred_at DESC;"
```

Count distinct `correlation_id` values:
- **1 distinct** → single failed probe; likely misconfigured customer
  client. Containment §2.1 (suspend JWT only).
- **2-5 distinct** → probable enumeration probe. Containment §2.2
  (suspend JWT + block source IP/ASN).
- **≥ 6 distinct in 5 min** → active enumeration. Containment §2.3
  (suspend JWT + block source IP + raise Tier-3 + freeze tenant
  audit-export endpoint per-tenant — see §3).

---

## 2. Containment

The endpoint already returned 403; containment exists to stop the
attacker from re-probing with a fresh attempt against any other
tenant.

### 2.1 Single probe — suspend the JWT only

```bash
# Resolve the offending JWT jti from the correlation_id.
wrangler d1 execute corelink-auth \
  --command "SELECT jti, sub_user_id, issued_at, expires_at
             FROM sessions_index
             WHERE correlation_id = '<CORRELATION_ID>';"

# Revoke the jti.
corelink admin auth revoke-jti \
  --jti <JTI> \
  --reason "RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT containment §2.1" \
  --paged-runbook RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT
```

The revoke emits `corelink.security.jwt_revoked.v1` — chain anchor
for the post-incident review.

### 2.2 Multiple probes — JWT + source IP block (same-tenant repeats)

Decision criterion: same `authenticated_tenant_blake3_hex8` produced
**≥ 2 distinct rejected requests within 5 min** against ≥ 2 distinct
`attempted_tenant_blake3_hex8` values. Block the source IP at the
Cloudflare edge:

```bash
# 1. Pull source IP from the audit row (the raw IP IS captured in the
#    SOC2-evidence audit envelope, NOT in the metric label — see
#    INV-OBS-CARDINALITY-BUDGET).
SOURCE_IP=$(corelink admin audit get \
  --event-id <EVENT_ID> \
  --field source_ip)

# 2. Add a Cloudflare firewall rule blocking the IP for 24h.
wrangler firewall rule create \
  --action block \
  --expression "(ip.src eq ${SOURCE_IP})" \
  --notes "RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT §2.2; expires 24h"

# 3. Revoke the JWT (per §2.1).
```

### 2.3 Enumeration probe — Tier-3 escalation + per-tenant endpoint freeze

Decision criterion: ≥ 6 distinct attempts in 5 min, OR the source IP
already has a Cloudflare firewall block but new attempts are arriving
from adjacent IPs in the same ASN.

```bash
# 1. Page Tier-3 (Architect + Security Lead) immediately via the
#    direct schedule (bypasses the 5-min Tier-1→Tier-2 delay).
corelink oncall page \
  --schedule corelink-oncall-tier-3 \
  --severity sev1 \
  --runbook RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT \
  --note "enumeration probe; runbook §2.3"

# 2. Block the entire ASN at the edge for 4h (revisit at MTTR review).
wrangler firewall rule create \
  --action block \
  --expression "(ip.geoip.asnum eq <ASN>)" \
  --notes "RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT §2.3; expires 4h"

# 3. Disable the audit-export endpoint for the targeted tenant
#    (the attacker keeps trying that authenticated_tenant; per-tenant
#    freeze prevents the bytes from ever shipping while we investigate).
corelink admin tenant feature-flag set \
  --tenant-blake3-hex8 <TENANT_ID_HEX8> \
  --flag audit_export_disabled \
  --value true \
  --reason "RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT containment §2.3"
```

**Escalation trigger to SEV-0:** if the source becomes able to
authenticate as a tenant OTHER than the one being attempted (i.e. a
JWT was minted to impersonate), this is no longer a cross-tenant
attempt — it is a **token-mint compromise**. Page on-call Tier-3 +
Privacy Officer + Legal immediately and switch to
`RB-AUDIT-EXPORT-VERIFY-FAILED.md` §3 (immediate freeze) for the
forensics steps.

---

## 3. Investigation

### 3.1 D1 query — prior export attempts by the same tenant + correlation chain

```bash
# Full attempt history for the offending authenticated tenant, last 24h.
wrangler d1 execute corelink-audit \
  --command "SELECT
    event_type,
    occurred_at,
    attempted_tenant_blake3_hex8,
    exit_status,
    bytes_written,
    correlation_id,
    source_ip_country,
    source_asn
  FROM audit_events_index
  WHERE authenticated_tenant_blake3_hex8 = '<HEX8>'
    AND occurred_at >= datetime('now', '-24 hours')
    AND event_type IN (
      'corelink.audit.export_request.v1',
      'corelink.security.audit_export_cross_tenant_attempt.v1'
    )
  ORDER BY occurred_at DESC;"
```

Decision tree on the result:

| Pattern | Interpretation | Next |
|---|---|---|
| All rows are `cross_tenant_reject`, distinct attempted tenants | Enumeration probe (no successful read). | §4 customer comm path "probable compromise". |
| Mix of `ok` `export_request` rows + `cross_tenant_reject` rows | Legitimate operator on a compromised credential trying to fish | §4 customer comm path "credential rotation". |
| Single `cross_tenant_reject` row, no history | Misconfigured client (typo in tenant query param) | §4 customer comm path "configuration help". |

### 3.2 Correlation across tenants

Pull every distinct `attempted_tenant_blake3_hex8` and check whether
any of THOSE tenants subsequently saw a successful export within 24h
(an attacker who fails on tenant A might succeed on tenant B via a
different vector). Cross-reference with the JWT revocation log
(`corelink.security.jwt_revoked.v1`) to confirm the revoke landed.

### 3.3 Rate-limit telemetry

The audit-export endpoint runs behind a 1 request/min/tenant rate
limiter (`corelink-ratelimit::InMemoryTokenBucketRateLimiter`;
`audit_export.rs::audit_export_rate_limit_config`). If the
`corelink_audit_export_requests_total{exit_status="rate_limited"}`
counter for the attacker's tenant grew in lockstep with the
cross-tenant counter, the attacker hit the floor — confirms an
automated tool (humans don't hit the 60s floor consistently).

---

## 4. Customer comm (if attacker was a legitimate tenant in compromised state)

If §3.1 surfaces the "legitimate operator on a compromised credential"
pattern, the customer must be notified. Coordinate with Privacy +
Legal before sending.

### 4.1 Template — credential-rotation notice (SEV-1)

> Subject: `[Action required] CoreLink detected an unusual access pattern on your account`
>
> Hi `<name>`,
>
> Our audit-export endpoint rejected `<N>` requests from your account
> in the last `<window>` that attempted to read audit data belonging
> to other tenants. The rejections fired our SEV-1 security alarm.
>
> We have **revoked the offending session token (jti `<jti-prefix>`)**
> and **blocked the source IP at our edge for 24 hours**. No data
> belonging to any other tenant was disclosed — every attempt was
> blocked at the authentication boundary BEFORE any bytes shipped.
>
> **Action required:**
> 1. Rotate the API key used to mint the offending token (instructions:
>    `https://corelink.humangr.com/docs/security/rotate-api-key`).
> 2. Review the activity log in your tenant dashboard for the same
>    24-hour window.
> 3. Confirm receipt within 24h to acknowledge.
>
> If you did not initiate this activity, treat this as a confirmed
> credential compromise — see `https://corelink.humangr.com/docs/security/compromise-response`.
>
> — CoreLink Security

### 4.2 Template — configuration-help (single probe)

For the single-probe / misconfiguration case, an internal email
(NOT a security incident notice) is sufficient. Reference
`apps/docs/docs/how-to/export-audit-log.mdx` and clarify the
`tenant` query parameter is for cross-tenant-attempt detection only,
NOT a tenant selector.

### 4.3 LGPD / GDPR posture

A cross-tenant **attempt** that the endpoint rejected does NOT
constitute a personal-data breach under GDPR Art. 4(12) / LGPD Art.
46 — no unauthorized access occurred (the bytes never shipped). The
**72-hour notification clock does NOT start** for cross-tenant
attempts. (It DOES start for `RB-AUDIT-EXPORT-VERIFY-FAILED`.)

Document the decision in the post-incident memo (§7); the DPO
reviews the SEV-1 incident at the weekly compliance review per
`RB-COMPLIANCE-WEEKLY-REVIEW.md`.

---

## 5. Post-incident audit-chain integrity verify

Run the daily-verify CLI manually for the offending authenticated
tenant + every distinct attempted tenant from §3.1:

```bash
# For each tenant_id (resolved from the BLAKE3 hex8 via the admin
# audit-viewer):
corelink audit verify \
  --tenant <TENANT_ID> \
  --since-ms $(date -d '-25 hours' +%s)000 \
  --until-ms $(date +%s)000 \
  --include-merkle-proofs
```

If verify returns `chain_verified_ok`, the audit chain remains intact
for every involved tenant (the attempt was caught at the auth boundary
without ever reaching the chain mutator) — close the SEV-1 with §6.
If verify surfaces ANY `ChainBreak`, escalate to SEV-0 immediately
and switch to `RB-AUDIT-EXPORT-VERIFY-FAILED.md`.

---

## 6. MTTR target + closure

**MTTR target ≤ 24h** per S-17 ops maturity baseline. Closure
checklist:

- [ ] JWT revoked (§2).
- [ ] Source IP (and ASN, if §2.3) blocked at the Cloudflare edge.
- [ ] §3.1 query run; pattern documented in incident memo.
- [ ] §5 post-incident chain verify GREEN for every involved tenant.
- [ ] §4 customer comm sent (when applicable).
- [ ] Incident memo filed at
      `specs/_post_mortems/PM-AUDIT-EXPORT-CROSS-TENANT-<YYYY-MM-DD>-<ticket>.md`
      within 5 business days.

---

## 7. Post-incident actions

For every SEV-1 / SEV-0:

1. Post-mortem within 5 business days
   (`specs/_post_mortems/PM-AUDIT-EXPORT-CROSS-TENANT-<YYYY-MM-DD>-<ticket>.md`).
2. If the same tenant repeats the cross-tenant pattern: open
   WI-S17-OPS-AUDIT-EXPORT-TENANT-RECIDIVISM to add a recidivism-
   tracking metric + permanent per-tenant block flag.
3. If the attempt came from a customer-side CLI bug (the customer
   never intended cross-tenant access): patch the CLI + ship a
   release-notes warning. Drata evidence task `EVT-2026-XXX-cli-
   patch`.
4. Tag the runbook execution in the runbook-drill tracker:

   ```bash
   corelink runbook-drill record \
     --runbook-id RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT \
     --executor op_<id> \
     --evidence <asciinema-url> \
     --duration-seconds <ACTUAL> \
     --expected-seconds 86400
   ```

---

## 8. Fitness function

This runbook MUST be drilled quarterly via a synthetic cross-tenant
attempt fired against the staging audit-export endpoint
(`corelink admin chaos audit-export-cross-tenant-attempt`). Expected
end-to-end drill duration **2 hours** (compressed from the 24h MTTR
target — the drill validates the path, not the regulatory wait). Drift
> 2× (4h) triggers FM-202 review per `corelink-runbook-tracker`.
