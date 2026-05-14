---
id: "ADR-S11-011"
type: "adr"
doc_status: "ACCEPTED"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["adr", "s11", "region-migration", "cooldown", "privacy", "lgpd", "gdpr"]
---

# ADR-S11-011: Region Migration Cooldown 30 Days

## Status

ACCEPTED

## Context

`privacy_model.md §7.2` specifies: "Mudança de região = pedido formal + migração
+ cooldown 30d". The 30-day cooldown ensures Privacy Officer + Compliance review
before data migration proceeds. The question is whether 30 days is the correct
window.

## Decision

Region migration cooldown is **30 days** from `tenant.primary_region` last-set
timestamp. Migration request is submitted via `POST /v1/admin/tenant/region-migration`
and is not executable until cooldown elapses AND Privacy Officer + Compliance
approve the ticket.

## Rationale

1. **Regulatory alignment**: LGPD Art. 33 §1º + GDPR Art. 46 require Transfer
   Impact Assessment (TIA) for cross-border transfers; 30d gives adequate time
   for TIA review + DPA update if needed.

2. **Data migration tooling window**: S-13 admin plane data migration tool requires
   time to schedule migration without business interruption; 30d creates a safe
   scheduling window.

3. **Customer expectation alignment**: Tenants choosing a region at signup implicitly
   consent to data residency; 30d cooldown signals the seriousness of the commitment
   and allows tenants to plan the migration.

4. **Privacy Officer review**: Compliance reviews typically require 5-10 business
   days; 30d is generous enough to avoid SLA pressure on reviewers.

## Alternatives Considered

- **7 days** (rejected): Insufficient for TIA review + DPA update + scheduling.
- **60 days** (rejected): Too conservative; unnecessary UX friction for legitimate
  region migration requests.
- **14 days** (considered): Reasonable but does not provide adequate buffer for
  complex enterprise tenants with large data volumes.

## Consequences

- `InMemoryMigrationStore.submit()` enforces 30d cooldown at application layer.
- D1 `region_migration_request.created_at_ms` is the cooldown reference timestamp.
- Privacy Officer + Compliance sign-off required before `status = 'approved'`.
- Email notification sent to tenant at ticket submission explaining 30d cooldown.
- ADR-S11-011 referenced in Privacy Notice (WI-S11-004) region migration section.

## Sign-off

- Privacy Officer: _pending pre-merge_
- Compliance: _pending pre-merge_
- Architect: _pending pre-merge_
