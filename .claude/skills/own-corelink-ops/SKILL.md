---
name: own-corelink-ops
description: >-
  Assume ownership of corelink-ops for static local module, manifest, dry-run
  binary, feature, and test-contract changes; do not use for external operation.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-ops"
  manifest: "crates/corelink-ops/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "source-inspection-6be030999"
---

# Ownership — corelink-ops

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07)

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| A mudança toca `crates/corelink-ops`, seus alvos locais ou aliases públicos. | O comportamento pertence a uma crate reexportada ou exige operação externa. |

<a id="s02"></a>
## S02 — Território e autoridade

O território é o manifesto, módulos físicos, três bins dry-run e um bin verificador. Aliases `pub use` preservam o contrato do dono original; não transferem implementação. A rota canônica verificada é o [OKF SRE operations hub](../../../docs/knowledge/ops/sre-operations-hub.md). Esta skill autoriza somente análise e alteração local aprovada.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler |
|---|---|
| Superfície e invariantes | [Referência](../../../docs/ownership/crates/corelink-ops/REFERENCE.md#r01) |
| Relações e consumidores | [Impactos](../../../docs/ownership/crates/corelink-ops/BLAST_RADIUS.md#b03) |
| Modo e recuperação | [Manutenção](../../../docs/ownership/crates/corelink-ops/MAINTENANCE.md#m02) |
| Conceito transversal | [OKF](../../../docs/knowledge/ops/sre-operations-hub.md) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição | Ação e evidência | Parar quando |
|---|---|---|
| Símbolo move entre módulo físico e alias. | Compare `lib.rs`, módulo e API-002; guarde o diff. | Dono do contrato não identificado. |
| Bin, target ou feature muda. | Compare manifesto com R06 e M04. | Seleção suportada não puder ser expressa. |
| Predicado local muda. | Atualize INV-001 e selecione alvo declarado. | Contraexemplo esperado não definido. |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme package, manifesto e baseline `6be030999`.
2. Abra R/REL pertinente e o único conceito OKF acima.
3. Classifique: módulo físico, alias, bin, feature ou teste.
4. Defina predicado falsificável e alvo local.
5. Faça somente a mudança autorizada e guarde o diff.
6. Relate execução separadamente de inspeção e solicite revisão independente.

<a id="s06"></a>
## S06 — Condições de parada

Pare se alias precisar de mudança no fornecedor, feature implicar ambiente não local, target não for declarado, um dos três bins dry-run deixar essa classificação, o bin verificador mudar de papel, ou evidência for comentário sem símbolo. Encaminhe ao dono do módulo ou à rota de revisão do OKF.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo, baseline, API/INV/REL/PROC afetados, caminhos e símbolos lidos, target/feature, resultado executado ou `não executado`, risco residual e próximo owner. Não conclua comportamento externo a partir de fonte local.

[Voltar ao início](#s01)
