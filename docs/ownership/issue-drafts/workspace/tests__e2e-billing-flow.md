<!-- corelink-ownership:v1:manifest=tests/e2e-billing-flow/Cargo.toml -->
# [ownership] e2e-billing-flow: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | e2e-billing-flow / tests/e2e-billing-flow/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto `tests/e2e-billing-flow/Cargo.toml` consta em `workspace.members` na baseline `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Isso não prova todos os targets nem o wiring. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Levantamento completo de dependências e consumidores ainda não executado.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file tests/e2e-billing-flow/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** O manual deve distinguir harness, sistema sob teste e fakes. Identificar fixtures, isolamento, testes locais versus live/ignored, evidência do resultado e cleanup seguro. Não atribuir cobertura de produção a um mock.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `e2e-billing-flow`; manifesto `tests/e2e-billing-flow/Cargo.toml`; skill `own-e2e-billing-flow`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-billing-flow/Cargo.toml#L1-L2); blob Git reportado `1ae56c9eb087110cfe2ead4cf657d0f87cf8fcc4`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-billing-flow/Cargo.toml).

- publish=false; library em src/lib.rs; teste explícito end_to_end.
- O manifesto descreve BillingHarness in-memory; não é evidência de Stripe/D1 real.

| Target declarado | Tipo | Path declarado |
|---|---|---|
| `Nome implícito: confirmar por Cargo` | lib | `src/lib.rs` |
| `end_to_end` | test | `tests/end_to_end.rs` |

Esta tabela contém os targets explicitamente inspecionados, não um inventário automático completo.

**Chaves de dependência inspecionadas:** `corelink-signup`, `corelink-tier-selection`, `corelink-billing-stripe`, `corelink-dsr`, `hex`, `sha2`, `uuid`, `thiserror`. Resolução de herança, aliases, features e inversas continua pendente.

### Pontos específicos a investigar

- Documentar quais implementações e efeitos os doubles realmente exercitam; não chamar esta composição de prova produtiva.

**Pendência:** completar fontes de implementação/OKF, consumidores, contratos, target/features, gates e prova comportamental. O manifesto é preparação, não a documentação final.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `e2e-billing-flow` em `tests/e2e-billing-flow/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 3 arquivos Rust rastreados, 1519 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-billing-flow/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-billing-flow/src/lib.rs:48` — `pub mod harness;`; `tests/e2e-billing-flow/src/lib.rs:50` — `pub use harness::{`

### Relações e consumidores declarados
- 8 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: nenhum na população Cargo examinada.

### OKF e riscos
- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/billing-commerce.md` — Billing/commerce crate cluster
- Contexto herdado de dependência: `docs/knowledge/compliance/dsr-erasure.md` — DSR / right-to-erasure pipeline
- Risco: Fronteira a conferir: `tests/e2e-billing-flow/src/harness.rs:908` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `tests/e2e-billing-flow/src/lib.rs:44` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `tests/e2e-billing-flow/src/lib.rs:50` contém `pub use harness::{`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file tests/e2e-billing-flow/src/lib.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path tests/e2e-billing-flow/Cargo.toml -p e2e-billing-flow --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `docs/knowledge/compliance/dsr-erasure.md` @ `123fa54ff307472e7c438202640276c27fe958cb`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `docs/knowledge/crates/billing-commerce.md` @ `be788f7829b718bfc0709a4e336f1095d6815bdd`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `docs/knowledge/crates/privacy-compliance.md` @ `041debef4182856b0ad013d6aa1b9f9087d9ea56`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `tests/e2e-billing-flow/Cargo.toml` @ `1ae56c9eb087110cfe2ead4cf657d0f87cf8fcc4`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-e2e-billing-flow/SKILL.md`.
- [ ] `docs/ownership/crates/e2e-billing-flow/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/e2e-billing-flow/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/e2e-billing-flow/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
