<!-- corelink-ownership:v1:manifest=tools/dt-reconcile/Cargo.toml -->
# [ownership] corelink-dt-reconcile: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-dt-reconcile / tools/dt-reconcile/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto `tools/dt-reconcile/Cargo.toml` consta em `workspace.members` na baseline `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Isso não prova todos os targets nem o wiring. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Levantamento completo de dependências e consumidores ainda não executado.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file tools/dt-reconcile/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** Conferir o contrato de invocação, formatos de entrada/saída, códigos de saída, efeitos externos, build/publicação e recuperação. Um comando de manutenção não recebe permissão para mutar produção por estar documentado.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-dt-reconcile`; manifesto `tools/dt-reconcile/Cargo.toml`; skill `own-corelink-dt-reconcile`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/dt-reconcile/Cargo.toml#L1-L2); blob Git reportado `1f0730263886e0639bd808ee3a5ccf2d0a0524ca`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/dt-reconcile/Cargo.toml).

- Package e binário corelink-dt-reconcile; entrada explícita src/main.rs.

| Target declarado | Tipo | Path declarado |
|---|---|---|
| `corelink-dt-reconcile` | bin | `src/main.rs` |

Esta tabela contém os targets explicitamente inspecionados, não um inventário automático completo.

**Chaves de dependência inspecionadas:** `corelink-dt-webhook`. Resolução de herança, aliases, features e inversas continua pendente.

### Pontos específicos a investigar

- Distinguir comandos de diagnóstico de injeção/replay/operação externa; nomes de subcomandos não concedem autorização.

**Pendência:** completar fontes de implementação/OKF, consumidores, contratos, target/features, gates e prova comportamental. O manifesto é preparação, não a documentação final.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-dt-reconcile` em `tools/dt-reconcile/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 1 arquivos Rust rastreados, 159 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/dt-reconcile/src/main.rs`.

### Relações e consumidores declarados
- 7 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: nenhum na população Cargo examinada.

### OKF e riscos
- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file tools/dt-reconcile/src/main.rs --full`; ausência de match deve permanecer explícita.
- Risco: Fronteira a conferir: `tools/dt-reconcile/src/main.rs:85` contém `let _dt_api_url = std::env::var("DT_API_URL")`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `tools/dt-reconcile/src/main.rs:87` contém `let _dt_api_key = std::env::var("DT_API_KEY").unwrap_or_default();`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `tools/dt-reconcile/src/main.rs:88` contém `let secret = std::env::var("DT_WEBHOOK_SECRET")`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file tools/dt-reconcile/src/main.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path tools/dt-reconcile/Cargo.toml -p corelink-dt-reconcile --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `docs/knowledge/ops/sre-operations-hub.md` @ `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `tools/dt-reconcile/Cargo.toml` @ `1f0730263886e0639bd808ee3a5ccf2d0a0524ca`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `tools/dt-reconcile/src/main.rs` @ `a9a35d157874e618eae8dd8ba7ef597e8013d1b1`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-dt-reconcile/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-dt-reconcile/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-dt-reconcile/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-dt-reconcile/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
