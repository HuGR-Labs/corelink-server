---
name: own-e2e-billing-flow
description: >-
  Assuma ownership do harness e2e-billing-flow para contratos isolados de
  signup, assinatura, webhook, reembolso e DSR; não opere Stripe ou clientes.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-billing-flow"
  manifest: "tests/e2e-billing-flow/Cargo.toml"
  source-commit: "cca798ff5bc2df660ecf2570ed243eb9775ff3d0"
  evidence-set: "e2e-billing-flow-pilot-source-20260920"
---

# Ownership — e2e-billing-flow

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar o harness ou cenário de jornada de cobrança in-memory | Operar Stripe, checkout, webhook, reembolso, DSR ou tenant real |
| Diagnosticar transição, deduplicação, HMAC ou gate DSR | Definir política de um ledger upstream sem seu owner |
| Mudar fixture, relógio fixo ou contrato de teste | Usar teste isolado como prova de integração em produção |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** `BillingHarness`, dados determinísticos, máquina de estado local, log de gates e cenários. **Contrato público:** tipos reexportados, sequência e fronteiras do harness. **Composição:** `BillingHarness::setup` compõe quatro ledgers in-memory; o runner local/CI apenas seleciona o target.

**Fora do território:** lógica interna de signup/tier/Stripe/DSR, endpoint HTTPS Stripe, segredos, dados e autorização reais. A rota observável é `.github/CODEOWNERS` (`@gmhelmold`), mas é apenas pedido de revisão e não prova de aprovador independente; sem owner/rota verificados, pare em `BLOCKED`. Esta skill não autoriza rede, cobrança, credencial nem escrita externa.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Estado, APIs e escopo do harness | [Referência](../../../docs/ownership/crates/e2e-billing-flow/REFERENCE.md#r03) |
| Relações e o que não é provado | [Impactos](../../../docs/ownership/crates/e2e-billing-flow/BLAST_RADIUS.md#b03) |
| Diagnóstico e teste seguro | [Manutenção](../../../docs/ownership/crates/e2e-billing-flow/MAINTENANCE.md#m02) |
| Política de cobrança/privacidade | OKF/ADR do owner, sem duplicá-los |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Alterar transição de assinatura | Preservar pré-condição e cenário de erro | Contrato upstream divergir |
| Validar HMAC adulterado | Distinguir cenário 5, que chama `deliver_signed_webhook` diretamente, do caminho separado por `process_refund` | A cobertura direta for tratada como prova do método de refund |
| Alterar ou investigar `process_refund` | Confirmar se a assinatura/log é validada antes da mutação; exigir um caso que chame `process_refund` com tamper e verifique erro + lifecycle preservado. A ordem atual muta antes do handler e não tem rollback | Rejeição deixar estado `Refunded`, exigir chave/endpoint real, ou não houver caminho para testar a falha |
| Alterar gate DSR | Garantir bloqueio antes do endpoint | Política de retenção/DSR mudar |
| Alterar fixture/tempo | Preservar determinismo e registrar impacto | Teste depender de relógio/rede externa |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme manifesto, baseline, cenário e owners dos contratos compostos.
2. Trace entrada, estado, audit log e chamada a cada ledger.
3. Para tamper, mantenha um cenário do handler direto separado do teste de `process_refund`; o primeiro já existe e não cobre o segundo.
4. Use apenas fakes/in-memory e tempo determinístico.
5. Atualize os três documentos e peça cold review para bytes alterados.

<a id="s06"></a>
## S06 — Condições de parada

Pare para segredo, `STRIPE_*`, endpoint, webhook real, cliente, reembolso, cobrança, DSR real ou dado pessoal. Pare se uma correção exigir alterar semântica de signup, tier-selection, billing-stripe ou DSR sem coordenação. Um teste verde não autoriza operação nem prova reachability de produção.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline, cenário, estado inicial/final, contrato upstream, comando e resultado reais. Para cada afirmação, aponte o registro `SRC/REL/PROC` com caminho, linhas/blob, classe e resultado. Declare `implemented`, `wired` e `runtime_verified` separadamente, além de `SOURCE`, `EXECUTED_LOCAL`, `DEPLOYMENT` e `OBSERVED_RUNTIME`. O caminho adulterado de `process_refund` permanece sem teste até haver cenário próprio; leitura estática não o promove a execução. Aprovação requer cold review independente de cada artefato e não certifica uma operação Stripe.

[Voltar ao início](#s01)
