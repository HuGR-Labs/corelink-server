---
id: "RB-FM-059"
type: "runbook"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "do", "quota", "rate-limit", "dry-run-executed"]
---

> **Dry-run executed 2026-05-02** — host-side harness `scripts/rb_fm_059_dry_run.sh` (WI-S07-005) green; chaos magnitude pinned to 1000 concurrent writes at 99.9% quota per Lote 10.6bis P0-W7-4. Detection signal cross-checked against `corelink_quota_denials_total{result=race_detected}` (must remain 0 across the prop suite) NOT a sustained-rate alert. Audit trace: `specs/_audits/sealed/2026-05-02-rb-fm-059-dry-run.md`.

# RB-FM-059 — Cloudflare Durable Object Quota Exceeded

> **FM:** FM-059 (S=4, O=3, D=3, RPN=36, P2) | **CTRL:** CTRL-RATE-001, CTRL-QUOTA-001 | **SLA:** recover ≤ 30 min

## Detecção

- Alert `corelink_do_quota_exceeded_total > 0` (DO storage size approaching 50GB hard limit).
- Worker logs `DurableObjectError: Storage quota exceeded`.
- Tenant affected impossibilitado de incrementar rate-limit counter ou quota counter.
- Métrica `corelink_rate_limit_state_size_bytes` ≥ 90% do budget per DO.

## Comunicação

- **SEV-2.** Page SRE on-call + Engineer responsável pelo subsystem afetado (rate-limit S-08 ou quota S-08/S-10).
- Internal incident channel `#incidents-corelink`.
- Não requer customer notification se isolated to single DO + customer impact é apenas latency spike.

## Mitigação imediata (≤ 5 min)

1. **Identificar DO afetado** via Worker logs `do_id` correlation.
2. **Re-route** novo tráfego do tenant para DO secondary (PAT-FAILOVER-001 se configurado) ou degrade-mode `rate-limit-bypass-warning` (PAT-DEGRADE-001).
3. **Compactar storage**: trigger DO `cleanup()` method que purga state expirado (TTL counters > window).
4. Se compaction insuficient: **reset DO state** com perda controlada de history (audit emissão + customer notification se quota perdida).

## Diagnóstico (≤ 30 min)

1. Query `do_storage_inspection` admin endpoint → top keys por size.
2. Identifica leak source:
   - Counters não-expirando (TTL bug)?
   - Sliding window com excessive entries?
   - Replay storage de chave revogada?
3. Cross-reference com tenant tier (free vs enterprise; enterprise tem budget maior).

## Resolução

- Hot fix: TTL aggressive cleanup + DO storage budget alert lowered para 70%.
- Cold fix: DO state model refactor (use array-based ring buffer em vez de KV-style unbounded keys).
- Prevenção: property test + chaos test gerando 100k requests/seg para DO única e medindo state growth.

## Post-incident

- Post-mortem dentro de 7d se SEV-2 confirmed.
- Update CTRL-RATE-001 + CTRL-QUOTA-001 alert thresholds.
- Review com SRE e Architect.

## Evidence

- Logs DO + traces.
- Storage size histogram.
- Tenant impact metrics (latency p99 delta).

## References

- `failure_modes.md` FM-059 entry.
- `resilience_patterns.md` PAT-DEGRADE-001, PAT-FAILOVER-001.
- Cloudflare docs: <https://developers.cloudflare.com/durable-objects/platform/limits/>.
