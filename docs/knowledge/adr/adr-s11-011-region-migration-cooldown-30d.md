---
type: "ADR"
title: "ADR-S11-011 — Region Migration Cooldown 30 Days"
description: "Why a tenant's region migration is gated by a 30-day cooldown from primary_region last-set, plus Privacy Officer + Compliance approval, before data may move."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "region-migration", "cooldown", "lgpd", "gdpr"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-011 — Region Migration Cooldown 30 Days

Changing a tenant's data-residency region is a serious, regulated act: it can trigger a cross-border Transfer Impact Assessment and a DPA update. This ADR fixes the cooldown window at 30 days from the tenant's `primary_region` last-set timestamp, so that Privacy Officer + Compliance always have time to review before any data moves — balancing regulatory diligence against unnecessary UX friction.

# Context

`privacy_model.md §7.2` specifies "Mudança de região = pedido formal + migração + cooldown 30d." The 30-day window exists to guarantee Privacy Officer + Compliance review before migration proceeds; the decision is whether 30 days is the right length (`specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:23-28`).

# Decision

The migration cooldown is **30 days** from `tenant.primary_region` last-set; a request submitted via `POST /v1/admin/tenant/region-migration` is not executable until the cooldown elapses **and** Privacy Officer + Compliance approve the ticket (`specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:30-35`). Rationale: LGPD Art. 33 §1º + GDPR Art. 46 require a TIA for cross-border transfers and 30d allows TIA review + DPA update; the S-13 admin data-migration tool needs a safe scheduling window; the cooldown signals the seriousness of a residency commitment; and Compliance reviews typically take 5–10 business days, which 30d comfortably accommodates (`specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:37-52`). 7d (too short for TIA + scheduling) and 60d (excessive friction) were rejected; 14d was considered but lacked buffer for large enterprise tenants.

# Consequences

The migration store enforces the 30d cooldown at the application layer with `region_migration_request.created_at_ms` as the reference timestamp; Privacy Officer + Compliance sign-off is required before `status = 'approved'`; a tenant email at submission explains the cooldown; and this ADR is referenced in the Privacy Notice region-migration section (`specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:62-69`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:23-28` — Context: privacy_model.md §7.2 cooldown requirement.
2. `specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:30-35` — Decision: 30d from primary_region last-set + dual approval gate.
3. `specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:37-52` — Rationale: TIA window, scheduling, commitment signal, reviewer time.
4. `specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md:62-69` — Consequences: enforcement, reference timestamp, sign-off, notice reference.
