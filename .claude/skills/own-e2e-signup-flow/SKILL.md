---
name: own-e2e-signup-flow
description: >-
  Assuma ownership do harness `e2e-signup-flow` ao mudar fixtures, cenários,
  contratos compostos ou o stub R2; não use para operar signup, Stripe ou tenants.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-signup-flow"
  manifest: "tests/e2e-signup-flow/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "w015-signup-flow-source-1177dad2"
---

# Ownership — e2e-signup-flow

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar fixture ou cenário da jornada signup → DPA → tier | Alterar o worker ou endpoint de signup em produção |
| Rever idempotência, ordem de audit assertions ou o fake R2 | Definir política dos crates signup, DPA, tier, Stripe ou hash |
| Ajustar target ou propriedade do harness | Rodar a variante Stripe ignorada, usar credenciais ou chamar provider |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** `src/helpers.rs`, `src/r2.rs`, crate root e seus seis cenários declarados.

**Contratos públicos deste harness:** fixtures, conversões de contexto, agregação de audit e `InMemoryR2Client`.

**Contratos upstream:** `corelink-signup`, `corelink-dpa-acceptance`, `corelink-tier-selection`, `corelink-stripe-real` e `corelink-hash` mantêm suas próprias APIs e semânticas.

**Operação e aprovação:** nenhum operador, rota de escalonamento ou aprovador foi verificado para este harness; registre `UNKNOWN` em vez de inventar um nome.

Este package é `publish = false`; seus fakes não dão autoridade para atuar em signup, cobrança, Stripe, tenants ou armazenamento real.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| APIs e limites do harness | [Referência](../../../docs/ownership/crates/e2e-signup-flow/REFERENCE.md#r03) |
| Dependências e impactos | [Relações](../../../docs/ownership/crates/e2e-signup-flow/BLAST_RADIUS.md#b03) |
| Procedimento e teste seletivo | [Manutenção](../../../docs/ownership/crates/e2e-signup-flow/MAINTENANCE.md#m02) |
| Semântica upstream | Documentação do crate proprietário, começando pelo REL afetado |

Carregue somente o contrato e o cenário afetados. O fluxo de onboarding de produção é outra superfície; não o deduza desta composição in-memory.

<a id="s04"></a>
## S04 — Decisões e invariantes de autoria

| Condição | Ação e evidência exigida | Parar quando |
|---|---|---|
| Muda API ou semântica upstream | Identifique owner e REL; confira consumidor e assertion correspondente | A política do provider ou a compatibilidade não estiver decidida |
| Muda fixture, estado ou ordem | Preserve determinismo, isolamento e predicado adversarial; cite fonte e target | A afirmação depender de comportamento observado que não existe |
| Alcança a variante Stripe ignorada | Deixe-a fora desta tarefa; registre os detalhes de provider como desconhecidos | For solicitado inspecionar config/segredo ou executar a variante |

Cinco axiomas: nenhuma mudança funcional disfarçada; nenhuma autoridade concedida pela skill; nenhuma evidência de produção inferida de fonte ou fake; nenhuma relação omitida para caber no perfil; nenhuma autoaprovação.

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme o manifesto, `package.name`, baseline e arquivos modificados.
2. Abra os API/INV/REL/PROC que correspondem à mudança.
3. Separe fixture local, contrato upstream e efeito não observado.
4. Escolha somente um gate permitido pela matriz de manutenção.
5. Atualize as quatro superfícies documentais quando seus contratos mudarem; registre resultados reais.
6. Encaminhe os hashes finais para revisão independente; não edite o índice global em paralelo.

<a id="s06"></a>
## S06 — Condições de parada

Pare se a mudança pedir credencial, provider/config Stripe, rede, pagamento, dado de tenant, armazenamento real, deploy ou execução da variante ignorada. Pare também quando um contrato upstream divergir da fixture, a recuperação for desconhecida, ou alguém pedir para chamar produção a partir deste harness. Encaminhe a decisão ao owner comprovado do contrato; se a rota não estiver verificada, mantenha `UNKNOWN`.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo, baseline, API/INV/REL/PROC afetados, fontes, checks e resultados realmente executados, limites e próximo owner verificado. Declare separadamente `SOURCE`, `RESOLVED`, `EXECUTED_LOCAL`, `DEPLOYMENT` e `OBSERVED_RUNTIME`. Uma assertion local não prova rota, composição, deploy ou signup real.

[Voltar ao início](#s01)
