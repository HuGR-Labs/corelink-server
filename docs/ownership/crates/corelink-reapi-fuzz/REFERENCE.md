---
schema: corelink-ownership/1.1
document: reference
package: corelink-reapi-fuzz
manifest: crates/corelink-reapi/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: reapi-fuzz-w016-1177dad2
---

# corelink-reapi-fuzz — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Targets](#r06) · [Falhas](#r07) · [Evidência](#r08).

<a id="r01"></a>
## R01 — Identidade e função

`corelink-reapi-fuzz` é um package Cargo isolado em `crates/corelink-reapi/fuzz`, com três bins de fuzz para funções e tipos de `corelink-reapi`. A existência dos bins e seus usos no código provam intenção e wiring declarado. Não provam compilação, execução, cobertura do serviço ou reachability em produção. Esta edição usa somente inspeção estática no pin indicado.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-reapi-fuzz` / `crates/corelink-reapi/fuzz/Cargo.toml` |
| Library / binaries / outros targets | Sem `[lib]`; três `[[bin]]`: `proto_decode_batch_update`, `audit_request_id_total`, `parse_read_resource_name`. |
| Papel | Harness de fuzz independente; manifesto declara `[workspace]` próprio e o workspace raiz exclui o caminho. |
| Licença / publish | `UNLICENSED` / `publish = false` no manifesto do package. |
| Implementado / wiring / runtime | Implementação: sim, arquivos-fonte; declaração: sim, três entradas bin; invocação estática de CI: dois nomes no workflow nightly; execução observada neste trabalho: não; runtime do serviço: desconhecido. |

Fonte: [manifesto imutável](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L1-L45) e [exclusão do workspace](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/Cargo.toml#L384-L394).

<a id="r02"></a>
## R02 — Fronteiras e papéis

| Papel | Escopo verificado | Owner humano / rota |
|---|---|---|
| Dono da implementação | Os três arquivos de harness e o manifesto do package. | Não identificado na fonte inspecionada. |
| Dono do contrato | Tipos protobuf e helpers exportados por `corelink-reapi`; mudanças do contrato pai não pertencem a este package. | Equipe/pessoa não verificada. |
| Composição e operação | Workflow nightly declara dois targets; corpus, compilação, saída e execução são responsabilidade da invocação. | Owner do workflow/runner não verificado. |
| Aprovador | Revisão independente dos quatro docs e das mudanças futuras. | Não atribuído nesta autoria. |

**Não faz:** implementar handler REAPI; enviar tráfego; validar autorização; provar ausência de panic/OOM; ou operar R2, D1, serviço, workflow ou runner.
**Estado próprio:** nenhum estado de aplicação é declarado. Uma execução pode produzir corpus, artefatos de crash e saída em diretórios locais; política de retenção/commit não está especificada nas fontes estudadas.
**Conceito de origem:** veja os WIs referenciados no R08; as notas de intenção no harness não substituem o contrato do pai.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entrada | Papel e entrada | Natureza e resultado usado | Fonte no pin |
|---|---|---|---|
| `fuzz_targets/proto_decode_batch_update.rs` / bin homônimo | Recebe `&[u8]`; chama `BatchUpdateBlobsRequest::decode` e `GetCapabilitiesRequest::decode` com a mesma entrada. | Owned harness; os dois `Result` são descartados. | [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs#L1-L19). |
| `fuzz_targets/audit_request_id_total.rs` / bin homônimo | Retorna sem chamada se `data.len() < 16`; os 16 primeiros bytes viram UUID; o restante é dividido ao meio e convertido com `String::from_utf8_lossy`; chama `audit_request_id_for_blob`. | Owned harness; ignora a `String` retornada. Não testa evento/outbox. | [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs#L1-L25). |
| `fuzz_targets/parse_read_resource_name.rs` / bin homônimo | Recebe bytes; só chama o parser se `std::str::from_utf8` aceitar a entrada. | Owned harness; ignora `Result`; bytes não UTF-8 não chegam ao parser. | [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs#L1-L19). |
| Dependência `corelink-reapi = { path = ".." }` | Provider das funções e mensagens chamadas. O pai declara `host-server` como feature default; `proto` e `handler` têm `cfg(feature = "host-server")`. | Path dependency declarado; resolução efetiva não foi obtida por Cargo nesta autoria. | [manifesto fuzz](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L24-L45), [features do pai](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/Cargo.toml#L17-L42), [gates de módulos](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/lib.rs#L73-L89). |
| Import direto observado sem declaração correspondente | `audit_request_id_total.rs` usa `uuid::Uuid`; o manifesto fuzz não declara `uuid` em `[dependencies]`. | Incompatibilidade SOURCE registrada; compilação/resolução do target permanece UNKNOWN e não foi testada. | [import](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs#L7-L19) · [manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L24-L27). |

**Inventário:** 3 declarações bin conferidas contra 3 arquivos `fuzz_targets`; nenhum corpus está rastreado no subdiretório fuzz no pin. Isso não enumera arquivos locais ignorados, corpus retido pelo runner ou caminhos dinâmicos.

<a id="r04"></a>
## R04 — Contratos do harness

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

<a id="api-001"></a>
### API-001 — Decoders protobuf do target

**Símbolos exatos:** `BatchUpdateBlobsRequest::decode(&[u8])` e `GetCapabilitiesRequest::decode(&[u8])` via `prost::Message`.

**Pré-condição:** o módulo `proto` do pai precisa estar exposto pela feature `host-server`; o target fornece bytes arbitrários.

**Resultado/efeito:** ambos os `Result` são descartados; não há chamada gRPC, transporte ou persistência no harness.

**Erros/compatibilidade:** rejeição do decoder é aceita pelo target; mudança dos tipos gerados, do trait ou da feature pode impedir o build ou alterar a superfície.

**INV / REL:** [INV-003](#inv-003) · [REL-002](BLAST_RADIUS.md#rel-002) · [REL-006](BLAST_RADIUS.md#rel-006).

**Prova:** [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs#L1-L19) · [protos](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/proto.rs#L39-L67). Nenhum teste foi executado.
[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Helper de request-id

**Símbolo/entrada:** `audit_request_id_for_blob(&str, uuid::Uuid, &str) -> String`; também usa `uuid::Uuid::from_bytes([u8; 16])`. Abaixo de 16 bytes retorna; depois, divide o restante com `String::from_utf8_lossy`.
**Saída/efeito:** descarta a `String`; sem evento, outbox ou IO no harness.
**Erros/compatibilidade:** função total no tipo; assinatura, UUID ou partição alterados mudam o target. O import `uuid::Uuid` não tem dependência direta no manifesto: `SOURCE incompatível / build UNKNOWN`, sem Cargo.
**INV / REL:** [INV-002](#inv-002) · [REL-003](BLAST_RADIUS.md#rel-003) · [REL-008](BLAST_RADIUS.md#rel-008).
**Prova:** [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs#L1-L25) · [helper](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/orchestrator.rs#L205-L221) · [manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L24-L27). Sem teste.
[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Parser de resource_name

**Símbolo exato:** `handler::parse_read_resource_name(&str) -> Result<ParsedReadResource, &'static str>`.

**Pré-condição/entrada:** o target chama o parser somente quando `std::str::from_utf8(data)` aceita os bytes; bytes inválidos não chegam ao parser.

**Resultado/efeito:** o `Result` é descartado; não há handler gRPC, transporte ou persistência no harness.

**Erros/compatibilidade:** erro de parsing é aceito; alteração da assinatura, domínio `str` ou feature `host-server` altera o target.

**INV / REL:** [INV-001](#inv-001) · [REL-004](BLAST_RADIUS.md#rel-004).

**Prova:** [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs#L1-L19) · [parser](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/handler/helpers.rs#L250-L296). Nenhum teste foi executado.

[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado / recurso | Owner e vida útil | Escrita / leitura / durabilidade |
|---|---|---|
| Bytes de entrada e valores temporários | Harness/libFuzzer durante um processo local ou runner; por invocação. | Nenhum armazenamento de aplicação aparece nos três corpos de target inspecionados. Dependências transitivas e ambiente de execução não foram executados. |
| Corpus e artifacts | Diretórios convencionais do package/runner; ownership e retenção ainda não especificados. | Workflow configura cache de corpus e upload de artifacts para dois targets; isso é configuração estática, não observação de execução. |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Domínio efetivamente encaminhado

**Regra:** `parse_read_resource_name` recebe do harness apenas strings UTF-8 válidas; não alegar fuzz de strings não UTF-8.
**Imposição:** ramificação `if let Ok(s) = std::str::from_utf8(data)` no target.
**Violação / prova:** a entrada inválida não chama o parser. A fonte estática mostra a condição; nenhum caso foi executado. Estado: declarado.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Prefixo e partição do target de request-id

**Regra:** menos de 16 bytes encerra a closure; a chamada usa UUID de 16 bytes e divide o restante em duas metades com UTF-8 lossily convertido.
**Imposição:** guards e conversões em `audit_request_id_total.rs`.
**Violação / prova:** uma alteração pode deixar regiões de entrada sem alcançar o helper. Fonte inspecionada; totalidade em runtime desconhecida.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Decodes não são asserts de domínio

**Regra:** o target protobuf tenta os dois tipos declarados e descarta ambos os resultados; sucesso ou rejeição de payload não é uma expectativa afirmada.
**Imposição:** chamadas `decode(data)` e bindings `_` no target.
**Violação / prova:** mudança que remova uma chamada reduz a superfície. Fonte estática inspecionada; cobertura executada desconhecida.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Entrada local ao harness

1. O runner/libFuzzer oferece bytes à closure.
2. A closure pode retornar cedo (UUID target) ou pular parser em UTF-8 inválido.
3. O target restante chama somente as funções/tipos listados em API-001–API-003.
4. Os valores retornados não são persistidos nem enviados a um serviço pelo código do harness inspecionado.
5. Em uma execução futura, crash/artifact/corpus dependem do runner; nenhum resultado foi observado aqui.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / fonte | Default declarado | Quando lido | Condição | Efeito / limite |
|---|---|---|---|---|
| `libfuzzer-sys = "0.4"`, `prost = "0.13"`, `corelink-reapi = { path = ".." }` | Restrições semver/path do manifesto. | Resolução/build do package. | Sem features do package fuzz declaradas. | Grafo e versões resolvidos não foram consultados por Cargo. |
| Import `uuid::Uuid` no target audit | O source importa `uuid`, mas o manifesto fuzz não declara a dependência direta. | Compilação/resolução do target audit. | Incompatibilidade SOURCE; `audit_request_id_total` e o build do package permanecem UNKNOWN até reconciliação. | Não adicionar ou remover dependência nesta documentação; exige mudança/decisão separada. |
| `[workspace]` próprio e `workspace.exclude` do pai | Presente no subpackage. | Descoberta do package. | Resolver isolado do workspace root. | Selecionar o package pai não equivale, por si só, a executar os bins fuzz. |
| `test = false`, `doc = false` nos três bins | Declarado em cada `[[bin]]`. | Descoberta de target de teste/doc. | Não desativa invocação `cargo fuzz`. | Não é prova de build, execução nem ausência de wiring externo. |
| `host-server` do pai | Feature `default = ["host-server"]`. | Build do path dependency se defaults forem habilitados. | `proto` e `handler` estão gated por essa feature. | Inferência a partir dos manifests/fontes; feature efetiva não resolvida nesta autoria. |
| Workflow nightly | Targets `proto_decode_batch_update` e `audit_request_id_total`; comando com limite configurado de 3600 s. | Trigger e matrix do workflow. | Runner macOS self-hosted, nightly preinstalado no workflow. | Configuração inspecionada; execução/resultado GitHub não consultados. |

**Matriz suportada observada:** apenas os dois primeiros targets são nomeados em `.github/workflows/nightly.yml`; `parse_read_resource_name` está declarado mas não apareceu na matrix pesquisada. [B06](BLAST_RADIUS.md#b06) registra o limite desta busca.
**Combinações recusadas:** não declarar configuração executável adicional sem uma seleção real e evidência.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal / erro | Causa possível | Estado após falha | Diagnóstico / limite |
|---|---|---|---|
| `Err` do decoder ou parser | Input não aceito pela mensagem/parser. | Resultado é ignorado pelo harness. | Não prova erro de serviço ou regressão por si só. |
| Panic/crash/timeout durante fuzz | Possível defeito no caminho executado ou no ambiente/fuzzer. | Pode haver corpus/artifact local/runner. | Preservar a entrada e logs; nenhuma ocorrência foi observada nesta autoria. |
| OOM/limite de recurso | Harness não define limite de memória no source inspecionado. | Resultado e contenção desconhecidos. | O objetivo em comentário não é evidência de que OOM foi evitado/detectado. |
| Diferença entre bin declarado e matrix | Target sem seleção encontrada em CI. | Estado de execução desconhecido. | Não inferir cobertura a partir de `test=false`/`doc=false`. |

**Limites:** o harness não emite métricas próprias, não audita um evento real e não observa tráfego do serviço; total de execuções/cobertura/corpus do runner permanece desconhecido.

<a id="r08"></a>
## R08 — Verificação e evidências

| Contrato / INV | Fonte no pin | Método nesta autoria | Resultado e limite |
|---|---|---|---|
| Manifesto, três targets e imports diretos | `crates/corelink-reapi/fuzz/Cargo.toml`; `fuzz_targets/*.rs` | `git show` do objeto local `1177dad2…`; comparação estática de imports e `[dependencies]`. | 3 declarações Cargo verificadas, mas `uuid::Uuid` é import não declarado; build/resolução UNKNOWN, sem Cargo. |
| Invocações | Três `fuzz_targets/*.rs` | `git show` do pin; inspeção estática. | Callsites verificados; sem compilação ou execução. |
| Helper de auditoria/parser e tipos proto | `corelink-reapi/src/{orchestrator.rs,handler/helpers.rs,proto.rs,lib.rs}` | Inspeção estática dos callsites/feature gates. | Estrutura verificada; comportamento dinâmico desconhecido. |
| Workflow/matrix | `.github/workflows/nightly.yml`; `.github/workflows/corelink-reapi.yml`; `scripts/fuzz-all.sh` | Busca local no commit exato; sem acesso a runs. | Dois targets nomeados na matrix; parser sem nome encontrado na busca; status GitHub desconhecido. |

O checkout de autoria parte de `8cdc02828132b9b6f03a3b57117b8140325f6762`; a referência está fixada no objeto local `1177dad2ca2a9f21c29b5a118aa7944b77147798`.

O diff de `crates/corelink-reapi/fuzz`, `crates/corelink-reapi/src` e manifests relevantes não mostrou alteração funcional; a comparação estática encontrou o import `uuid::Uuid` sem declaração direta no manifesto fuzz. O `Cargo.lock` raiz diverge fora desse escopo. Nenhuma atualização foi inferida da branch local divergente.

**Desconhecidos:** se a resolução/build tolera ou rejeita o import `uuid` não declarado; resultados e timestamps de runs; corpus em cache/runner; resolução exata; cobertura; memory budget; owner humano/rota; consumers fora do repositório e reachability de production.
**Revisão:** autora não aprovou esta referência; cold review independente pendente. Checks editoriais, se passarem, não mudam esse estado.

Continuar: [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
