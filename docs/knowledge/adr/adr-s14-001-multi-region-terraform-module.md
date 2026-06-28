---
type: "ADR"
title: "ADR-S14-001 — Multi-region Terraform module + per-region KV namespace + DO EU jurisdiction"
description: "Why CoreLink's 4-region infra uses one reusable Terraform module, a per-region KV namespace, a pinned EU DO jurisdiction, and a Rust migration binary."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md"
  - "apps/migrate-single-to-multi-region/src/main.rs"
  - "crates/corelink-container/src/storage/region_map.rs"
checkpoint_sha: "03c2ae27deb7094fea4009927b90959533dae21e"
provenance: "AUTHORED"
tags: ["adr", "s14", "region", "terraform", "residency"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-001 — Multi-region Terraform module + per-region KV namespace + DO EU jurisdiction

S-14 stands up CoreLink's four production regions (WNAM/ENAM/WEUR/SAM) as the foundation for everything residency-related. This ADR (DRAFT) bundles four infrastructure decisions whose forcing factors are HIGH_RISK: cross-region tenant isolation (FF-HR-002) and EU PII residency as a regulatory absolute (FF-HR-003). The throughline is preventing infrastructure-level data leaks that would constitute a Schrems II violation.

# Context

The four regions are the foundation layer for S-14, and four architectural decisions carry HIGH_RISK residency implications: any infra bug can introduce cross-region tenant leaks (FF-HR-002, → Schrems II), EU data must not transit non-EU infra (FF-HR-003, regulatory absolute), and KV is globally distributed by CF design so explicit namespace scoping is required (FM-054) (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:24-37`).

# Decision

- **Decision 1 — reusable Terraform module** `infra/terraform/modules/corelink-region/` with per-region inputs and `validation {}` blocks, rather than per-region copy-paste HCL whose drift over 4 regions × many sprints produces silent compliance gaps (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:39-52`).
- **Decision 2 — per-region KV namespace** `corelink-session-{region}` scoped in the Worker binding, because a single global KV namespace lets a WEUR Worker be served by WNAM KV nodes (FM-054 = EU data on US infra = Schrems II); a key-prefix scheme is rejected since CF KV does not enforce read isolation by prefix (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:58-71`).
- **Decision 3 — DO `jurisdictional_restriction = "eu"` for WEUR**, enforced by a post-deploy `verify_do_jurisdiction.sh` CI gate because CF provider 4.x does not expose the field in Terraform; an unpinned DO may route to US edge by capacity, violating Schrems II + GDPR Art. 46 (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:77-96`).
- **Decision 4 — Rust migration binary** `apps/migrate-single-to-multi-region/` with dry-run/execute/rollback modes and atomic audit emission, chosen over bash/Python for type-safe region enums, idempotent re-run, and a real audit trail (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:98-118`). The binary is built: `main` dispatches the three modes (rollback / execute / dry-run-default) (`apps/migrate-single-to-multi-region/src/main.rs:96-106`), and the execute path emits a per-tenant audit record BEFORE any state mutation, failing CLOSED on an audit-sink error (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER) (`apps/migrate-single-to-multi-region/src/main.rs:225-234`).

# Consequences

- Activates INV-DATA-RESIDENCY, INV-REGION-NO-CROSS-LEAK, and INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER as CRITICAL invariants for the region layer (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:120-124`).
- Operationally: module breaking changes require a major bump + migration plan; the per-region `.tf` files are thin wrappers; 4 KV namespaces (~$20/mo) are provisioned; and the WEUR deploy checklist must run `verify_do_jurisdiction.sh` to exit 0 (`specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:54-56`, `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:72-75`, `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:93-96`).

Runtime enforcement of this region pinning is the subject of [ADR-S14-002 — Region pinning enforcement](/adr/adr-s14-002-region-pinning-enforcement.md).

# Status vs shipped code

The **artifacts** this ADR mandates exist in the repo — the reusable Terraform module and the Rust migration binary (`apps/migrate-single-to-multi-region/src/main.rs:96-106`) are both present — but the **deployed reality is US-only**. All R2 buckets are ENAM (R2 has no SA region), so the Consequences as written — "four production regions stood up (WNAM/ENAM/WEUR/SAM)" and "4 KV namespaces provisioned" — are **not live**; the region topology is a built-and-tested module applied to a single live region, not four standing regions. The container's `region_map` carries `PROVISIONED_MACROS = {wnam, enam, weur}` (3, NOT 4 — `sam` is EXCLUDED: it stays routable but is NOT provisionable until PROD_SAM has a real SAM-jurisdiction bucket, else its data mis-lands in US R2) as the Phase-1 set (`crates/corelink-container/src/storage/region_map.rs:47`), but that is the macro-mapping table, not evidence of four provisioned regions. Treat the multi-region infra as designed-and-coded, US-only-deployed.

# Citations

1. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:24-37` — the FF-HR-002 / FF-HR-003 / FM-054 forcing factors.
2. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:39-71` — the reusable-module and per-region-KV-namespace decisions.
3. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:77-118` — the EU DO jurisdiction gate and the Rust migration-binary decision.
4. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:120-124` — the CRITICAL invariants activated.
5. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:54-56` — module-breaking-change consequence.
6. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:72-75` — KV-namespace provisioning consequence.
7. `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md:93-96` — WEUR deploy-checklist consequence.
8. `apps/migrate-single-to-multi-region/src/main.rs:96-106` — `main` dispatches the rollback / execute / dry-run-default migration modes (Decision 4's type-safe binary, designed + built).
9. `apps/migrate-single-to-multi-region/src/main.rs:225-234` — execute mode emits the per-tenant audit record BEFORE state mutation and fails CLOSED on a sink error (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
