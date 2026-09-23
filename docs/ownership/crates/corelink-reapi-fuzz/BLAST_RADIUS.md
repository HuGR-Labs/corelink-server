---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-reapi-fuzz
manifest: crates/corelink-reapi/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: reapi-fuzz-w016-1177dad2
---

# corelink-reapi-fuzz — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Leitura rápida e escopo

O package declara três targets e depende por path de `corelink-reapi`. Os targets chamam dois decoders, um helper de request-id e um parser.

O workflow nightly local nomeia dois targets. Nada nesta fonte prova que um job rodou, que o terceiro target é chamado em CI ou que qualquer caminho do harness é alcançável pelo serviço implantado.

**Seleção declarada:** package `corelink-reapi-fuzz`, manifesto próprio, sem features próprias; defaults do pai ainda não resolvidos. Nenhum runtime, runner ou serviço foi observado. Dependência, callsite, matrix e execução são estados distintos.

<a id="b02"></a>
## B02 — Populações e método

| População | Fonte/método no pin | Encontrado/documentado | Limite |
|---|---|---:|---|
| Targets e declarações | Manifesto fuzz + comparação com arquivos `fuzz_targets/`. | 3/3 | Não prova build nem inventário de artefatos locais ignorados. |
| Dependências diretas declaradas e imports | `[dependencies]` do manifesto comparado aos imports dos três targets. | 3 declarações; 1 import `uuid::Uuid` sem declaração direta | Build/resolução UNKNOWN; não promover dependência transitiva a dependência direta. |
| Chamadas do harness ao pai | Busca local por nomes e inspeção dos três targets, `lib.rs`, helper e parser. | 4 superfícies/4 | Callsite em source não é observação de runtime. |
| Consumers Cargo reversos do package fuzz | Busca literal de `corelink-reapi-fuzz` e caminho fuzz no commit pin. | 0 matches externos/0 | O workflow usa diretório `corelink-reapi`, não o nome do package; ecossistemas/repos externos não foram varridos. |
| Workflows/scripts | Busca literal pelos nomes de bin e inspeção de `nightly.yml`, `corelink-reapi.yml`, `fuzz-all.sh`. | 2 invocações declaradas/3 targets | Busca não coleta histórico de runs; parser não apareceu na matrix pesquisada. |
| Corpus / artefatos versionados | `git ls-tree` restrito ao package fuzz no pin. | 0 caminhos de corpus rastreados | Não cobre cache remoto, artefatos de workflow ou diretórios ignorados locais. |

**Resolução Cargo e semântica externa:** não executadas/não resolvidas. A busca não consulta GitHub, provider, bancos, storage, deployments ou produção. Relações de arquivos de workflow e comentários são configuração estática; não atestam execução.

<a id="b03"></a>
## B03 — Relações diretas

| ID | Tipo / direção de dependência | Superfície | Ativação declarada | Owner do contrato |
|---|---|---|---|---|
| [REL-001](#rel-001) | dependency: fuzz → corelink-reapi | Path dep usada pelos três bins. | Build/invocação do target. | corelink-reapi; pessoa não identificada. |
| [REL-002](#rel-002) | runtime-call candidato: fuzz → corelink-reapi | Dois decoders proto. | `proto_decode_batch_update`. | Corelink REAPI proto. |
| [REL-003](#rel-003) | runtime-call candidato: fuzz → corelink-reapi | `audit_request_id_for_blob`. | `audit_request_id_total`, bytes ≥16. | Helper no corelink-reapi. |
| [REL-004](#rel-004) | runtime-call candidato: fuzz → corelink-reapi | `parse_read_resource_name`. | `parse_read_resource_name`, entrada UTF-8. | Parser no corelink-reapi. |
| [REL-005](#rel-005) | dependency: fuzz → libfuzzer-sys | Macro `fuzz_target!` nos três targets. | Cargo-fuzz/libFuzzer build e run. | Provider externo; versão resolvida não observada. |
| [REL-006](#rel-006) | dependency: fuzz → prost | Trait `Message::decode` no target protobuf. | Proto target. | Provider externo; versão resolvida não observada. |
| [REL-007](#rel-007) | build-deploy/config: workflow → 2 targets | Matrix nightly para os dois primeiros bins. | Job do workflow, 3600s configurados. | Owner humano do workflow não verificado. |
| [REL-008](#rel-008) | source/dependency: fuzz → uuid | `uuid::Uuid` no target audit sem declaração direta. | Build/resolução do bin. | Incompatibilidade SOURCE; decisão de dependência pendente. |

<!-- corelink-ownership relations: REL-001 through REL-008; all peers pending independent reconciliation. -->

<a id="rel-001"></a>
### REL-001 — Path dependency para corelink-reapi
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-corelink-reapi-dep-001`.

**Dependência:** package fuzz → `corelink-reapi` por `path = ".."`.
**Fluxo de dados:** harness → decoder/helper; resultados são descartados nos três targets.

**Impacto:** mudança de assinatura/tipo no pai pode quebrar o harness; falha de harness pode reduzir evidência sobre o pai. Não há impacto de produção demonstrado.
**Ativação:** seleção/build de um bin; feature resolvida não verificada, embora o pai declare `host-server` default.

**Contrato / contenção:** funções/tipos listados em REL-002–004; fronteira para no código Rust local.
**Validação:** build/run de cada target e testes do pai escolhidos por mudança; não executados.
**Coordenação / fontes:** owner humano e doc peer pendentes; [manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L24-L45). [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Decoders de mensagens protobuf
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-proto-decode-001`.

**Dependência:** fuzz → corelink-reapi; também `prost` direto.
**Fluxo de dados:** bytes da closure → dois `Message::decode`; resultados ignorados.

**Impacto:** schema/tipo gerado incompatível → compilação ou superfície do harness; crash do alvo sinalizaria apenas o caminho executado.
**Ativação:** bin protobuf; módulos `proto` gated por `host-server` no pai.

**Contrato / contenção:** decoder local, sem chamada ao handler gRPC no harness.
**Validação:** alvo bounded e revisão de schema; nenhum run observado.
**Coordenação / fontes:** owner proto desconhecido; [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/proto_decode_batch_update.rs#L1-L19), [aliases proto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/proto.rs#L39-L67). [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Derivação de request-id
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-audit-request-id-001`.

**Dependência:** fuzz → corelink-reapi.
**Fluxo de dados:** segmentos lossily decodificados + UUID derivado dos primeiros 16 bytes → helper → `String` descartada.

**Impacto:** mudança do helper afeta o alvo; target crash não atesta atomicidade do outbox nem entrada gRPC real.
**Ativação:** bin audit e comprimento mínimo de 16 bytes.

**Contrato / contenção:** helper retorna concatenação textual; sem IO no callsite inspecionado.
**Validação:** alvo bounded e suite de idempotência do pai, conforme M04; não executados.
**Coordenação / fontes:** owner de auditoria no pai não verificado; [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs#L1-L25), [helper](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/orchestrator.rs#L205-L221). [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Parser de resource_name
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-read-resource-name-001`.

**Dependência:** fuzz → corelink-reapi.
**Fluxo de dados:** bytes UTF-8 → parser → `Result` descartado; bytes inválidos são pulados.

**Impacto:** mudança do parser/feature pode quebrar alvo; target verde não prova parsing via RPC ou resposta de rede.
**Ativação:** bin parser com entrada UTF-8 válida.

**Contrato / contenção:** parser local retorna parsed resource ou erro; sem handler chamado diretamente.
**Validação:** alvo bounded e testes unitários do pai; target não apareceu na matrix nightly inspecionada.
**Coordenação / fontes:** owner do parser desconhecido; [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/parse_read_resource_name.rs#L1-L19), [parser](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/src/handler/helpers.rs#L250-L296). [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Framework libFuzzer
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-libfuzzer-001`.

**Dependência:** fuzz → `libfuzzer-sys` (faixa `0.4`).
**Fluxo de dados:** framework fornece bytes à closure; artifact/corpus ficam sob runner.

**Impacto:** versão/toolchain incompatível impede build/run; achado pode produzir crash artifact.
**Ativação:** qualquer bin por cargo-fuzz.

**Contrato / contenção:** lints proíbem unsafe, `panic!`, `unwrap`, indexação e `todo!`; não provam execução sem panic.
**Validação:** confirmar versão resolvida e run isolado; não executados.
**Coordenação / fontes:** provider externo e versão resolvida desconhecidos; [manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L14-L27). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Trait prost para decode
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-prost-001`.

**Dependência:** fuzz → `prost` (faixa `0.13`).
**Fluxo de dados:** bytes → `Message::decode` para dois tipos gerados.

**Impacto:** API/versão incompatível pode impedir o alvo; sucesso não atesta handler/protocolo completo.
**Ativação:** somente target protobuf.

**Contrato / contenção:** dependency direta declarada; resolução não observada.
**Validação:** build/run isolado e compatibilidade de tipos; não executados.
**Coordenação / fontes:** provider externo; [manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L24-L27). [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Matrix nightly de fuzz
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-nightly-matrix-001`.

**Dependência:** configuração workflow → seleção de package pai/target; não é aresta Cargo.
**Fluxo de dados:** estado not-applicable; input vem do runner/libFuzzer.

**Impacto:** matrix omitida/desatualizada muda a execução pretendida; status verde depende de run externo.
**Ativação:** matriz em `.github/workflows/nightly.yml`, com comando configurado `cargo fuzz run … -- -max_total_time=3600`.

**Contrato / contenção:** nomes na matrix são `corelink-reapi` + target; parser não está listado.
**Validação:** leitura local do YAML; nenhum provider ou run consultado.
**Coordenação / fontes:** owner do workflow desconhecido; [matrix e comando](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/nightly.yml#L354-L430). [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Import UUID sem dependência direta
**Identidade:** `repo:1232040291:boundary:reapi-fuzz-uuid-import-001`.

**Tipo/direção/superfície:** source/dependency; target fuzz → `uuid`; `uuid::Uuid::from_bytes` no target audit.
**Dados/ativação:** primeiros 16 bytes aceitos viram UUID; exige resolução/compilação, não observada.
**Impacto/falha:** `uuid` não está em `[dependencies]`; target/package ficam `SOURCE incompatível / UNKNOWN`. Não inferir acesso a dependência transitiva.
**Estado/efeito:** construção local; nenhum UUID real ou estado de aplicação.
**Boundary/contensão:** import ↔ manifesto; sem runtime, storage ou autorização.
**Validação:** comparar imports e manifesto; Cargo/build não executados. **Coordenação/evidência:** owner e decisão desconhecidos; [target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/fuzz_targets/audit_request_id_total.rs#L7-L19) · [manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-reapi/fuzz/Cargo.toml#L24-L27). [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho testemunha | Condição | Efeito causal | Contenção / validação |
|---|---|---|---|---|
| Mensagens REAPI geradas | REL-001 → REL-002 → `proto.rs` → `tonic::include_proto!` | Feature `host-server` e build de proto. | Mudança proto/codegen pode alterar decode/build do harness; callsite não é RPC. | Revisar proto, build wiring e fuzz target; nenhum build executado. |
| Helper request-id | REL-008 → REL-003 → `orchestrator.rs` | Target audit com entrada ≥16 bytes e import UUID resolvido. | API/helper ou dependência ausente bloqueia o target; helper compõe string local. | Corrigir/reconciliar manifesto e depois teste de idempotência/fuzz; não executados. |
| Parser Read | REL-001 → REL-004 → `handler::parse_read_resource_name` | Feature pai e bytes UTF-8. | Parser muda → target; não há transporte no caminho direto. | Testes parser e fuzz; matrix não o seleciona no arquivo revisto. |
| Fuzz runtime | REL-005/006 → bin escolhido → closure | cargo-fuzz, toolchain e dependências funcionais. | Crash/timeout/artifact local; efeito fora do processo desconhecido. | Executar em checkout isolado e conservar evidência; não executado. |
| Nightly | REL-007 → matriz de dois bins → `cargo fuzz run` | Workflow e runner executados com a configuração do commit. | Execução pode falhar antes/durante o fuzzer; presença de YAML não prova resultado. | Verificar run em sistema autorizado; fora desta autoria. |

**Outras fronteiras:** nenhuma tabela/bucket/SQL/HTTP/event stream é mencionada diretamente nos corpos de fuzz. Isso não prova ausência de efeitos em dependências transitivas ou no ambiente do runner. Nenhum fluxo para production foi demonstrado.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | APIs / INVs / RELs | Consumidores / estado | Validação necessária | Coordenação / recuperação |
|---|---|---|---|---|
| Alterar proto ou generated type | API-001, INV-003, REL-002, REL-006 | Target protobuf e seleção nightly; sem dado persistente demonstrado. | Build e fuzz bounded do alvo; verificar teste pai pertinente. | Owner do proto não verificado; reverter somente mudança própria e preservar crash input. |
| Alterar helper de request-id | API-002, INV-002, REL-003, REL-008 | Target audit e contratos de idempotência do pai; import UUID precisa estar declarado. | Reconciliar manifesto/source antes de suite `prop_idempotency` e fuzz bounded; M04. | Owner do contrato desconhecido; compatibilidade da string deve ser decidida no pai. |
| Alterar parser Read | API-003, INV-001, REL-004 | Target parser, testes unitários, handler/consumers do pai. | Testes parser e fuzz; cobrir UTF-8 inválido separadamente se domínio mudar. | Reconciliar eventual seleção CI; nenhum status de execução afirmado. |
| Renomear bin/alterar dependência | R01, REL-005–008 | Manifest, imports, lockfile e matrix; `scripts/fuzz-all.sh` não lista este package. | Conferir imports/deps e todas as invocações/locks no pin; parar se houver mismatch. | A rota de owner workflow não foi identificada. |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População inspecionada | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| Bins no manifesto | 3 | 3 | 0 | 0 na enumeração estática |
| Direct deps no manifesto | 3 | 3 | 0 | Resolve/transitivas/features; import UUID não declarado |
| Imports versus manifest | 1 (`uuid::Uuid`) | 1 incompatibilidade registrada | 0 | Build/resolução do target audit |
| Superfícies de callsite dos targets | 4 (2 decodes + 2 helpers) + 1 import UUID | 5 | 0 | Execução/cobertura |
| Invocações nomeadas no nightly matrix | 2 | 2 | 0 | Histórico e estado do run |
| Targets sem invocação nomeada na matrix consultada | 1 (`parse_read_resource_name`) | 1 lacuna registrada | 0 | Outros invocadores não descobertos fora dos arquivos buscados |
| Corpus rastreado sob `fuzz/` no pin | 0 | 0 | 0 | Corpus local/cache/artifacts |

**Exclusões enumeradas:** `scripts/fuzz-all.sh` não lista este package. O workflow do pai executa testes do package pai, não prova run de fuzz.

A matrix noturna nomeia proto e audit; isso não prova ausência em outros providers. Nenhum run foi consultado.

**Censo fora de Cargo:** busca de nomes exatos em workflows/scripts no objeto local, inspeção dos caminhos acima e de `Cargo.toml` raiz; não foi feita consulta GitHub ou a sistemas externos.

Não foram encontrados identificadores do terceiro alvo na matrix/nightly consultada.
**Diferença para censo independente:** ainda não realizada. **Owner/peers e dependências resolvidas:** pendentes.
**Não observado:** compilação (o audit target tem import UUID não declarado; build package-wide permanece UNKNOWN), `cargo fuzz`, testes Rust, crash corpus, cobertura, CI, runtime de serviço, memória/CPU real ou persistência de artifacts.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
