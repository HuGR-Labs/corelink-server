---
id: "RB-DATA-RESIDENCY-LEAK"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "privacy", "residency", "schrems-ii", "gdpr", "lgpd"]
---

# RB-DATA-RESIDENCY-LEAK — Residency Leak (Cross-Region Tenant Data Exposure / Schrems II Risk)

> **INV:** INV-DATA-RESIDENCY CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL Schrems II) + INV-REGION-NO-CROSS-LEAK CRITICAL | **CTRL:** CTRL-PRIV-031 | **SLA:** detect ≤ 1h, mitigate ≤ 6h, customer notification ≤ 72h

## Pré-condições

- Multi-region: 6 regiões canonical pós Lote 10.11.0-bis (wnam/enam/weur/sam Phase 1 production live; apac/afr Phase 2/3 deferred per privacy_model.md §7.1; data_model.md §4.1 `tenant.primary_region` CHECK constraint enforced for all 6 enum values).
- Tenant region pinned via Neon `tenant.primary_region` (canonical pós Lote 10.11.0-bis; data_model.md §4.1 — NÃO `tenant_metadata.region_pinned` legacy).
- Insert checks reject cross-region writes.
- 20k property test verde S-11 (R-S11-19); 30k S-14 R-S14-19 deferred (cross-region routing semantics).
- Schrems II TIA template + DPA amendment prontos (S-14 + S-11).

## Detecção

### Sinais primários

- Property test cross-region detect cross-region blob (S-11: 20k baseline; S-14: 30k expandido).
- Customer report: "I'm EU-pinned and my data is appearing in US logs/queries".
- Audit log reveals routing bug: cross-region blob access events.
- Métrica `corelink.region.cross_region_violation_total` > 0.

### Detection queries

```sql
-- Blobs em wrong region per tenant pinning
SELECT
  bm.digest,
  bm.region as blob_region,
  t.primary_region as expected_region,  -- canonical pós Lote 10.11.0-bis; data_model.md §4.1
  bm.tenant_id,
  bm.created_at
FROM blob_meta bm
JOIN tenant t ON t.tenant_id = bm.tenant_id  -- canonical Neon `tenant` table NÃO legacy `tenant_metadata`
WHERE bm.region != t.primary_region
  AND bm.deleted_at IS NULL
ORDER BY bm.created_at DESC
LIMIT 100;
```

### Métricas

```
corelink.region.cross_region_violation_total{tenant_tier, source_region, target_region}
corelink.region.tenant_region_mismatch_total
corelink.region.audit_residency_check_fail_total
```

## Comunicação

- **Severidade**: **SEV-1** (Schrems II / GDPR Art. 44 catastrophic legal exposure).
- **Page**: Privacy Officer + Legal + Security + SRE + Architect.
- **Internal channel**: `#incidents-corelink-privacy` (CRITICAL escalation).
- **Customer notification**: REQUIRED within 72h (GDPR Art. 33).
- **Regulatory notification**:
  - **EU tenant affected**: Lead supervisory authority (Irish DPC) within 72h GDPR Art. 33.
  - **LGPD applicable**: ANPD notification per Art. 48 + Resolução CD/ANPD nº 15/2024 (incident communication; substitui Res. 2/2022 fiscalização — correção Lote 10.11.0-ter).
  - **DPC consideration**: cross-border transfer = Schrems II implications; legal basis review.
- **Status page**: customer-facing notification (transparency = trust signal).
- **DPO interim** (Gustavo) involvement mandatory.

## Mitigação imediata (≤ 6h)

### Step 1: Stop the bleeding (≤ 30 min)
1. **Pause writes globally** to wrong region: `gc-pause` style degrade flag (PAT-DEGRADE-001).
2. **Identify scope**:
   - Which tenants affected?
   - Which records cross-region?
   - How many records?
   - Time window of leak.
3. **Preserve evidence**: snapshot D1 + R2 audit logs antes any cleanup.

### Step 2: Migrate data to correct region (≤ 6h)
1. For each affected blob:
   - **Hash verify** integrity em both regions.
   - **Copy to correct region** (per `tenant.primary_region` canonical pós Lote 10.11.0-bis; data_model.md §4.1).
   - **Verify hash post-copy**.
   - **Delete from wrong region** (após verify).
2. Update D1 `blob_meta.region` per fix.
3. Audit emit `corelink.privacy.residency_leak` event per affected record com:
   - `tenant_id, blob_digest, leaked_to_region, expected_region, exposure_window_seconds, fix_executed_at`.

### Step 3: Customer + regulatory notification (≤ 72h)
1. Customer outreach per affected tenant:
   - Transparency: explanation + scope + exposure window.
   - Apology + remediation offer (SLA credit + extended monitoring).
   - Customer attestation se enterprise + DPA active.
2. **Regulatory** (Privacy Officer + Legal decide):
   - GDPR Art. 33 trigger: notify Irish DPC within 72h.
   - LGPD: ANPD notification "se risco/dano relevante".
   - CCPA: state AG.
3. Internal: Compliance officer + audit log retention 7y.

### Step 4: Block recurrence (≤ 24h)
1. Insert checks reinforced (root cause fix).
2. Property test S-11 baseline 20k → S-14 30k → 100k nightly cron sustained 90d.
3. Custom domain routing strict enforcement.
4. Monitoring + alerts amplified.

## Diagnóstico (≤ 24h)

### Causa raiz típica

1. **Routing bug** (40-50%):
   - Worker region determination logic bug; `Accept-Language` mis-parse.
   - DO sticky placement crossed region boundary.
2. **Tenant region migration** (20-30%):
   - Customer changed region; in-flight requests still routed to old region.
   - Migration plan incomplete.
3. **Replication misconfiguration** (10-20%):
   - Hot blob replica detector copying to ALL regions instead of sibling only.
4. **Failover edge case** (5-10%):
   - Region outage; failover routed to wrong region; data written there.
5. **Cross-region API bug** (rare):
   - R2 API quirk; written to wrong region by Cloudflare.
6. **Adversarial scenario** (extremely rare):
   - Compromised infrastructure; insider attack.

### Investigação

- Worker logs region routing trace.
- D1 `audit_log` cross-reference.
- R2 object versioning history.
- Cloudflare network status.
- TLA+ `region_residency.tla` (planned) review.
- Property test gap analysis (which scenario was missed).

## Resolução

### Hot fix (≤ 6h)

- Migrate affected data.
- Routing bug fix em code (deploy via S-13 progressive rollout).
- Customer notification.
- Audit chain integrity preserved.

### Cold fix (1-4 weeks)

- **Insert checks reinforced** em D1 schema (region tag mandatory + check constraint).
- **Property test** S-11 baseline 20k (R-S11-19) → S-14 30k (R-S14-19) → 100k nightly cron sustained 90d cross-region scenarios.
- **TLA+ region_residency.tla** (planned Lote 9.4 obligation matrix): formal model region pinning.
- **Custom domain routing** strict: `<tenant_id>.<region>.corelink.humangr.com` enforced (no fallback).
- **Quarterly Schrems II TIA review** (S-14 R-S14-5 alignment).
- **Customer-facing residency dashboard** (S-16): customer can verify own region pinning.

## Post-incident

- Post-mortem CRITICAL within 7d (mandatory).
- 5-Why focused on root cause + INV-REGION-NO-CROSS-LEAK strengthening.
- Customer trust review + outreach plan.
- Privacy Officer + Legal + Compliance officer formal review.
- DPO interim notification + regulatory engagement if needed.
- Insurance / legal exposure assessment.

## Evidence

- Detection query results (pre/post fix).
- Affected blobs migration log per record.
- Audit log trace cross-region.
- Customer notification log.
- Regulatory notification log (se applicable).
- Network/Cloudflare status correlation.
- Code commit que introduziu bug (root cause traceability).

## Escalation

- **Single tenant, < 100 records, < 1h exposure**: SEV-2 internal; customer notification opt-in.
- **Multiple tenants OR > 100 records OR > 1h exposure**: SEV-1 + customer + regulatory.
- **Pattern recurrent**: architectural review + INV-REGION-NO-CROSS-LEAK formal verification expansion.

## References

- `invariant_registry.md` INV-DATA-RESIDENCY (§3.11) + INV-REGION-NO-CROSS-LEAK (§3.12).
- `specs/04_sprints/S14/_spec_contract.md` (multi-region + R-S14-19 30k property test).
- `specs/04_sprints/S11/_spec_contract.md` (residency pinning).
- `specs/03_architecture/privacy_model.md` (CTRL-PRIV-031).
- `specs/03_architecture/error_taxonomy.md` `COR_RESIDENCY_VIOLATION`.
- TLA+ planned `region_residency.tla` (Lote 9.4 obligation matrix).
- **Schrems II ruling (CJEU C-311/18)** — invalidação Privacy Shield.
- **EDPB Recommendations 01/2020** — supplementary measures international transfers.
- **GDPR Art. 33 (breach notification 72h) + Art. 44 (cross-border transfer)**.
- **LGPD Art. 33 § 1º** + **Resolução CD/ANPD nº 15/2024** (incident communication; correção Lote 10.11.0-ter).
- RB-FM-105 (region replication diverge — adjacent runbook).
