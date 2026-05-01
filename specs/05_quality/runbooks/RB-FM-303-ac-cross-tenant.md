---
id: "RB-FM-303"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "tenant-isolation", "data-integrity", "ac-eviction"]
---

# RB-FM-303 — AC Entry Aponta Para Blob de Outro Tenant

> **FM:** FM-303 (S=5, P1 S=5→upgrade) | **CTRLs:** INV-TENANT-ISOLATION + INV-AC-EVICT-TENANT-SCOPED + integration test | **SLA:** mitigate < 15 min (cross-tenant!)

## Detecção

- Property test em CI falha cross-tenant assertion.
- Métrica `corelink_isolation_assertion_total{outcome="violation"} > 0` (alert SEV-1 imediato).
- Customer report (raríssimo, mas possível em pre-prod).
- **(WI-S04-005 forward)** Audit chain `corelink.ac.evict.ttl_expired` shows `tenant_id` mismatch vs the row's expected tenant (cross-tenant DELETE attempted by cron worker — structurally unreachable per `INV-AC-EVICT-TENANT-SCOPED`, but defense-in-depth alert if observed).

## Comunicação

- **SEV-1 imediato.** Page Security Lead + Architect + SRE.
- Status page: degraded (sem detalhe sobre isolation).
- Comms preparados para customer notification se confirmado em prod.

## Mitigação imediata (≤ 15 min)

1. **Disable AC writes globalmente** via degrade_mode flag (`PAT-DEGRADE-001` cache-only).
2. **(WI-S04-005)** **Disable AC TTL cron worker** all 5 regions: unbind `worker-evict-ac-ttl-<region>` DO via wrangler CLI; preserves `ac_meta` rows from any further DELETE while forensics runs.
3. Snapshot dos AC entries afetados (D1 query); preserve evidence.
4. Identificar tenant_id afetado (vítima + contaminador).
5. Quarantine tenant contaminador (suspend writes).

## Mitigação completa (≤ 4h)

1. Patch hot-fix do bug específico (path validation, HMAC derivação).
2. Re-validar TODOS AC entries via batch job: `expected_tenant_hmac == observed`.
3. Quarantine entries com mismatch.
4. Re-enable AC writes apenas após patch verificado em staging.

## Forensics

1. Audit log: quem escreveu o AC entry inválido? Quando?
2. Se PAT comprometido: revoke + investigar uso.
3. Se bug de código: git blame + revisão de PR responsável.

## Notificação obrigatória

- **Tenant vítima**: dado pode ter sido exposto. Email formal + DPA reference.
- **ANPD/DPA** (se confirmado data exposure): conforme `RB-BREACH-NOTIF`.
- **Tenant contaminador**: provável bug, não maliciosidade; informe.

## Post-mortem

- TLA+ spec INV-TENANT-ISOLATION precisa cobrir o cenário que foi violado.
- Adicionar regression test ao property test suite.
- Considerar promoção para FF-HR-002 + revisar code review process.

## TTL eviction defense-in-depth (WI-S04-005)

The AC TTL cron worker is the primary delete path during normal operation. INV-AC-EVICT-TENANT-SCOPED makes a cross-tenant DELETE structurally unreachable through the canonical surface:

1. **`AcMetaStore::delete_tenant_scoped`** takes `(tenant_id, action_digest, region)` by value — there is no "DELETE WHERE expires_at < ?" bulk method on the trait surface.
2. **`AcMetaStore::select_expired_for_region`** filters on `(region, tenant_id, expires_at < now)` — every batch is single-tenant single-region; cross-tenant batches are impossible.
3. **`EvictBatch::run_one_batch`** re-asserts the tenant + region match per candidate (`debug_assert_eq!(candidate.tenant_id, tenant_id)` + region check returns `EvictError::RegionMismatch`).
4. **Per-region cron sharding**: each region has its own `InMemoryTtlWorker` / DO instance pinned to a single region; cross-region pollution is structurally impossible.

If FM-303 fires from the TTL eviction path, the assumption violated is that **the trait surface itself was bypassed** (e.g., a future adapter that emits raw SQL `DELETE FROM ac_meta WHERE expires_at < ?` without the `tenant_id` predicate). The fix is to revert the offending adapter + re-apply the trait-surface contract; never paper over with a flag.

## References

- WI-S04-005 — TTL infrastructure (refresh-on-hit + cron worker + tenant-scoped expiry).
- ADR-0019 — TTL ownership boundary S-04 ↔ S-07.
- `crates/corelink-worker/src/reapi/ac/ttl/evict.rs` — canonical eviction logic.
- `RB-FM-AC-TTL-DRIFT.md` / `RB-FM-AC-TTL-STORM.md` — sibling cron operational runbooks.
