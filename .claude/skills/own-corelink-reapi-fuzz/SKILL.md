---
name: own-corelink-reapi-fuzz
description: >-
  Assuma ownership de corelink-reapi-fuzz ao alterar os três harnesses de fuzz,
  suas dependências ou sua relação com os contratos do corelink-reapi. Use para
  revisar ou diagnosticar esses targets; não use como owner do serviço REAPI,
  de dados de produção ou da infraestrutura de CI.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-reapi-fuzz"
  manifest: "crates/corelink-reapi/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "reapi-fuzz-w016-1177dad2"
---

# Ownership — corelink-reapi-fuzz

Candidata documental. Os targets foram inspecionados estaticamente; nenhuma compilação ou execução de fuzz foi realizada nesta autoria. A revisão independente continua pendente.

[Acionamento](#s01) · [Território](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Evidência](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use como owner principal quando |
|---|---|
| Mudar `proto_decode_batch_update`, `audit_request_id_total` ou `parse_read_resource_name`. | Mudar os handlers REAPI, o transporte gRPC/HTTP ou o contrato de produção; encaminhe ao owner de `corelink-reapi`. |
| Alterar dependências, lockfile, entrada Cargo ou seleção de CI deste package. | Tratar a intenção do harness, `test = false` ou `doc = false` como evidência de execução. |
| Avaliar um crash encontrado por um desses targets e seu alcance no código chamado. | Inferir cobertura de serviço, reachability de produção ou segurança geral por existência de um target. |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** três arquivos em `fuzz_targets/` e `fuzz/Cargo.toml`.

**Contratos exercitados:** decoders REAPI, `audit_request_id_for_blob` e `handler::parse_read_resource_name`; ver [API-001](../../../docs/ownership/crates/corelink-reapi-fuzz/REFERENCE.md#api-001).

**Fora do território:** handlers e protos pertencem ao pai; este package não concede operação do workflow ou runner.

**Papéis distintos:** owners humanos de implementação, contrato REAPI e operação CI, além do aprovador, não foram identificados. Não invente nomes ou escalonamento.

Esta skill não autoriza alteração funcional, publicação, operação de produção, alteração de workflow, gasto ou retenção de corpus fora da tarefa aprovada.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Abrir |
|---|---|
| O que cada harness chama e quais entradas ignora? | [Implementação e contratos](../../../docs/ownership/crates/corelink-reapi-fuzz/REFERENCE.md#r03) |
| Qual é a diferença entre dependência, fluxo e impacto? | [Relações](../../../docs/ownership/crates/corelink-reapi-fuzz/BLAST_RADIUS.md#b03) |
| O que está nomeado no workflow e o que ficou sem seleção? | [Cobertura](../../../docs/ownership/crates/corelink-reapi-fuzz/BLAST_RADIUS.md#b06) |
| Como fazer uma execução local isolada e preservar falhas? | [Procedimentos](../../../docs/ownership/crates/corelink-reapi-fuzz/MAINTENANCE.md#m03) e [matriz](../../../docs/ownership/crates/corelink-reapi-fuzz/MAINTENANCE.md#m04) |
| Qual é o contrato REAPI de origem? | [WI-S02-001](../../../specs/04_sprints/S02/work_items/WI-S02-001-bytestream-read.md) e as fontes imutáveis na referência. |

Leia somente as seções pertinentes. Esta skill não substitui revisão do contrato pai nem uma cold review independente.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição | Ação e evidência exigida | Parar quando |
|---|---|---|
| Mudar assinatura, mensagem protobuf ou feature do pai | Atualize a superfície afetada, enumere consumer/target e selecione validação no [M04](../../../docs/ownership/crates/corelink-reapi-fuzz/MAINTENANCE.md#m04). | A seleção real, compatibilidade ou consumer inverso for desconhecido. |
| Um target importa um crate sem declaração direta correspondente | Registre a incompatibilidade SOURCE em R03/R06, REL-008 e M04; mantenha build/runtime UNKNOWN e não execute Cargo para contornar. | A dependência, owner ou mudança de manifesto não estiver reconciliada. |
| Mudar `audit_request_id_for_blob` ou seu harness | Preserve a separação do tenant e a derivação textual documentadas no pai; rode a validação isolada e o teste de idempotência pertinente. | Um resultado de fuzz for tratado como prova de atomicidade de auditoria ou de execução em produção. |
| Mudar o parser ou o harness de parser | Registre que o target atual só chama o parser para bytes UTF-8 válidos e descarte qualquer alegação de cobertura para entradas inválidas. | O domínio de entrada do novo harness não estiver explícito. |
| Alguém pedir status de CI ou alcance em runtime | Recolha evidência daquele run e target, sem inferir do workflow estático ou dos flags Cargo. | Não houver saída/run identificável; marque desconhecido. |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme manifesto, package, baseline `1177dad2ca2a9f21c29b5a118aa7944b77147798` e escopo autorizado. 2. Abra a referência para o harness exato e o blast radius para sua fronteira e seleção de CI. 3. Escolha um target declarado no manifesto; não inclua automaticamente targets do package pai. 4. Antes de fuzz local, use checkout e diretórios Cargo isolados, toolchain/dependências já disponíveis e limite explícito. 5. Preserve corpus e artefatos de falha; registre target, comando, ambiente,

tempo, exit status e hash da fonte. 6. Atualize os documentos afetados e encaminhe os hashes finais à revisão independente.

<a id="s06"></a>
## S06 — Condições de parada

Pare se a fonte não corresponde ao pin ou sua substituição ainda não foi reconciliada; se o alvo não está declarado; se toolchain, dependências ou limite de recursos não estão confirmados; se o comando exige instalação/rede não autorizada; ou se um crash indica violação no pai sem owner e plano de recuperação conhecidos. Não apague corpus ou artefatos de outra tarefa para obter uma árvore limpa. Sem rota de owner verificada, registre a lacuna e não invente escalonamento.

<a id="s07"></a>
## S07 — Evidência, critérios e saída

Entregue baseline, objetivo, APIs/INVs/RELs/PROCs afetados, seleções exatas, comandos e resultados reais, estado observado, limitações e risco residual. “Declarado”, “compilado”, “executado localmente” e “observado em runtime” são estados diferentes.

Preserve os cinco critérios do contrato: **success** — outra pessoa consegue assumir sem relato do autor; **completeness** — populações e desconhecidos são conciliados; **quality** — limites, fontes e navegação são cumpridos.

**Definition of done:** quatro docs revisados nos hashes finais e gates aplicáveis satisfeitos. **Invariants:** nenhuma mudança funcional disfarçada, autoridade implícita, evidência fictícia, omissão para caber ou autoaprovação. Esta candidata ainda não satisfaz a revisão independente nem certifica fuzz execution.

[Voltar ao início](#s01)
