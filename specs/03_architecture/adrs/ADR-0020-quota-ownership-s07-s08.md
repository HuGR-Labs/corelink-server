---
id: "ADR-0020"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-25"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "ownership", "quota", "rate-limit", "eviction"]
---

# ADR-0020 — Quota ownership: S-07 = storage via eviction; S-08 = bandwidth + behavioral

## Context

Round 2 audits (Codex CF-06) detectaram ownership clash entre S-07 e S-08:

- `S-07` entrega `CAP-EVICT-003` "Quota enforcement (storage)" via eviction.
- `S-08` entrega `CAP-QUOTA-001` "Storage quota per-tenant enforcement" + `CAP-QUOTA-002` "Bytes ingress/egress quota".

Sem decomposição clara, ambos sprints reivindicam "quota". Quem detecta breach? Quem aplica enforcement? Quem emite alerts?

## Decision

**Ownership decomposto por categoria de quota:**

### S-07 = Storage quota (eviction-driven)
- **Trigger:** soft-pressure quando `tenant.bytes_used > 0.8 × tenant.storage_limit`.
- **Enforcement:** eviction worker (LRU dentro do tier) reduz `bytes_used` antes de hard-block.
- **CAP affected:** `CAP-EVICT-003` (renomeado: "Storage soft-pressure eviction").
- **Métrica:** `corelink.storage.bytes_used{tenant_tier}`, `corelink.storage.eviction_runs_total`.
- **Customer-facing:** silent eviction até 100% (cold-storage moved out of hot path); 100% block em S-08 layer.

### S-08 = Bandwidth + behavioral quota (rate-limit-driven)
- **CAP-RATE-001..004**: per-tenant + per-IP + per-PAT + global circuit breaker rate limit.
- **CAP-QUOTA-001 (renomeado: "Storage hard-block")**: 100% storage breach → 429 + `Retry-After: days-until-month-reset`.
- **CAP-QUOTA-002**: bytes ingress/egress monthly rolling counter + 429 over-quota.
- **CAP-ABUSE-001/002**: abuse score + automated response.
- **Métrica:** `corelink.rate_limit.rejects_total{layer}`, `corelink.bandwidth.bytes_total{direction, tenant_tier}`.

## Boundary protocol

- **Soft state** (80-95% storage): S-07 owns (eviction triggered).
- **Hard state** (100% storage OR bandwidth quota OR rate limit OR abuse): S-08 owns (429 response + headers).

## Consequences

**Positive:**
- Single owner per quota category — decisões de redirect/block claros.
- Eviction (S-07) reduz pressure antes de hit hard-block (S-08).
- Métricas separadas: storage vs bandwidth vs behavioral.
- Customer-facing: gradient (silent eviction → 429 hard-block), não cliff.

**Negative:**
- 2 sprints ainda devem coordenar configuração via DO config-singleton (S-13).
- Cross-reference em código: S-08 quota check lê `tenant.bytes_used` que S-07 mantém — coupling, mas explicit.

## Spec contract patches required

- `S-07 _spec_contract.md`: `CAP-EVICT-003` rename "Storage soft-pressure eviction"; clarify ≤ 95% trigger; ≥ 100% defer to S-08.
- `S-08 _spec_contract.md`: `CAP-QUOTA-001` rename "Storage hard-block (100%)"; explicit reference S-07 boundary.

## Alternatives considered

- **S-08 owns todas as quotas** (S-07 só dedup/eviction sem quota): rejected — eviction sem quota awareness é "blind eviction"; S-07 precisa do contexto pra decidir prioridade.
- **S-07 owns todas storage quotas** (S-08 só bandwidth): rejected — hard-block é rate-limit semantics (429 + Retry-After); pertence à mesma surface dos outros 429.
- **Per-tenant config switch**: rejected — operational complexity sem benefit.

## References

- `specs/04_sprints/S07/_spec_contract.md` (CAP-EVICT-003).
- `specs/04_sprints/S08/_spec_contract.md` (CAP-QUOTA-001/002).
- Codex Round 2 CF-06.

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (Lote 9.1) | Criação ADR-0020 (Quota ownership boundary S-07 ≤95% triggers eviction; S-08 100% hard-block rate-limit). |
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7bis P0-5 + P0-4 fixes) | **DRAFT → FROZEN promotion** (Agent R4 + Sonnet R5 caught: WI-S07-003/005 cited "ADR-0020 FROZEN" mas era DRAFT). Content audit by Owner: Decision §32 boundary clarification — S-07 owns ≤95% (telemetry SEV-3 80% + ad-hoc eviction trigger 95%); S-08 owns 100% hard-block (rate-limit/429 + Retry-After). **80% soft-warn email path defer to S-13** (admin/notifications) per Lote 10.7bis P0-4 fix; ADR-0020 §Decision clarified "soft-pressure 80% = telemetry SEV-3 oncall only; 95% = ad-hoc eviction trigger; 100% = S-08 hard-block 429" — eliminates email infrastructure assumption sem S-13. Architect + Crypto SME independent re-review optional (advisory). |
