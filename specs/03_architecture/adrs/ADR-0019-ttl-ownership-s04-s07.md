---
id: "ADR-0019"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "ownership", "ac-ttl", "eviction", "tier"]
---

# ADR-0019 — AC TTL ownership: S-07 supersedes S-04 (per-tier TTL)

## Context

Round 2 audits (Opus C-03 + Codex CF-05) detectaram ownership clash entre S-04 e S-07:

- `S-04` entrega `CAP-AC-004` "AC TTL management" com **default 90d, refresh on hit** (R-S04-5).
- `S-07` entrega `CAP-EVICT-002` "TTL-based AC entry expiry **per-tier**" (free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable).

Customer free criado em S-04 staging → AC entries com prometida vida útil de 90d. Quando S-07 ship, esse mesmo customer free passa a ter TTL 7d (12.8× redução). **Sem migration plan; sem ADR; sem override declaration**.

Pior, sprint contracts publicados em `docs.corelink.dev/sla` (S-18) prometem tier semantics — divergência entre S-04 e S-07 cria contract violation público.

## Decision

**S-07 supersedes S-04 CAP-AC-004 TTL semantics.** A canonical truth pós-S-07 SEALED é a tabela per-tier:

| Tier | AC TTL default |
|---|---|
| Free | 7d |
| Solo | 30d |
| Team | 90d |
| Business | 365d |
| Enterprise | Customer-configurable (default 365d, max 730d) |

S-04 mantém entrega de **CAP-AC-004 infrastructure** (TTL worker, refresh on hit, expiry detection); S-07 sobrescreve apenas o **default value**.

## Migration plan

Durante S-07 implementation:
1. Pre-S-07: existing tenants têm TTL 90d default (S-04 setup).
2. S-07 deploy: rollout per-tier defaults via DO config-singleton (S-13 admin plane).
3. Free tenants com entries ativas > 7d: 14d notification email + grace period.
4. Pós-grace: TTL aplicado per tier; entries expiradas seguem normal eviction flow.
5. Audit trail: cada migration evento emite `corelink.ac.tier_migration` CloudEvent.

## Consequences

**Positive:**
- Single source of truth para AC TTL (S-07 + ADR + S-18 docs).
- Customer trust preserved via 14d notification.
- INV-AC-OUTPUTS-VALID mantido (entries expired via flow normal).
- Pricing tier semantics enforce-able.

**Negative:**
- Customer free users perdem 12.8× cache lifetime — **mitigado** com clear pricing page + upgrade path.
- Migration adiciona ~2 semanas de complexity (notification + grace).

## Alternatives considered

- **Manter S-04 default 90d, S-07 overrides apenas per opt-in tier**: rejected — customer free com 90d em produção é cost overhead (R2 storage); pricing tier semantics dictam shorter TTL para free.
- **S-04 elimina CAP-AC-004; S-07 entrega tudo**: rejected — S-04 já SEALED ou em iminent implementation; rework excessivo.
- **Per-customer override toda hora**: rejected — operational complexity sem benefício; tier-based é simpler + aligned com pricing model.

## Spec contract patches required

- `S-04 _spec_contract.md`: adicionar `local_deltas: ["CAP-AC-004 TTL default semantics overridden by S-07 CAP-EVICT-002 (ADR-0019)"]`.
- `S-07 _spec_contract.md`: já reflete; adicionar reference explícita a ADR-0019.
- `S-18 _spec_contract.md`: pricing page renderiza tabela canonical; CI gate verifica match com ADR.

## References

- `specs/04_sprints/S04/_spec_contract.md` (CAP-AC-004).
- `specs/04_sprints/S07/_spec_contract.md` (CAP-EVICT-002).
- Opus Round 2 C-03 (`specs/_audits/2026-04-24-opus-independent-sota-review-r2.md:63-71`).
- Codex Round 2 CF-05.
