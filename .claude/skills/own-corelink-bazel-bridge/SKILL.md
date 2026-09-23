---
name: own-corelink-bazel-bridge
description: >-
  Route source-backed changes to the REAPI v2 REST bridge for CoreLink CAS and
  AC handlers, including URI, digest, adapter, find-missing, and error contracts.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-bazel-bridge"
  manifest: "crates/corelink-bazel-bridge/Cargo.toml"
  source-commit: "59c76cf260bcdeb5246772f70821ac8b7e8a9780"
  evidence-set: "bazel-bridge-static-20260920"
---

# Ownership — corelink-bazel-bridge

Fonte estática somente; candidata de autoria, sem certificação de runtime, deploy ou grafo completo.

[Acionamento](#s01) · [Fronteira](#s02) · [Leitura](#s03) · [Invariantes](#s04) · [Fluxo](#s05) · [Parada](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| URI REAPI, Digest, adapter, find-missing ou erro muda | Assumir esta unidade e abrir a referência | Manifesto e `src/{adapter,digest,error,find_missing,lib,uri}.rs` | A mudança é de montagem HTTP ou autorização |
| CAS/AC handler trait muda | Coordenar contrato com seu owner | Dependências diretas no manifesto | A assinatura ainda não está congelada |

<a id="s02"></a>
## S02 — Fronteira de autoria

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| Traduzir REST REAPI v2 para handlers | Alterar bridge e preservar fronteira de trait | `adapter.rs`, `uri.rs` e `find_missing.rs` | For necessário criar transporte gRPC/tonic |
| Montar rota, extrair HTTP ou aplicar auth | Encaminhar a `corelink-container` | Consumidor direto no manifesto; rotas Bazel no container | Não houver owner de composição identificado |
| Alterar hash interno CoreLink | Coordenar com `corelink-hash` | Dependência direta e constante de limite | Confundir SHA-256 REAPI com contrato de hash CoreLink |

<a id="s03"></a>
## S03 — Roteamento de leitura

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| Dúvida sobre endpoint e método | Ler R03 e as rotas Axum do container | [Referência](../../../docs/ownership/crates/corelink-bazel-bridge/REFERENCE.md#r03) | A montagem/verb não é contrato do parser |
| Dúvida sobre digest ou limite | Ler R04 e R05 | [Contratos](../../../docs/ownership/crates/corelink-bazel-bridge/REFERENCE.md#r04) | A compatibilidade de cliente não foi demonstrada |
| Dúvida sobre consumidor | Ler B02 e B06 | [Blast radius](../../../docs/ownership/crates/corelink-bazel-bridge/BLAST_RADIUS.md#b02) | O grafo reverso completo for necessário |

<a id="s04"></a>
## S04 — Invariantes de decisão

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| CAS PUT é alterado | Preservar checagem de tamanho no adapter e SHA-256 no boundary do container antes da delegação | `adapter.rs`, `digest.rs` e rota do container | O novo caminho ignora uma verificação |
| Path ou método é alterado | Tratar `uri.rs` como parser; coordenar verb/path com rotas Axum do container | `uri.rs`, R03 | Método ou prefixo de montagem não é conhecido |
| Status de erro é alterado | Coordenar `http_status` sugerido com o mapper ativo do container | `error.rs`, R07 | O status HTTP ativo não foi revisado |
| Batch find-missing muda | Preservar cap, ordem e propagação de falha | `find_missing.rs`, R05 | Resultado do handler não tem cardinalidade esperada |

<a id="s05"></a>
## S05 — Fluxo de trabalho

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| Antes de editar | Confirmar checkout, manifesto e escopo da mudança | PROC estático em M02 | Baseline ou escopo divergir |
| Durante a edição | Mapear contrato, relação e manutenção afetados | R04, B01–B06 e M01–M06 | Uma dependência exigir decisão externa |
| Depois da edição | Registrar gates executados e lacunas | Saída S07 | Só houver alegação de runtime |

<a id="s06"></a>
## S06 — Condições de parada

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| Pedido inclui cliente Bazel, R2, D1, edge header ou request | Marcar como não observado e localizar owner | B06 | Não atribuir comportamento sem fonte |
| Mudança requer rota/container | Escalonar ao owner da composição | B02 | Não editar a montagem nesta unidade |
| Prova solicitada é operacional | Registrar limite da evidência | R08 | Não inferir runtime da fonte |

<a id="s07"></a>
## S07 — Evidência e saída

| Condição | Ação | Evidência | Pare quando |
|---|---|---|---|
| Entrega de mudança | Informar baseline, arquivos, contratos, relações e gates | Diff e comandos com status | Faltar resultado observável |
| Lacuna persistir | Declarar desconhecido e recovery necessário | R08, B06 e M06 | Não promover candidato a aprovado |
