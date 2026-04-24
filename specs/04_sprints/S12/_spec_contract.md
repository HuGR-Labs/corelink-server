---
id: "SPEC-CONTRACT-S12"
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
tags: ["spec-contract", "s12", "supply-chain", "slsa", "sbom", "high-risk"]
---

# Spec Contract — S-12: Supply Chain Hardening (SLSA L3 + SBOM + Cosign)

## 0. Metadata

| Sprint ID | S-12 | Lane | HIGH_RISK |
|---|---|---|---|
| Duração | 2.5 semanas | WIs | 6 |
| Forcing factors | FF-HR-005 (controles supply chain) |

## 1. Objetivo

Endurecer supply chain: SLSA Level 3 provenance, SBOM CycloneDX em cada release, cosign signature + verified deploy, cargo-audit daily, reproducible builds (goal-state). Mitiga THR-T-006 (código modificado no pipeline) e FM-156 (dep maintainer malicioso). Implementa CTRL-SUPPLY-001..005 + CTRL-CRYPTO-* + ADR-0014.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK. Compromisso de supply chain = todos os customers afetados.

## 3. Inherits_from

```yaml
inherits_from:
  - "SECURITY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
```

## 4. CAPs entregues

- **CAP-SUPPLY-001**: SLSA L3 build provenance (GitHub Actions + sigstore/Fulcio).
- **CAP-SUPPLY-002**: SBOM CycloneDX 1.5+ per release (ADR-0014).
- **CAP-SUPPLY-003**: Cosign signature + CF deploy verify.
- **CAP-SUPPLY-004**: Cargo-audit daily + Dependabot + `cargo-deny` policy.
- **CAP-SUPPLY-005**: Reproducible builds (2-runner diff check).

## 5. Requirements específicos

- **R-S12-1**: GitHub Actions workflow com SLSA L3 provenance (slsa-github-generator).
- **R-S12-2**: `cargo-cyclonedx` em build pipeline; SBOM published como release asset.
- **R-S12-3**: Cosign sign release artifacts; CF deploy verify signature pre-activation.
- **R-S12-4**: `cargo-audit` + `cargo-deny` em CI (PR + daily).
- **R-S12-5**: Dependency-Track integration (self-hosted ou CDN free-tier).
- **R-S12-6**: Reproducible build check (2 parallel runners; diff hashes).

## 6. DoD

- [ ] 6 WIs SEALED.
- [ ] SLSA L3 attestation publicada em sigstore transparency log.
- [ ] SBOM disponível como release asset + Dependency-Track ingestion.
- [ ] Cosign verify é gate antes de CF deploy (EVT-011).
- [ ] `cargo-audit` ≥ 0 findings HIGH/CRITICAL em `Cargo.lock`.
- [ ] Reproducible build: 2 runners produzem binário bit-identical (ou documentação das fontes de non-determinism pending).
- [ ] RB-FM-156 (dep malicioso) dry-run.

## 7. Completeness (delta)

- [ ] **10.s12.1** SBOM export passa NTIA minimum elements check.
- [ ] **10.s12.2** Cosign verify failure em deploy bloqueia rollout (chaos test).

## 8. Invariants

- INV-SUPPLY-SIGNED-DEPLOY (HIGH): binário não-assinado rejeitado.
- INV-SUPPLY-SBOM-PRESENT (HIGH): release sem SBOM blocked.

## 9. Quality Standards

- Zero dependências yanked no `Cargo.lock`.
- Zero licenses GPL/AGPL (cargo-deny enforces MIT/Apache/BSD/ISC/MPL-2.0).
- SBOM timestamp RFC 3161 attested.

## 10. Anti-scope

- ❌ Software Security Questionnaire completo (S-20 GA readiness).
- ❌ SOC 2 Type II audit engagement (pós-GA).

## 11. Dependencies

- Blocker: nenhum (sprint independente, mas recomendado após S-03 pra ter contexto real de deploy).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S12-001 | SLSA L3 GitHub Actions workflow |
| WI-S12-002 | SBOM CycloneDX generation + publish |
| WI-S12-003 | Cosign sign + CF deploy verify |
| WI-S12-004 | cargo-audit + cargo-deny policies |
| WI-S12-005 | Dependency-Track integration |
| WI-S12-006 | Reproducible build verification |

## 13. Duração

2.5 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + SLSA L3 verified + zero CVEs HIGH/CRITICAL.
- RB-FM-156 dry-run clean.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| SLSA L3 runner self-hosted forced (not GitHub) | L | MEDIUM |
| Reproducible build infeasible com deps Rust | H | LOW (goal, não blocker) |
| Cosign learning curve | M | LOW |
| Dep supply chain attack durante sprint (FM-156) | L | CRITICAL |

---
