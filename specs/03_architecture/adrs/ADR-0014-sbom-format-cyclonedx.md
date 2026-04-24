---
id: "ADR-0014"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
related_adrs: ["ADR-0012", "ADR-0013"]
tags: ["adr", "supply-chain", "sbom", "compliance"]
---

# ADR-0014: SBOM em CycloneDX 1.5+ (preferido) com SPDX 2.3+ aceito

> **doc_status:** FROZEN
> **Versão:** 1.0.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Supersedes:** —
> **Superseded By:** —

## 0. Contexto

Audit Lote 3+4 (Sonnet S-04) identificou contradição:

- `00_framework.md §35.7 EVT-010` (versão 0.4.2): `"SPDX 2.3+ JSON/YAML"`.
- `security_model.md §8.2`: `"CycloneDX 1.5 (JSON). Gerado via cargo-cyclonedx."`.
- `security_model.md §6.4 CTRL-SUPPLY-003`: `"CycloneDX gerado em build"`.

Implementação real do ecossistema Rust:
- `cargo-cyclonedx` é maduro (latest 0.5.x, mantido pela CycloneDX Foundation).
- `cargo-spdx` existe mas é menos maduro / tem limitações.
- Ferramentas downstream (Dependency-Track, Snyk, JFrog Xray) **aceitam ambos**.
- Compliance (SOC 2, NTIA SBOM minimum elements) aceita qualquer formato comum.

## 1. Decisão

Adotar **CycloneDX 1.5+ JSON como formato preferido** para SBOM no CoreLink, mantendo **SPDX 2.3+ como aceito** para casos onde stakeholders externos exigirem.

Atualizar `00_framework.md §35.7 EVT-010`:

> **EVT-010 SBOM**: CycloneDX 1.5+ JSON (preferido no ecossistema Rust via cargo-cyclonedx) ou SPDX 2.3+ JSON/YAML; ambos aceitos.

## 2. Alternativas consideradas

| Opção | Pró | Contra |
|---|---|---|
| **Apenas SPDX 2.3+** | Padrão ISO; favorito governo US | cargo-spdx imaturo; gera SBOM incompleta em Rust |
| **Apenas CycloneDX 1.5+** | Tooling maduro; rich em vuln data | Alguns enterprise customers podem pedir SPDX especificamente |
| **Aceitar ambos com CycloneDX preferido (escolhida)** | Tooling Rust funciona; flexibilidade comercial | Pipeline precisa suportar 2 formatos (mas é trivial) |
| **Custom format** | Total controle | Anti-pattern; quebra interop |

## 3. Consequências

- **Positivas:**
  - `cargo-cyclonedx` em CI funciona out-of-the-box.
  - Vendor compliance (SOC 2 auditor) recebe SBOM padrão da indústria.
  - Dependency-Track ingestion funciona.
- **Negativas:**
  - Se cliente enterprise específico exigir SPDX, precisamos converter (`cyclonedx-cli convert`).
  - 2 formatos = 2 caminhos de teste em CI (overhead pequeno).

## 4. Implementação

- [x] Atualizar `00_framework.md §35.7 EVT-010` com formato dual aceito.
- [x] Bump framework v0.5.0.
- [x] Confirmar `security_model.md §8.2` e `§6.4 CTRL-SUPPLY-003` consistentes (já estavam em CycloneDX).
- [ ] Pipeline CI: gerar CycloneDX 1.5 com `cargo-cyclonedx` em todo build de release.
- [ ] Pipeline CI: opcional gerar SPDX (se vendor exigir).
- [ ] Assinar SBOM com cosign (CTRL-SUPPLY-003).
- [ ] Publicar como release asset + enviar pra Dependency-Track.

## 5. Evidence

- Audit finding: `specs/_audits/2026-04-24-sonnet-audit-lote3-4.md` (S-04).
- Framework atualizado: `00_framework.md §35.7 EVT-010` (v0.5.0).

---

**Status final:** FROZEN. Mudanças requerem novo ADR (supersede).
