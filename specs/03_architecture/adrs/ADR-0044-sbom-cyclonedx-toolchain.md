---
id: "ADR-0044"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
related_adrs: ["ADR-0014", "ADR-0015", "ADR-0042"]
tags: ["adr", "sbom", "supply-chain", "cosign", "ci"]
---

# ADR-0044: SBOM CycloneDX 1.5+ toolchain — cargo-cyclonedx + sbomqs + cosign keyless

> **doc_status:** FROZEN
> **Versão:** 1.0.0
> **Última atualização:** 2026-04-29
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Supersedes:** —
> **Superseded By:** —

## 0. Contexto

ADR-0014 (FROZEN, 2026-04-24) decidiu **CycloneDX 1.5+ JSON** como formato preferido. Ficou pendente em §4 a implementação concreta do pipeline CI:

- [ ] Pipeline CI: gerar CycloneDX 1.5 com `cargo-cyclonedx` em todo build.
- [ ] Validar SBOM com NTIA minimum elements check.
- [ ] Assinar SBOM com cosign keyless (CTRL-SUPPLY-003).
- [ ] Publicar como release asset.

WI-S01-007 fecha esses 4 itens. Este ADR documenta a **toolchain stack canonical** (com versões pinadas), o **failure-mode policy** (Sigstore outage handling), e o **release-only signing guard** que limita exposição do OIDC `id-token`.

## 1. Decisão

A toolchain canonical para SBOM + assinatura no CoreLink é:

| Camada | Ferramenta | Versão pinada | Função |
|---|---|---|---|
| Geração | `cargo-cyclonedx` | `=0.5.9` (exact) | Walk workspace + emit per-crate `bom.json` em CycloneDX 1.5 JSON |
| Validação | `sbomqs` (interlynk-io) | `v1.0.5` (binary release) | Score NTIA minimum elements; gate ≥ 10.0/10.0 (todos elementos presentes) |
| Assinatura | `cosign` | `v2.4.1` (sigstore-installer-action SHA-pinned) | Keyless OIDC sign-blob via Fulcio short-lived cert + Rekor bundle |
| Identity | GitHub Actions OIDC | runtime token | Fulcio extrai certificate identity = workflow URL; ataque offline-forge é improvável |
| Mutation | `cargo-mutants` (nightly only) | `=27.0.0` (exact) | Workspace mutation testing fail-on-any-survivor |
| Fuzz | `cargo-fuzz` (nightly only) | `=0.13.1` (exact) | libFuzzer harness driver (5 crates × 9 targets) |

**Workflow integration:** `.github/workflows/cas_foundation.yml` jobs `sbom-cyclonedx` + `cosign-sign`.

### 1.1 NTIA gate threshold

`sbomqs score --category NTIA-minimum-elements --json` retorna `avg_score` em `[0.0, 10.0]`. Gate: `avg_score >= 10.0` (todos os 7 NTIA elementos presentes: supplier, component, version, dep_relationship, author, timestamp, unique_id). Score < 10.0 = job fail (HIGH_RISK lane: zero warning-only).

### 1.2 Cosign keyless OIDC mechanics

- Cada CI run recebe um **OIDC token** automaticamente (permissão `id-token: write` no job-level).
- `cosign sign-blob --yes` troca o token por um certificate Fulcio de **10 min de vida**, vinculado ao workflow URL como `subject`.
- Assinatura + cert vão para **Rekor public transparency log** (inclusion proof).
- `cosign verify-blob` re-valida via `--certificate-identity-regexp` (URL do workflow) + `--certificate-oidc-issuer` (token.actions.githubusercontent.com).

Resultado: tampering offline (atacante sem write-access ao Rekor) é detectável; chave longa nunca existiu para ser roubada.

### 1.3 Release-only signing guard (mandatório)

Cosign signing **só** roda em pushes para a release branch (`main`); pull requests — fork OR same-repo — não recebem o OIDC `id-token: write` e **não** chamam `cosign sign-blob`. O job condicional explícito:

```yaml
if: github.event_name == 'push' && github.ref == 'refs/heads/main'
```

Rationale: minimiza a superfície OIDC. Cada PR rodaria o gate de signing sem necessidade — o push para `main` é o único momento onde a release é "real". Forks + same-repo PRs continuam rodando todos os outros gates (TLC, build, test, deny, SBOM gen + NTIA validate); só o sign + Rekor passos são pulados.

Decisão pós round-2 codex review (ADR v1.0.0; antes a v0.x considerava também same-repo PR signing, mas criava exposure desnecessária e ainda assim não permitia fork-PR signing — duas alternativas com os mesmos benefícios líquidos, e a release-only é mais simples).

### 1.4 Sigstore outage failure-mode policy

Se Fulcio OU Rekor estiverem offline:

1. **Release blocked**: `cosign sign-blob` falha → job vermelho → release não passa.
2. **Sem grace period silencioso**: certificates Fulcio são curtos demais (10 min) para cachear; permitir "skip + retry" é supply-chain anti-pattern.
3. **Runbook de escalation**: `RB-FM-SIGSTORE-OUTAGE` (a criar em S-09 quando runbook corpus consolidar — referenced de WI-S01-007 §1#4).
4. **Ops-side comm**: status page ack + ETA monitoring; release queue até restauração.

Justificativa: HIGH_RISK lane (FF-HR-005) tolera **delay de release**, não tolera **release sem signature**.

## 2. Alternativas consideradas

| Opção | Pró | Contra |
|---|---|---|
| **cargo-cyclonedx (escolhida)** | Native Rust; CycloneDX maintained; multi-format | None significativo |
| `cyclonedx-cli` standalone | Multi-language | Não enxerga `Cargo.lock` corretamente para Rust |
| `syft` (anchore) | Multi-format universal | Mais pesado; menos deep no ecosistema Rust |
| `cargo-spdx` | SPDX 2.3 alternative | Imaturo (ADR-0014 §0); SBOM incompleta |
| **sbomqs (escolhida) para validation** | NTIA + BSI + open-source; CLI simples | None significativo |
| Manual NTIA python script | Total controle | Re-implementação de spec NTIA = drift risk |
| **cosign keyless (escolhida)** | Sem chaves longas; Fulcio + Rekor SOTA | Requer Sigstore live (mitigado §1.4) |
| Cosign chave longa | Funciona offline | Key rotation overhead; key leak = catastrophic |
| GPG-style detached signature | Familiar | Trust path manual; sem transparency log |

## 3. Consequências

### 3.1 Positivas

- **Supply-chain visível**: Rekor public log = qualquer terceiro pode verificar.
- **Zero key management**: nada para rotacionar, nada para vazar.
- **NTIA-compliant out-of-the-box**: SOC 2 auditor recebe SBOM + signature + inclusion proof.
- **Automated**: zero intervenção humana per release.

### 3.2 Negativas

- **Sigstore dependency**: outage do Sigstore bloqueia release (mitigado: runbook + comms).
- **Keyless = OIDC dependency**: GitHub Actions runtime obrigatório (alternativa: GitLab CI OIDC; defer pós-GA se migrarmos).
- **Fork PR limitação**: external contributors não conseguem emitir signatures; soluciona-se via `if:` guard mas adiciona um path no workflow.

## 4. Implementação

- [x] `.github/workflows/cas_foundation.yml::sbom-cyclonedx` job — gera SBOM via cargo-cyclonedx + sbomqs NTIA validation.
- [x] `.github/workflows/cas_foundation.yml::cosign-sign` job — keyless sign + Rekor bundle (offline-verifiable inclusion proof) + release-only guard (`push` to `main`).
- [x] `sbom/*.cdx.json` artifacts uploaded to GitHub Actions storage (90 day retention; 365 day para signed bundle).
- [x] `.gitignore` adicionado `sbom/` + `**/bom.json` (regen per release).
- [ ] Runbook RB-FM-SIGSTORE-OUTAGE — defer S-09 runbook consolidation (enumerated em corpus).

## 5. Evidence

- WI: `specs/04_sprints/S01/work_items/WI-S01-007-ci-tlc-gate-sbom.md` v1.2.0.
- Workflow: `.github/workflows/cas_foundation.yml` jobs `sbom-cyclonedx` + `cosign-sign`.
- Predecessor ADR: `ADR-0014` (format decision).
- Reproducible build sibling: `ADR-0015` (best-effort scope; full SLSA L3 deferred WI-S12-006).

## 6. Risk register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | sbomqs version drift | M | L | LOW | L | LOW | Pinned `v1.0.5`; bump via PR |
| R-002 | Sigstore Fulcio outage | L | L | MEDIUM | L | LOW | §1.4 outage policy; release blocked + runbook |
| R-003 | cargo-cyclonedx breaks on workspace dep change | L | M | LOW | M | LOW | `--locked` install + integration test smoke; PR catches |
| R-004 | OIDC token misuse in PR (same-repo OR fork) | L | L | LOW | L | LOW | Release-only `if:` guard — token nunca solicitado em PRs |

---

**Status final:** FROZEN. Mudanças requerem novo ADR (supersede).
