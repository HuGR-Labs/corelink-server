---
id: "SPEC-CONTRACT-S18"
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
tags: ["spec-contract", "s18", "docs", "docusaurus", "low-risk"]
---

# Spec Contract — S-18: Public Docs + API Reference + Pricing

## 0. Metadata

| Sprint ID | S-18 | Lane | LOW_RISK |
|---|---|---|---|
| Duração | 1.5 semanas | WIs | 4 |

## 1. Objetivo

Publicar docs em `docs.corelink.dev`: getting started, REAPI reference, SDK guides, compliance page (SOC 2 timeline), pricing page, security page (SLAs, SBOM access, pentest summary). Docs são o face público — impacto GTM direto.

## 2. Lane + forcing factors

- **Lane:** LOW_RISK. Docs bug = revisa; sem impacto técnico.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "COMPLIANCE-MATRIX"
```

## 4. CAPs entregues

- **CAP-DOCS-001**: Getting started (5-min quickstart Bazel/Buck2).
- **CAP-DOCS-002**: REAPI v2 reference + examples.
- **CAP-DOCS-003**: SDK guides (Python, Go, JS, CLI).
- **CAP-DOCS-004**: Compliance & security page (SBOM, pentest, SLAs).
- **CAP-DOCS-005**: Pricing page (5 tiers + feature matrix).

## 5. Requirements específicos

- **R-S18-1**: Docusaurus 3.x em `apps/docs/` deployed em CF Pages.
- **R-S18-2**: OpenAPI/gRPC reference generation de protos.
- **R-S18-3**: Pricing calculator (usage based + tier).
- **R-S18-4**: Security page com SBOM download + pentest summary + SOC 2 roadmap.

## 6. DoD

- [ ] 4 WIs SEALED.
- [ ] Docs URL live + SSL.
- [ ] 5 dev externos fazem getting started em < 5 min.
- [ ] Pricing calculator validado por Finance.

## 7. Completeness (delta)

- [ ] **10.s18.1** Zero broken links (CI check).
- [ ] **10.s18.2** i18n: en-US + pt-BR (basic).

## 8. Invariants

- Zero customer data real em screenshots/examples.

## 9. Quality Standards

- Lighthouse ≥ 95.
- Docs seguem sistema informação (Diátaxis: tutorial / how-to / reference / explanation).

## 10. Anti-scope

- ❌ Video tutorials (backlog pós-GA).
- ❌ Enterprise-only docs (S-19 customer onboarding).

## 11. Dependencies

- Blocker: S-15 (CLI + SDK existem).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S18-001 | Docusaurus setup + CF Pages deploy |
| WI-S18-002 | REAPI reference + SDK guides |
| WI-S18-003 | Compliance & security page |
| WI-S18-004 | Pricing calculator + page |

## 13. Duração

1.5 semanas; buffer 2 dias.

## 14. Critérios de promoção

- DoD + external dev feedback.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Docs stale vs realidade do código | H | LOW (auto-gen mitiga parcial) |
| Pricing mudança post-launch confunde customers | M | LOW |

---
