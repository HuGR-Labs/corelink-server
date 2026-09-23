---
name: own-corelink-failover-router
description: >-
  Route source-backed ownership changes for CoreLink failover routing decisions,
  health/probe contracts, failover audit boundaries, and failback outbox gates.
metadata:
  schema: "corelink-ownership/1.3"
  package: "corelink-failover-router"
  manifest: "crates/corelink-failover-router/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "failover-router-static-20260920"
---

# Ownership — corelink-failover-router

Fonte estática somente; candidata de autoria, sem certificação de execução de probe, Tower, Cloudflare, roteamento ao vivo, deploy ou produção. OKF canônico verificado é referência externa: não é duplicado, redefinido nem revalidado aqui.

[Acionamento](#s01) · [Fronteira](#s02) · [Leitura](#s03) · [Invariantes](#s04) · [Fluxo](#s05) · [Parada](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Condição | Decisão | Evidência estática | Pare quando |
|---|---|---|---|
| Mudam decisão de read failover, sinais de saúde ou modo de escrita | Assumir este package e abrir R04–R06 | `health.rs`, `probe.rs`, `router.rs` | A mudança requer execução de probe ou request path |
| Muda a recusa de failback | Traçar R05, B05 e `failback.rs` | Gate `assert_outbox_drained_or_block` | For necessário consultar um outbox real |

<a id="s02"></a>
## S02 — Fronteira de autoria

| Condição | Decisão | Evidência estática | Pare quando |
|---|---|---|---|
| Alterar avaliação, trait, decisão ou fixture em memória | Alterar o contrato desta crate | Módulos públicos em `src/lib.rs` | A interface de dependência não estiver congelada |
| Alterar emparelhamento/regra de residência | Coordenar com `corelink-replica-worker` | Dependência direta e `ResidencyGraph` reexportado | For preciso inventar topologia regional |
| Alterar montagem HTTP, status entregue ou middleware | Escalonar ao owner de composição | Não há montagem HTTP nesta crate | A única prova for comentário ou OKF |

<a id="s03"></a>
## S03 — Roteamento de leitura

| Dúvida | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Limites e estado de saúde | Ler R04–R06 | [Referência](../../../docs/ownership/crates/corelink-failover-router/REFERENCE.md#r04) | For necessária medição operacional |
| Impacto de dependência/auditoria | Ler B02–B05 | [Blast radius](../../../docs/ownership/crates/corelink-failover-router/BLAST_RADIUS.md#b02) | O grafo reverso completo for necessário |
| Procedimento e evidência permitida | Ler M01–M06 | [Manutenção](../../../docs/ownership/crates/corelink-failover-router/MAINTENANCE.md#m01) | A ação pedir runtime, rede ou deploy |

<a id="s04"></a>
## S04 — Invariantes de decisão

| Condição | Decisão obrigatória | Evidência estática | Pare quando |
|---|---|---|---|
| Mudar `RegionHealthSnapshot::evaluate` | Preservar `Degraded` somente com os três gatilhos ativos | `health.rs` | Um quarto sinal ou janela real for proposto sem contrato |
| Mudar rota degradada | Preservar audit emit antes de retornar decisão de réplica | `router.rs` | O sink/a ordem externa não estiver conhecida |
| Mudar modo de escrita | Preservar bloqueio para saúde que requer failover | `router.rs`, `health.rs` | For exigida resposta HTTP real |
| Mudar failback | Preservar recusa em outbox sujo ou erro de consulta; zero permite o gate | `failback.rs` | For necessária leitura de storage real |

<a id="s05"></a>
## S05 — Fluxo de trabalho

| Fase | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Antes de editar | Confirmar baseline, manifesto e limites estáticos | M02 | HEAD ou escopo divergir |
| Durante a edição | Mapear contrato, relação atômica e procedimento afetado | R04–R07, B01–B05 | Depender de owner externo |
| Depois da edição | Rodar apenas checks documentais autorizados e registrar lacunas | M06 | Resultado depender de execução |

<a id="s06"></a>
## S06 — Condições de parada

| Condição | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Pedido cita Tower, Cloudflare, tráfego, produção ou probe executado | Declarar não observado neste recorte | R08 e B06 | Não inferir runtime de fonte/OKF |
| Pedido exige topologia, réplica ou consumidor adicional | Coordenar com o owner identificado | B04 e B06 | Não houver fonte estática suficiente |
| Pedido altera o OKF canônico | Encaminhar à autoridade do OKF | Escopo declarado deste skill | Não copiar, redefinir ou revalidar OKF |

<a id="s07"></a>
## S07 — Evidência e saída

| Item | Critério |
|---|---|
| Critérios de sucesso | Os quatro artefatos existem, apontam o manifesto correto e distinguem contrato estático de execução desconhecida. |
| Critérios de completude | S01–S07, R01–R08, B01–B06 e M01–M06 são navegáveis; cada decisão crítica aponta fonte ou lacuna. |
| Padrões de qualidade | Escopo falsificável, relações atômicas, sem inferência de Tower/Cloudflare/live routing/probe execution e sem duplicar OKF. |
| Definição de pronto | Os quatro comandos externos de M06 passam; seu resultado é estrutural, não aprovação, runtime ou cold review; `git diff --check` não reporta whitespace. |
| Invariantes | Três sinais para degradação; erro de probe é conservador no router; failover bloqueia escrita; gate de failback só permite outbox limpo. |
