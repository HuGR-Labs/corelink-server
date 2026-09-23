<!-- corelink-ownership:v1:manifest=crates/corelink-hash/Cargo.toml -->
# [ownership] corelink-hash: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-hash / crates/corelink-hash/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto confirma package `corelink-hash`, publish=true, licença MIT OR Apache-2.0 e benches blake3_bench/blake3. Defaults proprietários do workspace não podem sobrescrever esses fatos na documentação. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-hash/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Normais declaradas: blake3, bytes, hex, subtle, thiserror. Dev: proptest, criterion, tokio, thiserror. Consumidores selecionados e invariantes da implementação foram inspecionados no piloto 1.3; censo inverso completo e validação por execução continuam pendentes.
**Contexto canônico OKF:** `docs/knowledge/crates/cas-ac-core.md` foi lido na baseline, incluindo suas fronteiras entre REAPI e native CAS. A consulta `okf_context.py` continua não executada; o piloto liga ao conceito e distingue suas afirmações do código verificado.
**Riscos que não podem desaparecer na documentação:** Documentar contratos de digest e compatibilidade de consumidores/publicação; conferir os símbolos e testes reais, sem extrapolar a descrição do package para prova de comportamento.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-hash`; manifesto `crates/corelink-hash/Cargo.toml`; skill `own-corelink-hash`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-hash/Cargo.toml#L1-L2); blob Git reportado `0b836118960c8f21c50473dc0e37461fd5b47bd7`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-hash/Cargo.toml).

- Override local: publish=true e license=MIT OR Apache-2.0.
- Dois benches explícitos: blake3_bench e blake3, ambos harness=false; paths não aparecem no manifesto.

| Target declarado | Tipo | Path declarado |
|---|---|---|
| `blake3_bench` | bench | `Path implícito: confirmar por Cargo` |
| `blake3` | bench | `Path implícito: confirmar por Cargo` |

Esta tabela contém os targets explicitamente inspecionados, não um inventário automático completo.

**Chaves de dependência inspecionadas:** `blake3`, `bytes`, `hex`, `subtle`, `thiserror`. Resolução de herança, aliases, features e inversas continua pendente.

### Pontos específicos a investigar

- Não copiar política de publicação/licença do workspace sobre o override local.
- Não usar benchmarks como prova dos contratos de parsing, digest ou verificação.

**Atualização 1.3:** implementação local e conceito OKF foram lidos; quatro documentos candidatos estão abaixo. Permanecem censo de consumidores/aliases, seleção resolvida, execução e cold review. O manifesto não é prova comportamental.

<!-- source-preparation-v1.2:end -->

## Piloto de autoria — revisão 1.3

Quatro documentos candidatos preenchidos, sem aprovação independente:

| Artefato local | Conteúdo desta revisão | Limite |
|---|---|---|
| [Skill](../../pilot-overlay/.claude/skills/own-corelink-hash/SKILL.md) | Gatilhos, fronteiras, decisões e roteamento | Não concede autoridade; cold review pendente |
| [Referência](../../pilot-overlay/docs/ownership/crates/corelink-hash/REFERENCE.md) | Cinco arquivos src lidos; 16 fichas API, cinco invariantes e um fluxo | Não certifica wiring global |
| [Blast radius](../../pilot-overlay/docs/ownership/crates/corelink-hash/BLAST_RADIUS.md) | Dezenove fichas; reexports, writer, chaves, envelope e limite Bazel | População de relações ainda incompleta |
| [Manutenção](../../pilot-overlay/docs/ownership/crates/corelink-hash/MAINTENANCE.md) | Seis procedimentos com comandos/condições de parada | Rust não executado; recuperação ainda precisa de fixtures reais |

**Fatos que exigem atenção:** parsing prova formato, não conteúdo; o construtor não aplica
limite de tamanho; `as_bytes` é usado pelo container em persistência apesar de doc(hidden);
`ScopedR2Writer` mapeia Fresh e Duplicate para `Ok(())`. Não remover nem ampliar garantias
sem reconciliar esses consumidores. As fontes imutáveis estão nos documentos e no
[registro de estudo](../../evidence/revision-1.3/source-study.json).

**Ainda bloqueado:** o piloto não encerra G0. Comandos não executados, peers não
reconciliados e relações ainda não levantadas impedem aprovação dos artefatos finais.
A revisão editorial da autoria não satisfaz o cold review exigido nesta issue.


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-hash` em `crates/corelink-hash/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 11 arquivos Rust rastreados, 1227 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-hash/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-hash/src/lib.rs:56` — `pub use digest::{Digest, DIGEST_LEN};`; `crates/corelink-hash/src/lib.rs:57` — `pub use error::{HashMismatch, ParseError, COR_CAS_DIGEST_MISMATCH};`; `crates/corelink-hash/src/lib.rs:58` — `pub use store::BlobStoreWrite;`; `crates/corelink-hash/src/lib.rs:59` — `pub use verified_body::VerifiedBody;`

### Relações e consumidores declarados
- 9 declarações de dependência e 13 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: `corelink-ac`, `corelink-bazel-bridge`, `corelink-cas`, `corelink-client-verify`, `corelink-crypto`, `corelink-hash-fuzz`, `corelink-meta`, `corelink-meta-fuzz`, `corelink-reapi`, `corelink-server`, `corelink-worker`, `corelink-worker-fuzz`, `e2e-signup-flow`.

### OKF e riscos
- Direto: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-hash/src/lib.rs --full`; ausência de match deve permanecer explícita.
- Risco: Fronteira a conferir: `crates/corelink-hash/src/lib.rs:49` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-hash/src/lib.rs:56` contém `pub use digest::{Digest, DIGEST_LEN};`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-hash/src/lib.rs:57` contém `pub use error::{HashMismatch, ParseError, COR_CAS_DIGEST_MISMATCH};`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file crates/corelink-hash/src/lib.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path crates/corelink-hash/Cargo.toml -p corelink-hash --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `crates/corelink-hash/Cargo.toml` @ `bb0642df8d4a80b95f48eefd8d4339f851a4c8bd`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-hash/src/lib.rs` @ `403f7e2e4ef8c7bb30a47e7d7eee3ce94a785730`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `docs/knowledge/crates/cas-ac-core.md` @ `01b501172f40115174fe6c88d8942af5f3590b51`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-hash/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-hash/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-hash/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-hash/MAINTENANCE.md`, único, com procedimentos verificáveis.

## Success criteria

Um novo owner encontra contratos e impactos a partir da skill em até três cliques,
seleciona e executa um procedimento local aplicável sem o relato do autor, e distingue
implementação, wiring e runtime observado.

## Completeness criteria

Conciliar packages/targets, módulos próprios/absorvidos/reexports, APIs/invariantes,
normal/build/dev dependencies, consumidores, dados/configuração/runtime/build e testes.
Enumerar relações diretas e propagação transitiva material; justificar ausências.
Não considerar `cargo metadata --no-deps` um grafo resolvido completo.

## Quality standards

Usar CO-1 e seus templates, evidências por revisão e índices/IDs estáveis. Os tetos
simultâneos de linhas/palavras/KiB são: skill 180/1.200/12; referência S400/3.000/36
ou H700/5.500/64; blast S900/7.000/80 ou H1.800/14.000/160; manual S400/3.000/36
ou H700/5.500/64. Ficha REL: até 120 palavras/14 linhas. Não preencher até o teto.
Overflow bloqueia aprovação; não truncar, fragmentar nem transferir conteúdo essencial
para anexos. Reutilizar a wiki OKF e as skills transversais.

## Cold review — quatro aprovações independentes da autoria

- [ ] Skill aprovada no hash final, com gatilhos positivos/negativos e limites de autoridade.
- [ ] Referência aprovada no hash final, com contratos e invariantes conferidos no código.
- [ ] Blast radius aprovado no hash final, após recenso independente e busca fora de Cargo.
- [ ] Manual aprovado no hash final, com execução local aplicável, falha/parada e recuperação e estado por PROC.

Revisor em contexto novo, sem histórico de elaboração. Guardar vereditos, fontes,
comandos e resultados; corrigir achados e repetir a revisão afetada. Autorrevisão não conta.

## Definition of done

Quatro arquivos integrados via PR e gates existentes, quatro vereditos válidos no conjunto
final de fontes/artefatos, nenhuma lacuna obrigatória, navegação e limites validados,
registro de evidência completo e índices/backlog reconciliados. Nenhuma pendência
criada pela execução fica sem responsável. Arquivo escrito ou CI genérico verde não fecha a issue.

## Invariants

Sem alteração funcional disfarçada de documentação; sem produção/segredos/gastos implícitos;
sem assumir pasta=package, façade=implementação ou compilação=runtime; sem duplicar
políticas do OKF; sem omitir relações para caber; sem aprovação do próprio autor.

## Antes da emissão

Não certificada. O BACKLOG.md não foi recuperado integralmente. Conferir estado aberto/fechado, aliases e chave por manifesto; registrar a decisão de reaproveitamento ou criação.
Não publicar sem identidade Cargo conciliada, contrato vinculado e decisão sobre duplicatas.
