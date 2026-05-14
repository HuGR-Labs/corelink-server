---
id: "ADR-S11-008"
type: "adr"
doc_status: "ACCEPTED"
version: "2.0.0"
created: "2026-04-26"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["adr", "s11", "sub-processor", "notification", "gdpr", "lgpd", "legal-obligation"]
---

# ADR-S11-008: Mandatory Sub-Processor Notifications for ALL 5 Canonical Plans

## Status

**ACCEPTED** (v2.0.0 — Lote 10.11.0-bis REVISED)

## Context

WI-S11-005 implements sub-processor transparency per GDPR Art. 28.2 + LGPD Art. 39.
A key design decision is: which customer plans receive 30-day advance notice emails
when sub-processors change?

**ADR-S11-008 v1.0** (initial) proposed tier-gating:
- `team`+ plans: auto-subscribed
- `free`/`solo` plans: opt-in only

The rationale was LGPD Art. 6 (minimização — avoid spam for casual users).

## Decision (v2.0.0 — Lote 10.11.0-bis)

**REVERSED: ALL 5 canonical plans receive mandatory sub-processor notifications.**

Plans: `free`, `solo`, `team`, `business`, `enterprise`.

The `sub_processor_notifications` purpose has **`legal_obligation`** basis
(privacy_model.md §5.6.1 canonical enum) — NOT opt-out-able via consent_revoke.
Only `marketing_email` is opt-out via consent. Sub-processor transparency is a
regulatory right, not an email marketing campaign.

## Rationale

1. **GDPR Art. 28.2**: "The processor shall inform the controller of any intended
   changes concerning the addition or replacement of other processors, thereby giving
   the controller the opportunity to object to such changes." — No tier carve-out.

2. **LGPD Art. 39**: "O operador deverá realizar o tratamento segundo as instruções
   fornecidas pelo controlador, que verificará a observância das próprias instruções
   e das normas sobre a matéria." — No tier carve-out.

3. **EDPB Guidelines 7/2020 §125**: 30-day notification is industry standard for
   ALL data controllers, regardless of their subscription tier.

4. **Tier-gating would violate regulatory transparency**: A `free` plan customer
   still enters into a Data Processing Agreement (DPA) with HuGR. The GDPR Art. 28.2
   right to object to sub-processor changes applies to ALL DPA counterparties.

5. **LGPD Art. 6 minimização does NOT override LGPD Art. 39**: The minimização
   principle applies to personal data collection, not to legal notification obligations.
   Sub-processor transparency notification is a legal obligation that OVERRIDES
   anti-spam minimization arguments.

## Correction to v1.0

ADR v1.0 conflated:
- `marketing_email` (opt-in/opt-out via consent — legitimate interest)
- `sub_processor_notifications` (legal obligation — NOT opt-out-able)

This correction aligns with privacy_model.md §5.6.1 canonical purpose taxonomy.

## Consequences

- All 5 canonical plans receive 30d advance notice emails on sub-processor change.
- `sub_processor_notifications` purpose in consent_ledger has `legal_obligation` basis.
- Customers cannot opt out via `POST /v1/privacy/dsr/consent_revoke` for this purpose.
- Broadcast log (`sub_processor_broadcast_log`) seeds ALL subscribed tenants.
- Privacy Officer + Compliance + Legal sign-off required before any tier-gating reintroduction.

## Risks

- More emails to `free`/`solo` tier users (perceived spam risk).
- **Mitigation**: Email template is transactional + regulatory in nature (not promotional);
  DKIM signed; 30-day cadence is extremely low frequency (only on sub-processor changes).

## Sign-off

| Role | Status |
|---|---|
| Owner (Gustavo Schneiter) | Approved |
| Privacy Officer | Mandatory — legal_obligation basis decision |
| Compliance | Mandatory — SOC 2 CC9.2 + GDPR Art. 28.2 alignment |
| Legal | Mandatory — DPA clause defensibility + LGPD Art. 39 alignment |

## Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 2.0.0 | 2026-05-13 | Gustavo Schneiter (WI-S11-005 impl) | REVERSED v1.0 tier-gating; mandatory all-plans; legal_obligation basis canonical |
| 1.0.0 | 2026-04-26 | Gustavo Schneiter (Lote 10.11) | Initial ADR proposing team+ tier-gating (LGPD Art. 6 minimização rationale — SUPERSEDED) |
