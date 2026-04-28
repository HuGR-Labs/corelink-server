---
id: "RB-DSR-INTAKE-FAILURE"
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
tags: ["runbook", "rb", "dsr", "intake", "wi-s11-001", "s11", "stub", "planned"]
---

# RB-DSR-INTAKE-FAILURE — DSR Intake Failure / API Down / Rate Limit Storm

> **Status: PLANNED stub (Lote 10.11.0-bis-prime cycle 14)** — full SOP a ser drafted pré-PRR por SRE + Privacy Officer.
> **WI:** WI-S11-001 (DSR API 7 endpoints) | **CTRL:** CTRL-PRIV-022 (DSR self-service) + CTRL-AUTH-010 (step-up MFA) | **SLA:** detect ≤ 5min, mitigate ≤ 1h, customer comms ≤ 4h

## Pré-condições

- S-11 DSR API live (`/v1/privacy/dsr/{access|correction|erasure|portability|objection|consent_revoke|confirmation}`).
- Rate limit 10/dia/subject (S-08 inheritance).
- step-up MFA WebAuthn (S-03 inheritance + CTRL-AUTH-010).

## Detecção

### Sinais primários

- DSR API 5xx error rate > 1% sustained 5min.
- Rate limit denied count > 50/h (potential abuse OR misconfiguration).
- step-up MFA verify failure rate > 5% (UI bug, WebAuthn integration drift, or attack).
- Métrica `corelink_dsr_intake_failure_total` > threshold.

## Step 1: Triage (≤ 5min)

1. Identify failure class: `api_5xx | rate_limit_storm | mfa_failure | neon_unavailable`.
2. Check downstream: Neon `dsr_tickets` write availability; KV session integrity; PagerDuty `corelink-dsr-intake`.
3. If 5xx storm: enable degrade mode (read-only DSR API + queue submissions).

## Step 2: Mitigation (≤ 1h)

- **api_5xx**: rollback last deploy if regression; escalate to Architect.
- **rate_limit_storm**: investigate source (single subject vs distributed); humane appeal path per LGPD Art. 20 review.
- **mfa_failure**: verify WebAuthn assertion service health; check security_model.md §242 CTRL-AUTH-010.
- **neon_unavailable**: failover to read-only mode; queue submits for replay; alert on-call SRE.

## Step 3: Customer comms (≤ 4h)

- Status page banner if widespread.
- Direct email to affected DSR subjects via Cloudflare Email transactional (3 locales).

## Post-incident

- Post-mortem.
- SLO impact: DSR submission latency p99 + availability.
- Update RB if new failure class encountered.

## References

- `WI-S11-001` DSR API 7 endpoints + JWT receipt + step-up MFA.
- `failure_modes.md` FM-450 (erasure pipeline failure).
- `invariant_registry.md` INV-AUDIT-APPEND-ONLY CRITICAL.
- `privacy_model.md` §6.1 DSR rights + clock semantics F-11.
- LGPD Art. 18 + GDPR Art. 15-22.
