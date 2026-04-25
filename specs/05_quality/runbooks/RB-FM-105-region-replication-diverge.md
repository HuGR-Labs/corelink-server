---
id: "RB-FM-105"
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
tags: ["runbook", "p2", "region", "replication", "data-integrity"]
---

# RB-FM-105 — Region Replication Diverge (Cross-Region Hash Mismatch)

> **FM:** FM-105 (S=3, O=3, D=3, RPN=27, P2) | **INV:** INV-CAS-INTEGRITY HIGH + INV-REGION-NO-CROSS-LEAK CRITICAL | **SLA:** detect ≤ 1h, mitigate ≤ 6h, root-cause ≤ 24h

## Pré-condições

- S-14 multi-region infrastructure live (4 regiões: WNAM/ENAM/WEUR/SAM).
- PAT-REGION-FAILOVER-001 hot blob replica detector ativo.
- Reconcile cross-region cron job em execução.

## Detecção

### Sinais primários

- Reconcile cross-region cron alerta hash mismatch entre primary e replica.
- Métrica `corelink.region.replication_diverge_total` > 0.
- Failover read returns content diferente do primary post-recovery.
- Customer report: "I'm seeing different data depending on region routing".

### Sinais secundários

- Propagation lag > 60s p99 sustained 7d (S-14 R-S14-3 SLO breach).
- R2 cross-region replication API errors em logs.
- BLAKE3 hash chain audit detects diverged digests.

### Métricas relevantes

```
corelink.region.replication_lag_seconds{region_pair, p50/p95/p99}
corelink.region.replication_diverge_total{region_pair, blob_class}
corelink.cas.integrity_check_fail_total{region}
```

## Comunicação

- **Severidade**: SEV-2 (data integrity concern; SEV-1 if customer-visible AND > 5 min sustained).
- **Page**: SRE on-call + Architect (S-14 owner) + Privacy Officer (Schrems II potential).
- **Internal channel**: `#incidents-corelink`.
- **Customer notification**: required se serving stale data > 5 min OR cross-region tenant boundary violated (INV-REGION-NO-CROSS-LEAK).
- **Status page**: `degraded` se affecting > 1 region; `partial outage` if global.
- **Regulatory consideration**: GDPR Art. 33 notification se data integrity gap exposes EU tenant data; CRITICAL escalation.

## Mitigação imediata (≤ 6h)

### Step 1: Quarantine (≤ 15 min)
1. Identificar blob digest divergente via reconcile log.
2. **Quarantine ambas as cópias**: mark `quarantined=true` em D1; serving rejected with `COR_CAS_BLOB_QUARANTINED` (error_taxonomy add se ausente).
3. Pause replication worker para affected blob_class temporariamente.

### Step 2: Authoritative version determination (≤ 1h)
1. Determinar authoritative version via:
   - **Primary region** (per tenant.region_pinned em D1) é authoritative por default.
   - **Hash chain audit log** S-09 R-S09-10 cross-reference: which version was originally signed.
   - **Customer-confirmed** se support engagement available.
2. Document decision em incident log.

### Step 3: Re-replication (≤ 6h)
1. Delete divergent replica.
2. Re-replicate from authoritative source.
3. Hash verify post-replication (INV-CAS-INTEGRITY enforcement).
4. Audit emit `corelink.region.replication_diverge_resolved` com `before_hash, after_hash, authoritative_region, resolution_method`.
5. Clear quarantine flag.
6. Customer notification: divergence resolved.

## Diagnóstico (≤ 24h)

Causa raiz típica:

1. **Network partition durante replication** (5-10% dos casos):
   - Cloudflare WAN issue ou specific region connectivity.
   - Replication retry resumed mid-stream → corrupt write.
2. **Clock skew entre regiões** (2-5%):
   - Worker timestamp drift; replication policy uses ts comparison.
3. **R2 cross-region replication API quirk** (1-3%):
   - Eventual consistency window; delete-write race.
4. **Bug em replicator code** (rare; high impact):
   - Off-by-one em chunk offsets, encoding error.
5. **Adversarial scenario** (extremely rare):
   - Compromised replica region; tampering attempt.

Investigação:
- Cloudflare network status page.
- Worker logs cross-region routing trace.
- R2 audit logs (object versioning history).
- TLA+ scope review: replication não modelada formalmente; expand if pattern recurrent.

## Resolução

### Hot fix (24h)
- Re-replication completed.
- Quarantine cleared.
- Customer notification.
- Audit chain integrity verified.

### Cold fix (7d-30d)
- **TLA+ scope expansion**: model cross-region replication formally (planned); add `replication_correctness.tla` to obligation matrix.
- **Property test**: 100k iter cross-region with adversarial network partition.
- **Reconcile cadence**: tighten from daily → hourly se occurrences > 1/mês.
- **Replication retry policy**: refine PAT-RETRY-IDEMPOTENT-001 com cross-region awareness.
- **Customer trust review**: post-mortem público se SEV-1.

## Post-incident

- Post-mortem mandatório se SEV-2+ (within 7d).
- 5-Why analysis.
- Action items rastreados.
- Customer outreach se trust impact.
- INV-CAS-INTEGRITY review com Architect.
- Compliance officer notification se GDPR Art. 33 trigger met.

## Evidence

- Reconcile cron logs (pre/post fix).
- Replication lag métricas timeline.
- Hash chain audit cross-reference.
- Customer impact summary.
- Network status correlation.

## References

- `failure_modes.md` FM-105.
- `invariant_registry.md` INV-CAS-INTEGRITY (§3.2), INV-REGION-NO-CROSS-LEAK (§3.12).
- `specs/04_sprints/S14/_spec_contract.md` (multi-region).
- `specs/04_sprints/S02/_spec_contract.md` (CAS read path).
- TLA+ `cas_integrity.tla` + planned `replication_correctness.tla`.
- Cloudflare R2 cross-region docs.
