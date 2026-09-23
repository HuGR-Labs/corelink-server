<!-- corelink-ownership:v1:manifest=crates/corelink-billing-aggregator/Cargo.toml -->
# [ownership] corelink-billing-aggregator: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-billing-aggregator / crates/corelink-billing-aggregator/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto `crates/corelink-billing-aggregator/Cargo.toml` consta em `workspace.members` na baseline `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Isso não prova todos os targets nem o wiring. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Levantamento completo de dependências e consumidores ainda não executado.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file crates/corelink-billing-aggregator/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** Identificar os contratos e os riscos específicos após leitura da implementação; não inferir comportamento a partir do nome da pasta.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-billing-aggregator`; manifesto `crates/corelink-billing-aggregator/Cargo.toml`; skill `own-corelink-billing-aggregator`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-aggregator/Cargo.toml#L1-L2); blob Git reportado `2530bce59d9f0966fdead36b7fb82013b6957c56`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-aggregator/Cargo.toml).

- Manifesto declara corelink-billing-emit em dependencies e corelink-analytics em dev-dependencies.
- Teste explícito prop_billing_aggregator; a descrição do manifesto separa orquestrador in-memory da integração produtiva adiada.

| Target declarado | Tipo | Path declarado |
|---|---|---|
| `prop_billing_aggregator` | test | `tests/prop_billing_aggregator.rs` |

Esta tabela contém os targets explicitamente inspecionados, não um inventário automático completo.

**Chaves de dependência inspecionadas:** `thiserror`, `serde`, `serde_json`, `serde_jcs`, `blake3`, `hex`, `uuid`, `corelink-billing-emit`. Resolução de herança, aliases, features e inversas continua pendente.

### Pontos específicos a investigar

- Não transformar a aresta dev de analytics em chamada produtiva.
- Reexports de billing não demonstram execução por cron. Conferir os pontos reais de construção e agendamento.

**Pendência:** completar fontes de implementação/OKF, consumidores, contratos, target/features, gates e prova comportamental. O manifesto é preparação, não a documentação final.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-billing-aggregator` em `crates/corelink-billing-aggregator/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 8 arquivos Rust rastreados, 3859 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing-aggregator/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing-aggregator/src/lib.rs:178` — `pub mod aggregator;`; `crates/corelink-billing-aggregator/src/lib.rs:179` — `pub mod audit;`; `crates/corelink-billing-aggregator/src/lib.rs:180` — `pub mod chain;`; `crates/corelink-billing-aggregator/src/lib.rs:181` — `pub mod error;`

### Relações e consumidores declarados
- 12 declarações de dependência e 4 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: `corelink-billing`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-runner-aggregate`.

### OKF e riscos
- Direto: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing-aggregator/src/lib.rs --full`; ausência de match deve permanecer explícita.
- Risco: Fronteira a conferir: `crates/corelink-billing-aggregator/src/aggregator.rs:465` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-billing-aggregator/src/audit.rs:130` contém `pub use crate::error::AggregatorAuditSinkError as AggregatorAuditEmitError;`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-billing-aggregator/src/audit.rs:159` contém `impl InMemoryAggregatorAuditSink {`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file crates/corelink-billing-aggregator/src/lib.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path crates/corelink-billing-aggregator/Cargo.toml -p corelink-billing-aggregator --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `crates/corelink-billing-aggregator/Cargo.toml` @ `2530bce59d9f0966fdead36b7fb82013b6957c56`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-billing-aggregator/src/lib.rs` @ `8e6067efbdfb36deb2618714a3af5f60aaee2c0c`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `docs/knowledge/crates/billing-pipeline.md` @ `e81604ff79a7b904e705844047b215d9fd7264a4`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-billing-aggregator/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-billing-aggregator/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-billing-aggregator/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-billing-aggregator/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
