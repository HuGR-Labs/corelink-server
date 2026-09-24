---
schema: corelink-ownership/1.1
document: reference
package: corelink-meta-fuzz
manifest: crates/corelink-meta/fuzz/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: "S"
state: draft
evidence_set: corelink-meta-fuzz-structural-normalization-20260921
---

# corelink-meta-fuzz — referência de ownership

Harness package independente com dois targets cargo-fuzz. O código dos dois targets cria InMemoryMetaStore por entrada e exercita APIs da crate corelink-meta. A fonte não chama adapter D1 nem prova que campanha de fuzz foi executada.

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001) · [FLOW-002](#flow-002)

| Campo | Valor verificado |
|---|---|
| Package / manifesto | corelink-meta-fuzz / crates/corelink-meta/fuzz/Cargo.toml |
| Targets declarados | bins commit_put_roundtrip e audit_idempotency; nenhum lib target declarado |
| Papel | Workspace Cargo independente; harness, não serviço ou adapter |
| Licença / publicação | UNLICENSED / publish=false no manifesto |
| Implementação | Dois arquivos de target e manifesto lidos no source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` |
| Wiring | PR smoke de 60s está ativo; `fuzz-nightly` declara comandos de 3600s, mas o `on.schedule` de `corelink-meta.yml` está comentado; a matriz raiz mantém os dois targets em 3600s |
| Runtime observado | Nenhum run foi coletado neste estudo; resultado operacional desconhecido |

O manifesto inclui [a chave cargo-fuzz](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/Cargo.toml#L1-L43). Isso identifica o propósito/configuração, não comprova execução.

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Implementação | Contrato | Operação / escalonamento |
|---|---|---|---|
| Harnesses | corelink-meta-fuzz | Assertions locais do target | Runner configurado nos workflows; pessoa/rota não confirmada |
| MetaStore e fake | corelink-meta | Trait e fake nessa crate | Adapter D1/worker fora do harness |
| Digest | corelink-hash | Tipo e parsing nessa crate | Owner humano não identificado |
| Review | Código de configuração CODEOWNERS | Catch-all @gmhelmold solicita review | Arquivo diz que a regra não impõe merge; reviewer independente não confirmado |

**Não faz:** não instancia D1, não aplica migration, não chama R2 ou corelink-worker. MetaStore documenta o adapter real como trabalho futuro em corelink-worker; os targets importam somente InMemoryMetaStore.
**Aliases e inventário Cargo:** não conciliados por Cargo metadata nesta autoria.
**Conceitos canônicos:** o predicado testado aqui é local ao fake; não substitui o contrato ou documentação da crate provedora.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entrada | Papel e dado | Natureza | Fonte |
|---|---|---|---|
| fuzz/Cargo.toml | Workspace próprio, dependências e dois bin targets | Manifesto | [S01](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/Cargo.toml) |
| fuzz_targets/commit_put_roundtrip.rs | Bytes → tenant/digest/request → commit_put, get e retry | Harness | [S02](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/fuzz_targets/commit_put_roundtrip.rs) |
| fuzz_targets/audit_idempotency.rs | Repetições com request estável e payload igual/diferente | Harness | [S03](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/fuzz_targets/audit_idempotency.rs) |
| corelink-meta/src/fake.rs e store.rs | Implementa o fake e declara o trait exercitado | Provider; não pertence a este package | [S04](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/src/fake.rs), [S05](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/src/store.rs) |
| corelink-hash/src/digest.rs | Digest usado para compor BlobMetaKey | Provider | [S06](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-hash/src/digest.rs) |

O manifesto declara cinco dependências diretas: corelink-meta e corelink-hash por path; libfuzzer-sys, uuid e futures-executor por versão compatível. Versões efetivamente resolvidas e inversas não foram levantadas. O root Cargo exclui este caminho e o manifesto do fuzz contém [workspace] próprio.

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Oráculo de commit_put_roundtrip
**Símbolos:** `fuzz_target!`, `CommitPutRequest`, `MetaStore::commit_put/get`, `InsertOutcome`, `BlobMetaRow`.
**Entrada:** ao menos 96 bytes; os primeiros campos produzem UUID, digest, size, timestamp e seeds.
**Pós-condição assertada:** Inserted; get preserva size/timestamps, refcount=1 e linha viva; retry retorna AlreadyExists, uma row e um outbox entry.
**Limite:** commit_put que retorna Err encerra o callback sem assertion sobre a causa.
**Âncoras:** `commit_put_roundtrip.rs:25-32,62-105`; provider `store.rs:97-114,140-146`. **Relações:** [REL-001](BLAST_RADIUS.md#rel-001), [REL-002](BLAST_RADIUS.md#rel-002).
[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Oráculo de audit_idempotency
**Símbolos:** `fuzz_target!`, `AuditEvent::cas_put_completed`, `MetaStore::commit_put`, `MetaError::AuditIdempotencyConflict`.
**Entrada:** ao menos 68 bytes; 2–9 tentativas, mesmo key/request/event e payload derivado de bytes.
**Pós-condição assertada:** retry de payload idêntico não conflita; divergência deve dar AuditIdempotencyConflict; row_count permanece em um e outbox em no máximo um.
**Limite:** é estado do fake em um key já inserido, não rollback medido no D1.
**Âncoras:** `audit_idempotency.rs:23-30,43-97`; provider `store.rs:97-114`. **Relações:** [REL-001](BLAST_RADIUS.md#rel-001), [REL-002](BLAST_RADIUS.md#rel-002).
[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Orçamento de tamanho do primeiro target
**Símbolo:** transformação de size_bytes em commit_put_roundtrip.
**Entrada:** u64 do input; o target transforma-o em valor entre 1 byte e 5 MiB antes da chamada.
**Efeito:** zero e valores acima do teto são evitados; a assertion cobre o fake com o valor transformado.
**Limite:** não testa rejeição de size zero, política de limite externo, upload R2 nem validação de input real.
**Âncoras:** `commit_put_roundtrip.rs:32-60`; **Relação:** [REL-001](BLAST_RADIUS.md#rel-001).
[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Semântica e limite do fake
**Símbolos:** InMemoryMetaStore::new, MetaStore::commit_put/get, row_count e outbox_snapshot.
**Imposição observada:** BTreeMap em memória protegido por Mutex; commit_put planeja o outbox, altera a row e finaliza o outbox sob o fake.
**Limite:** a implementação comenta que simula batch atômico; a API usada no harness não é D1 nem persistência, e o teste não mede equivalência universal.
**Âncoras:** `fake.rs:81-130,228-250,283-335`; `store.rs:92-114`. **Relação:** [REL-001](BLAST_RADIUS.md#rel-001).
[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Chave e owner | Vida útil | Escrita / leitura |
|---|---|---|---|
| Input do callback | Bytes de libFuzzer; mutador externo | Uma chamada ao target | Cada target decodifica uma fatia própria |
| Store exercitado | BTreeMap rows/outbox; fake em corelink-meta | Nova instância dentro de cada callback | commit_put, get, row_count e outbox_snapshot |
| D1/blob_meta/audit_outbox | Fora do estado do harness | Não criada nem lida pelo target | Nenhuma chamada de adapter no código dos targets |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001) · [FLOW-002](#flow-002).

<a id="inv-001"></a>
### INV-001 — Domínio de entrada do round-trip
**Regra:** inputs menores que 96 bytes retornam; size é normalizado para 1..5 MiB.
**Imposição:** `commit_put_roundtrip.rs:32-35,56-60` faz o length check e sanitização antes da request; [REL-001](BLAST_RADIUS.md#rel-001).
**Limite:** não exercita size zero nem parser malformado; fonte é `commit_put_roundtrip.rs:38-67`.
[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Resultado e retry do primeiro target
**Regra assertada:** primeira inserção e get preservam os campos conferidos; mesma key no retry não cria outra row/outbox entry.
**Imposição:** `commit_put_roundtrip.rs:74-104` e [REL-001](BLAST_RADIUS.md#rel-001) usam callback e store novo.
**Violação / prova:** panic/assertion encerra o input; não há execução e não prova D1.
[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Idempotência de payload no segundo target
**Regra assertada:** request/event repetido aceita payload idêntico e rejeita payload divergente com AuditIdempotencyConflict.
**Imposição:** `audit_idempotency.rs:50-97` e [REL-001](BLAST_RADIUS.md#rel-001) controlam loop/assertions.
**Limite:** conta rows e limita outbox; não confere banco persistente nem todos os campos.
[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Um fake novo por input
**Regra:** nenhum estado de row/outbox é compartilhado entre invocações do callback no target.
**Imposição:** `commit_put_roundtrip.rs:74` e `audit_idempotency.rs:50` instanciam o fake dentro da closure; [REL-001](BLAST_RADIUS.md#rel-001).
**Violação / prova:** persistência entre inputs não é propriedade deste harness.
[Índice de estado](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Fuzz intent não é execução
**Regra:** código do target e comando em workflow demonstram implementação/configuração; não demonstram campanha executada ou cobertura/runtime.
**Imposição:** `corelink-meta.yml:149-197,199-245` e `nightly.yml:356-430` são configuração; nenhuma evidência de execução foi coletada.
**Violação / prova:** somente logs/run records e revisão independente sustentariam campanha; [REL-006](BLAST_RADIUS.md#rel-006), [REL-007](BLAST_RADIUS.md#rel-007), [REL-008](BLAST_RADIUS.md#rel-008).
[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Round-trip em memória
1. libFuzzer fornece bytes ao callback.
2. O target ignora input curto, gera key/audit e normaliza size.
3. Cria InMemoryMetaStore e chama commit_put via block_on.
4. Para em Err; se Ok, exige Inserted e confere get.
5. Repete o commit com a mesma key/audit e exige AlreadyExists.
6. Confere contagem local de row e outbox; descarta o store ao fim do callback.
[Índice de estado](#r05)

<a id="flow-002"></a>
[↩](#r01)
### FLOW-002 — Retry audit idempotente em memória
1. O target ignora input curto e cria key, store e payload seed.
2. Tenta commit_put de 2 a 9 vezes com request/event estáveis.
3. Payload igual deve retornar Ok; payload divergente deve retornar conflito.
4. Em conflito, confere row_count; ao final limita outbox a uma row.
5. Store e estado desaparecem ao fim do callback; nenhuma persistência é chamada.
[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / fonte | Default efetivo | Quando é lido | Target / condição | Efeito / falha |
|---|---|---|---|---|
| cargo-fuzz metadata | true declarado | Manifesto | Package independente | Marca intenção/estrutura; não prova execução |
| Features | Nenhuma seção de features declarada | Cargo | Dois bins do manifesto | Cargo metadata não foi executado |
| unsafe/lints | unsafe_code=forbid; lints de panic/unwrap/indexing/todo/unimplemented deny | Manifesto; targets permitem alguns lints localmente | Ambos os bins | Não torna assertion uma falha de produção |
| Limite de campanha | 60s PR; 3600s nos comandos nightly | PR smoke ativo; nightly per-crate com schedule comentado; matriz raiz ativa | Ambos os targets | Tempo declarado; resultado e duração real desconhecidos |

Workflow PR usa `crates/corelink-meta` e 60s; o job nightly per-crate usa o mesmo diretório, mas seu schedule está comentado; a matriz raiz usa 3600s. A versão cargo-fuzz 0.13.1 e nightly são configuração. Features, toolchain local e dependências não foram resolvidos via Cargo.

<a id="r07"></a>
## R07 — Erros e observabilidade

| Sinal / erro | Causa no contrato | Estado após falha | Diagnóstico |
|---|---|---|---|
| Input curto | Menor que o limiar de cada target | Callback retorna sem assertion | Não significa input validado pelo provider |
| Error em commit_put_roundtrip | Erro do fake | Target retorna sem classificar o erro | Não prova sucesso nem testa motivo do erro |
| Assertion/panic em target | Oráculo violado ou erro inesperado | Processo fuzz sinaliza finding | Preservar input/log e separar harness vs provider |
| AuditIdempotencyConflict | Payload diferente para request/event já armazenado | Target espera conflito e conta rows | Estado local do fake; não é log de D1 |

Os targets permitem panic/unwrap para que o fuzzer os trate como findings. A severidade de um finding não é classificada pelo target. Não há métrica, corpus rastreado ou output de execução verificado nesta autoria.

<a id="r08"></a>
## R08 — Verificação e evidências

| ID | Fonte e revisão | Método | Resultado e limite |
|---|---|---|---|
| S01 | fuzz/Cargo.toml, blob 67d11bdb8fb8751e143301e2da9b8eb79f0165ab | leitura em `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` | Fonte inspecionada; Cargo não executado |
| S02 | commit_put_roundtrip.rs, blob 5857a8316fbedffe1bb5e14b66c1f9d4cada044c | leitura integral | Intenção/oráculo observados; target não executado aqui |
| S03 | audit_idempotency.rs, blob 5c903c7c3402d61f45a7455a2d30f7cbee50f386 | leitura integral | Intenção/oráculo observados; target não executado aqui |
| S04/S05 | corelink-meta fake/store, blobs b79a0fd072c374e3b3c0010c46f063408db56130 e 52965189d754b474bf4bba6c7eac2f43ded4eaf1 | leitura dos símbolos usados | Fake/trait; não exercício de adapter persistente |
| S06 | corelink-hash digest.rs | leitura de Digest::from_hex | Fonte apenas |
| S07 | corelink-meta.yml e nightly.yml | leitura de triggers, jobs, matriz e comandos | PR ativo; nightly per-crate dormente; matriz raiz 3600s ativa; logs não coletados |

**Desconhecidos:** resolução Cargo, corpus local, runs CI reais, resultado atual, alcance de consumidores/inversas, owner humano dos contratos e rota independente de review.
**Não alegado:** execução Rust/fuzz, persistência D1, completa cobertura de consumers, aprovação, runtime ou publicação.
**Continuar:** [Blast radius](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
