---
id: "ADR-S14-001"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s14", "region", "terraform", "multi-region", "kv-namespace", "do-jurisdiction", "migration", "high-risk"]
---

# ADR-S14-001 — Multi-Region Terraform Module `corelink-region` + Per-Region KV Namespace Prefix + Migration Script

## Status

DRAFT — pending Architect + SRE Lead + Security Lead + Compliance Officer ratification.

## Context

WI-S14-001 introduces CoreLink multi-region production infrastructure (WNAM/ENAM/WEUR/SAM) as the foundation layer for S-14. Four key architectural decisions are captured here:

1. **Reusable Terraform module** vs. per-region copy-paste.
2. **Per-region KV namespace prefix** vs. single global KV namespace.
3. **DO `jurisdictional_restriction = "eu"` for WEUR** vs. global routing.
4. **Rust migration script** vs. bash/ad-hoc tooling for tenant data migration.

These decisions have HIGH_RISK implications (FF-HR-002: cross-region tenant isolation; FF-HR-003: PII residency regulatory absolute).

Forcing factors:
- **FF-HR-002**: Any infrastructure bug can introduce cross-region tenant data leaks → Schrems II violation.
- **FF-HR-003**: EU data must not transit non-EU infrastructure → regulatory absolute.
- **FM-054**: KV is globally distributed by CF design → explicit namespace scoping required.

## Decision 1: Reusable Terraform Module `corelink-region`

**Decision:** Use reusable Terraform module `infra/terraform/modules/corelink-region/` with per-region inputs (`region_name`, `r2_location_hint`, `d1_location`, `do_jurisdiction`, `cf_zone_id`, `cf_account_id`) rather than per-region copy-paste HCL files.

**Rationale:**
- Copy-paste drift is inevitable over time. WNAM/ENAM/WEUR/SAM start identical; divergence over 4 regions × multiple sprints = silent compliance gaps.
- Module validation rules (Terraform `validation {}` blocks) enforce invariants at plan time: `do_jurisdiction ∈ {none, eu, us}`, `region_name ∈ {wnam, enam, weur, sam}`.
- Future regions (APAC/AFR demand-driven, pós-GA) trivially added via new invocation.
- Consistent naming enforced: `corelink-cas-{region}`, `corelink-meta-{region}`, `corelink-session-{region}`, `corelink-do-{region}`.
- Single source of truth for resource structure; changes propagate to all regions via module update.

**Alternatives rejected:**
- Per-region copy-paste: rejected (drift risk, inconsistent compliance, maintenance overhead).
- Terragrunt: overkill for 4-region scale; adds toolchain complexity; Terraform native modules sufficient.

**Consequences:**
- Module breaking changes require major version bump + ADR + migration plan (14.s14.001.8).
- Per-region invocation files (`wnam.tf`, `enam.tf`, `weur.tf`, `sam.tf`) are thin wrappers — all logic in module.

## Decision 2: Per-Region KV Namespace Prefix (FM-054 Prevention)

**Decision:** Each region gets a dedicated KV namespace `corelink-session-{region}` (titles: `corelink-session-wnam`, `corelink-session-weur`, etc.). Worker bindings scope KV reads/writes to per-region namespace only.

**Rationale:**
- Cloudflare KV is globally distributed by design — a single namespace means WEUR Workers can (by proximity or capacity) be served by KV nodes in WNAM.
- FM-054 (KV global namespace cross-region leak): tenant cache in WEUR served from WNAM = EU data on US infrastructure = Schrems II violation.
- Per-region namespace enforced in Worker binding (`SESSION_KV` binding points to `corelink-session-{region}` namespace).
- INV-REGION-NO-CROSS-LEAK requires this baseline; WI-S14-002 property test (30k) validates zero cross-region reads.

**Alternatives rejected:**
- Single global KV namespace with key prefix (`{region}:{tenant_id}:{key}`): CF KV does not enforce read isolation by key prefix — Worker code could bypass the prefix. Requires application-level enforcement only; infrastructure-level enforcement preferred for defense-in-depth.
- KV per-tenant namespace: cardinality explosion (10k tenants × N regions = unsustainable).

**Consequences:**
- 4 KV namespaces provisioned (one per region); ~$5/month each = $20/month baseline.
- Worker binding `SESSION_KV` scoped to per-region namespace; cross-region KV reads technically impossible at infrastructure level.
- Runbook RB-FM-054 dry-run required in WI-S14-009.

## Decision 3: DO `jurisdictional_restriction = "eu"` for WEUR

**Decision:** WEUR Durable Object Workers MUST have `jurisdictional_restriction = "eu"` set post-deploy. CI gate (`verify_do_jurisdiction.sh` exit code check) fails PR if WEUR jurisdiction ≠ "eu".

**Rationale:**
- Schrems II + GDPR Art. 46: EU personal data must not transit non-EU infrastructure.
- Cloudflare DO without `jurisdictional_restriction` may route to US edge by capacity decision.
- `jurisdictional_restriction = "eu"` pins DO execution to EU infrastructure — CF guarantees execution in EU edge PoPs.
- Risk R-003 (WI-S14-001 risk register): "DO jurisdiction not set" → probability=Low but impact=CRITICAL (legal + regulatory).
- Post-deploy verification via `verify_do_jurisdiction.sh` provides defense-in-depth beyond Terraform (since CF provider 4.x doesn't expose this field directly).

**Limitations:**
- CF provider 4.52.x does not expose `jurisdictional_restriction` in `cloudflare_workers_script` resource → requires post-deploy API call or Dashboard setting.
- Terraform cannot enforce this at plan time; verification script is the enforcement gate.
- This limitation is tracked in WI-S14-009 PRR checklist; CF provider version monitoring in quarterly review.

**Consequences:**
- WEUR deploy checklist MUST include `verify_do_jurisdiction.sh --region weur` → exit 0.
- Exit 2 from script = CRITICAL + halt + Compliance Officer escalation.
- Quarterly config audit includes DO jurisdiction verification per region.

## Decision 4: Rust Migration Script (vs. Bash/Ad-hoc)

**Decision:** Tenant data migration (single-region → multi-region) implemented as Rust binary `apps/migrate-single-to-multi-region/` with structured dry-run/execute/rollback modes and audit emission.

**Rationale:**
- Pattern reused from S-12 `rb_fm_156_dry_run.rs` (established convention).
- Type safety: Region enum enforces valid target regions at compile time.
- Structured error handling: `MigrationDecision` taxonomy allows idempotent re-run (SkippedAlreadyMigrated = safe re-run).
- Audit emission: `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` enforced in execute mode.
- Bounded memory: streaming R2 copy (14.s14.001.9 quality standard).
- Bash alternative: no type safety, no idempotency guarantee, no audit trail, no rollback mode.

**Alternatives rejected:**
- Bash script: fragile, no type safety, error handling ad-hoc.
- Python script: valid but diverges from Rust-first codebase convention; no wasm32-clean property.
- Ad-hoc CF API calls: no idempotency, no dry-run, no rollback.

**Consequences:**
- Migration binary built and tested as part of CI (cargo test -p migrate-single-to-multi-region).
- Dry-run mandatory pre-execute (operator policy + runbook enforcement).
- Rollback path output guides operator through Terraform state revert + D1 PITR + R2 restore.

## Invariants Activated

- **INV-DATA-RESIDENCY** (CRITICAL, S-11 herdada): module validates location hints; verify scripts enforce post-deploy.
- **INV-REGION-NO-CROSS-LEAK** (CRITICAL, new S-14): per-region KV namespace + WI-S14-002 insert checks.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL, S-03 herdada): migration script emits audit before state mutation.

## References

- WI-S14-001 §§1, 2, 6, 9 (design decisions)
- WI-S14-001 risk register R-001..R-012
- FM-054 (KV global namespace cross-region leak)
- FF-HR-002 (cross-region tenant isolation)
- FF-HR-003 (residency PII regulatory absolute)
- Schrems II (EDPB Recommendations 01/2020)
- GDPR Art. 46 (transfers to third countries)
- LGPD Art. 33 (international data transfers)

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo Schneiter (via Claude Sonnet 4.6) | Criação ADR-S14-001 (WI-S14-001 implementation). |
