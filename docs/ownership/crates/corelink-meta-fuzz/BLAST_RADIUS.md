---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-meta-fuzz
manifest: crates/corelink-meta/fuzz/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: "S"
state: draft
evidence_set: corelink-meta-fuzz-structural-normalization-20260921
---

# corelink-meta-fuzz — blast radius

O package depende diretamente de corelink-meta, corelink-hash e três crates externas. Os targets consomem bytes de fuzz e chamam somente InMemoryMetaStore; nenhum caminho deles alcança o adapter D1. Workflows configuram execuções, mas run status, alcance e gate de merge não foram observados.

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) ·
[Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Leitura rápida e escopo

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008)

As relações diretas abaixo vêm do manifesto e das chamadas dos dois targets no commit fixado. RELs separam direção de dependência, fluxo de dados e propagação de impacto. Um workflow configurado é wiring; sem run record, não prova execução. O alcance em produção do provider corelink-meta fica fora deste package.

**Builds avaliados:** dois bin targets declarados; nenhuma feature declarada; Cargo metadata não executado.
**Ambientes não observados:** runner, run logs, corpus de campanha e aplicação de branch protection.

<a id="b02"></a>
## B02 — Inventário e método

| População | Fontes / método | Seleção / revisão | Limite |
|---|---|---|---|
| Manifesto e targets | Cargo.toml e git ls-tree no source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` | Um package fuzz; dois bins declarados | Não equivale a cargo metadata/resolução |
| Dependências | Chaves diretas em Cargo.toml | Cinco dependências declaradas | Versões resolvidas, features e inversas não verificadas |
| Fora de Cargo | Busca literal de package/path e nomes dos dois targets em arquivos rastreados | Workflow, nightly e specs encontrados | Texto de workflow/spec não é log de execução |
| Dados e estado | Dois targets, fake/store de corelink-meta | Chamadas lidas em fonte fixada | Não mede D1, R2 ou runtime de worker |

A busca fora de Cargo localizou comandos de ambos os targets em `.github/workflows/corelink-meta.yml` (PR 60s; nightly 3600s declarado) e entrada na matriz de `.github/workflows/nightly.yml` (3600s; artefato em falha). O schedule do workflow per-crate está comentado; a matriz raiz é a superfície ativa. Specs e relatórios não são run logs. A busca por `corelink_meta` também encontra uso de MetaStore em corelink-worker, mas não dependência do fuzz package.

<a id="b03"></a>
## B03 — Registro completo de relações diretas observadas

| ID | Tipo / direção | Superfície | Ativação | Owner do contrato |
|---|---|---|---|---|
| [REL-001](#rel-001) | Cargo: harness→provider | corelink-meta | ambos os bins | Package corelink-meta; pessoa desconhecida |
| [REL-002](#rel-002) | Cargo: harness→provider | corelink-hash::Digest | ambos os bins | Package corelink-hash; pessoa desconhecida |
| [REL-003](#rel-003) | Cargo: harness→externo | libfuzzer-sys | ambos os bins | Terceiro; não verificado |
| [REL-004](#rel-004) | Cargo: harness→externo | uuid | ambos os bins | Terceiro; não verificado |
| [REL-005](#rel-005) | Cargo: harness→externo | futures-executor | ambos os bins | Terceiro; não verificado |
| [REL-006](#rel-006) | Workflow PR→harness | `corelink-meta.yml::fuzz-smoke` | `pull_request`, 60s | YAML; merge gate não comprovado |
| [REL-007](#rel-007) | Workflow nightly declarado→harness | `corelink-meta.yml::fuzz-nightly` | schedule comentado, 3600s declarado | YAML dormente |
| [REL-008](#rel-008) | Matriz raiz→harness | `nightly.yml::fuzz-matrix` | schedule/dispatch, 3600s | YAML ativo; run não comprovado |

REL-001..008 usam endpoints qualificados e fingerprints estáveis; IDs locais são âncoras. Os fatos compartilhados de REL-006/007/008 são package, targets, commands e source pin; efeitos locais diferem. O owner canônico é a configuração YAML, não o harness. Reconciliar por fingerprint antes de aceitar mudança.

<a id="rel-001"></a>
### REL-001 — Provider corelink-meta e fake
**Identidade compartilhada:** `repo:1232040291:boundary:meta-fuzz-meta-provider-001`; peer: [corelink-meta B06](../corelink-meta/BLAST_RADIUS.md#b06).

A relação é o path dependency `..` no manifesto independente e as chamadas `MetaStore::{commit_put,get}`/`InMemoryMetaStore` nos dois targets.

**Dependência:** corelink-meta-fuzz → corelink-meta.
**Dados:** bidirecional; requests entram em commit_put e outcomes/rows retornam ao harness.
**Impacto:** corelink-meta → harness/CI; mudança de API/semântica pode quebrar compilação ou assertion.
**Ativação:** dois bins; ambos constroem InMemoryMetaStore.
**Estado:** BTreeMap/Mutex local por callback; sem D1.
**Contenção:** nenhuma chamada ao worker/D1 no target.
**Validação:** PROC-001/002; execução pendente.
**Coordenação / fonte:** `crates/corelink-meta/src/fake.rs` e `store.rs` em `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; consumer-side counterpart: [corelink-meta B06](../corelink-meta/BLAST_RADIUS.md#b06). [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Provider corelink-hash
**Identidade compartilhada:** `repo:1232040291:boundary:meta-fuzz-hash-001`; hash-side record: [corelink-hash REL-032](../corelink-hash/BLAST_RADIUS.md#rel-032). A relação é a path dependency `../../corelink-hash` e o uso de `Digest` pelos dois targets.

**Dependência:** corelink-meta-fuzz → corelink-hash.
**Dados:** bidirecional; bytes viram hex lowercase e Digest::from_hex retorna valor para BlobMetaKey.
**Impacto:** corelink-hash → harness/CI se tipo/API/semântica mudar; nenhum impacto runtime de volta foi observado.
**Ativação:** código de ambos os bins.
**Estado:** valor digest em memória.
**Contenção:** nenhuma escrita de storage.
**Validação:** PROC-001/002; não executada.
**Coordenação / fonte:** Cargo.toml e digest.rs no commit fixado. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Runtime libfuzzer
**Identidade:** dependência libfuzzer-sys = 0.4 e macro fuzz_target.
**Dependência:** corelink-meta-fuzz → libfuzzer-sys.
**Dados:** provider fuzzer → callback; callback finaliza por return ou finding.

**Impacto:** assertion/panic do target → processo fuzz/job; campanha real não observada.
**Ativação:** bin executado por cargo-fuzz.
**Contrato:** versão resolvida não verificada; não inferir alcance de inputs.
**Estado:** input/finding local ou artifact de campanha.
**Contenção:** nenhum acesso D1/R2 no código do target.
**Validação:** PROC-001/002, pendentes.
**Coordenação / fonte:** fuzz manifest e ambos os targets. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — UUID
**Identidade:** dependência direta uuid = 1.
**Dependência:** corelink-meta-fuzz → uuid.
**Dados:** fatias de 16 bytes viram Uuid::from_bytes e entram no BlobMetaKey/AuditEvent.
**Impacto:** mudança incompatível ou semântica alterada pode afetar build/input do harness.
**Ativação:** ambos os targets.
**Estado:** UUID somente em memória.
**Limite:** resolução e versão real não examinadas via Cargo.
**Validação:** PROC-001/002; não executada.
**Coordenação / fonte:** manifesto e imports dos targets. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Executor de futures
**Identidade:** dependência direta futures-executor = 0.3.
**Dependência:** corelink-meta-fuzz → futures-executor.
**Dados:** block_on aguarda commit_put/get no mesmo callback; resultados retornam ao oráculo.
**Impacto:** mudança do provider/error pode afetar assertions; bloqueio/hang afeta campanha/job.
**Ativação:** todos os callbacks que passam o length check.
**Estado:** executor síncrono; nenhum runtime remoto.
**Limite:** não prova concorrência real.
**Validação:** PROC-001/002; não executada.
**Coordenação / fonte:** ambos os targets no commit fixado. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Workflow PR smoke
**Identidade:** `repo:1232040291:boundary:meta-fuzz-pr-smoke-001`; fingerprint `wf:corelink-meta.yml:pull_request:fuzz-smoke:60s:v1`.
**Endpoints / contrato:** `corelink-meta-fuzz` → `corelink-meta.yml::fuzz-smoke`; owner canônico é o YAML; targets são ambos os bins.
**Dados / impacto:** libFuzzer → callback; exit status/finding → PR job; merge enforcement não observado.
**Ativação:** `pull_request` com path match; 60s por target em `corelink-meta.yml:9-16,149-197`.
**Estado / validação:** configuração ativa no source; sem run/log. **Fonte:** S07. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Workflow per-crate nightly declarado, sem schedule
**Identidade:** `repo:1232040291:boundary:meta-fuzz-per-crate-nightly-001`; fingerprint `wf:corelink-meta.yml:schedule:fuzz-nightly:3600s:dormant-v1`.
**Endpoints / contrato:** `corelink-meta-fuzz` → `corelink-meta.yml::fuzz-nightly`; ambos os targets são declarados, mas o `on.schedule` está comentado em `:22-32`.
**Dados / impacto:** se rearmado, bytes → target → job; atualmente não há trigger per-crate ativo.
**Ativação:** guard `if: github.event_name == 'schedule'`; commands `:199-245`; schedule removido.
**Estado / validação:** declaração dormente, sem run; não contar como segundo nightly ativo. **Fonte:** S07. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Matriz nightly raiz efetiva
**Identidade:** `repo:1232040291:boundary:meta-fuzz-root-nightly-001`; fingerprint `wf:nightly.yml:fuzz-matrix:corelink-meta:3600s:v1`.
**Endpoints / contrato:** `corelink-meta-fuzz` → `.github/workflows/nightly.yml::fuzz-matrix`; matriz inclui ambos os targets.
**Dados / impacto:** bytes → target; failure → job/artifact; schedule/dispatch pode consumir runner.
**Ativação:** `on.schedule`/`workflow_dispatch`, matrix `:356-378`, command `:428-430`; 3600s por target.
**Estado / validação:** wiring ativo; upload em falha por 14 dias; run não observado. **Fonte:** S08. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho por RELs | Condição | Efeito causal | Contenção / validação |
|---|---|---|---|---|
| Job PR/schedule | REL-001..008 | Trigger ativo seleciona target; REL-007 é dormente | Compilação, assertion ou timeout muda job quando executado | Logs e regra de merge não observados |
| API/fake corelink-meta | REL-001 | Mudança do provider | Harness pode falhar ou deixar de detectar regressão | Rodar ambos os PROC aplicáveis e rever semântica |
| corelink-worker | Nenhuma REL a partir do fuzz package | Mudança separada em corelink-meta | Worker importa MetaStore em seu próprio caminho | Revisar package/provider e seus owners; não atribuir alcance ao fuzz |

O package não grava estado compartilhado. Uma falha do harness não é execução de uma mudança de production path. A busca encontrou consumer de MetaStore em corelink-worker, mas a população inversa não foi conciliada e nenhuma chamada foi traçada a partir dos fuzz targets.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | API / INV / REL | Consumidores / estado | Validação necessária | Coordenação / recuperação |
|---|---|---|---|---|
| Alterar input ou oracle do round-trip | API-001/003; INV-001/002; REL-001/002 | Target e job; memória efêmera | PROC-001; conferir resultado e limites de input | Reviewer independente; guardar finding |
| Alterar retries/conflict oracle | API-002; INV-003; REL-001 | Target e job; fake local | PROC-002; conferir payload igual/divergente | Não alegar rollback D1 |
| Atualizar dependência local | REL-001/002 | Ambos os bins podem deixar de compilar ou mudar oracle | PROC-001 e PROC-002 | Owner do contrato do provider não confirmado |
| Alterar workflow | REL-006–008 | PR smoke ativo; per-crate nightly dormente; matriz raiz 3600s ativa | Conferir triggers, targets, budgets, artifacts e run status | Não rearmar schedule duplicado sem decisão; Git não reverte runner/artifact |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| Bins no manifesto | 2 | 2 | 0 | Resolução Cargo |
| Dependências diretas declaradas | 5 | 5 | 0 | Versões resolvidas/inversas |
| Arquivos de workflow com target explícito | 2 | 2; 3 fingerprints | 0 | Execuções e merge enforcement |
| Corpus e campaign runs | 0 observado | 0 | 0 | Estado completo |

**Exclusões:** nenhuma dependência foi classificada como vendor/fixture; não houve censo rastreado de todos os manifestos independentes.
**Diferença para censo independente:** censo Cargo ainda não executado.
**Não observado:** corpus, alvo efetivo do run, resoluções, consumers completos de providers, logs e owners operacionais.
A igualdade de contagens não prova completude. Reconciliar fingerprints REL-006/007/008 quando workflow, target ou orçamento mudar.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
