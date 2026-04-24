---
id: "SPEC-CONTRACT-S08"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s08", "rate-limit", "quotas", "standard"]
---

# Spec Contract — S-08: Rate Limiting + Quotas (Multi-Camada)

## 0. Metadata

| Sprint ID | S-08 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2 semanas | WIs | 5 |

## 1. Objetivo

Implementar rate limiting multi-camada (global → per-tenant → per-IP → per-PAT) + quota enforcement + abuse detection heuristics. Previne noisy-neighbor (FM-001) e abuse (FM-255). Per-tenant DO singletons são core.

## 2. Lane + forcing factors

- **Lane:** STANDARD. Toca CTRL-RATE-001 mas não invariante CRITICAL.

## 3. Inherits_from

```yaml
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-RATE-001**: Per-tenant token bucket (refill rate conforme plan).
- **CAP-RATE-002**: Per-IP rate limit edge CF.
- **CAP-RATE-003**: Per-PAT rate limit (prevent hijacked credential abuse).
- **CAP-QUOTA-001**: Storage quota per-tenant (bytes).
- **CAP-ABUSE-001**: Abuse detection heuristic (PAT-ABUSE-DETECT-001).

## 5. Requirements específicos

- **R-S08-1**: DO `RateLimiter-<tenant_id>` com token bucket stateful.
- **R-S08-2**: CF edge rules para per-IP rate limit (não-atrelado a tenant).
- **R-S08-3**: Quota checker middleware antes de write path (SLO-impacting).
- **R-S08-4**: Abuse score calculator (CPU/wallclock ratio, egress outliers).
- **R-S08-5**: `corelink_abuse_score{tenant_id}` gauge + alert em threshold.

## 6. DoD

- [ ] 5 WIs SEALED.
- [ ] Load test: 1 tenant "abusivo" não degrada SLO de tenants vizinhos (INV-AVAIL-ISOLATION).
- [ ] Rate limit 429 `rate_limited_over_quota` ≠ `rate_limited_within_quota` (SLI distinction per `slo_catalog.md §4.2`).
- [ ] Abuse score heurística calibrada com workload real.

## 7. Completeness (delta)

- [ ] **10.s08.1** Chaos: tenant flood 10k QPS → outros tenants mantêm SLO.
- [ ] **10.s08.2** 429 response headers (Retry-After + X-RateLimit-*) conforme REAPI + HTTP padrão.

## 8. Invariants

- INV-AVAIL-ISOLATION (HIGH): tenant DoS não afeta outros (bulkhead enforcement).
- INV-QUOTA-ENFORCEMENT: quota real-time check.

## 9. Quality Standards

- Rate limit p99 overhead ≤ 3ms.
- Quota check é atomic (DO actor model).

## 10. Anti-scope

- ❌ Billing override (S-10).
- ❌ Multi-region rate limit sync (S-14).

## 11. Dependencies

- Blocker: S-03 (auth context pra per-tenant).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S08-001 | DO RateLimiter (token bucket) |
| WI-S08-002 | CF edge per-IP rate rules |
| WI-S08-003 | Quota middleware + enforcement |
| WI-S08-004 | Abuse detection heurística |
| WI-S08-005 | Dashboards + alerts |

## 13. Duração

2 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + chaos test clean.
- Quota enforcement E2E.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| DO storage quota exceeded (FM-059) | M | MEDIUM |
| Rate limit too aggressive → legit clientes 429 | M | MEDIUM |
| Abuse heurística falso positivo | H | LOW |

---
