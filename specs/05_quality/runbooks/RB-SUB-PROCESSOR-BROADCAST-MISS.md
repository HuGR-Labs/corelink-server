---
id: "RB-SUB-PROCESSOR-BROADCAST-MISS"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-28"
updated: "2026-04-28"
owner: "Privacy Officer"
final_approver: "Privacy Officer"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "rb", "sub-processor", "broadcast", "fm-453", "s11", "stub", "planned"]
---

# RB-SUB-PROCESSOR-BROADCAST-MISS — Sub-Processor Change Broadcast Failure (Email Delivery Drop)

> **Status: PLANNED stub (Lote 10.11.0-bis-prime cycle 14)** — full SOP a ser drafted pré-PRR por Privacy Officer + SRE.
> **FM:** [FM-453](../../03_architecture/failure_modes.md) (sub-processor breach upstream OR broadcast channel down) | **WI:** WI-S11-005 | **CTRL:** CTRL-PRIV-021 (sub-processor transparency) | **SLA:** detect ≤ 1h, remediate ≤ 24h (continue 30d window)

## Pré-condições

- S-11 sub-processor register auto-gen `/privacy/sub-processors` (Cloudflare Pages).
- 30d email broadcast cron worker (Cloudflare Email transactional + DKIM signed).
- Per-recipient delivery confirmation tracked em D1 `sub_processor_broadcast_log`.

## Detecção

### Sinais primários

- Broadcast cron job marked `failed` em PagerDuty.
- Delivery confirmation rate < 95% (target 98%) — pode indicar DKIM record mismatch, transactional sender outage, or quota exhaustion.
- Customer report: "didn't receive sub-processor change notification".
- Métrica `corelink_subprocessor_broadcast_delivery_total{outcome="failed"}` > threshold.

## Step 1: Triage (≤ 1h)

1. Identify failure class: `email_outage | dkim_mismatch | quota_exhaustion | broadcast_log_drift`.
2. Check Cloudflare Email transactional health.
3. Verify DKIM record (DNS TXT) matches HKDF info=`corelink/v1/dkim-broadcast` derivation.

## Step 2: Mitigation (≤ 24h)

- **email_outage**: failover to backup transactional sender (S-13 deferred); queue messages.
- **dkim_mismatch**: rotate DKIM key + update DNS TXT record (RB-KEY-COMPROMISE if compromise suspected).
- **quota_exhaustion**: temporary quota increase via Cloudflare Email; root-cause throttling.
- **broadcast_log_drift**: reconcile D1 sub_processor_broadcast_log via re-broadcast for missed recipients.

## Step 3: 30d window protection

- Extend 30d notice window if delivery delayed > 24h (Privacy Officer documented decision).
- Direct customer outreach for high-value enterprise customers.

## Post-incident

- Post-mortem: why broadcast failed?
- Update PAT-RETRY-IDEMPOTENT-001 broadcast retry policy if needed.
- Verify FM-453 mitigation effective.

## References

- `WI-S11-005` sub-processor register + 30d broadcast + objection.
- `failure_modes.md` FM-453.
- `compliance_matrix.md` §7 sub-processor compliance posture.
- GDPR Art. 28.2 + LGPD Art. 39.
