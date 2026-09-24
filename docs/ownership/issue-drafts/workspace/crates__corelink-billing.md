<!-- corelink-ownership:v1:manifest=crates/corelink-billing/Cargo.toml -->
# [ownership] corelink-billing: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-billing / crates/corelink-billing/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Package `corelink-billing` confirmado no manifesto. Híbrido com cinco antigas crates absorvidas (abuse, replay e quota/core/cas/fsm) e namespaces de reexport. `src/aggregator.rs` contém `pub use corelink_billing_aggregator::*`. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Onze internas normais declaradas: `corelink-billing-aggregator`, `corelink-billing-emit`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-billing-stripe-materializer`, `corelink-stripe-real`, `corelink-billing-stripe-traits`, `corelink-tier-selection`, `corelink-rate-headers`, `corelink-ratelimit`, `corelink-eviction`. Externas: thiserror, serde, serde_json, uuid. Dev: proptest, rand, rand_chacha. Inversas e alcance por target/features ainda pendentes.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file crates/corelink-billing/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** Não tratar façade como dona de toda a cobrança. Separar contratos reexportados de implementação própria; rastrear invariantes por quota/replay e consumidores de Stripe sem inventar ativação produtiva.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-billing`; manifesto `crates/corelink-billing/Cargo.toml`; skill `own-corelink-billing`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing/Cargo.toml#L1-L2); blob Git reportado `225860c6b6d03b7ad7bd16e11428f2b48599ee12`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing/Cargo.toml).

- Manifesto declara dependências para reexports e dependências dos módulos absorvidos.
- Leitura anterior de src/lib.rs documenta cinco absorções; aggregator.rs contém pub use corelink_billing_aggregator::*.

**Chaves de dependência inspecionadas:** `corelink-billing-aggregator`, `corelink-billing-emit`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-billing-stripe-materializer`, `corelink-stripe-real`, `corelink-billing-stripe-traits`, `corelink-tier-selection`, `corelink-rate-headers`, `corelink-ratelimit`, `thiserror`, `serde`, `serde_json`, `uuid`, `corelink-eviction`. Resolução de herança, aliases, features e inversas continua pendente.

### Pontos específicos a investigar

- Tratar a crate como híbrida: implementação própria e reexports têm owners diferentes.
- As chaves de dependência não certificam seleção por target/features nem agendamento da agregação.

**Pendência:** completar fontes de implementação/OKF, consumidores, contratos, target/features, gates e prova comportamental. O manifesto é preparação, não a documentação final.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-billing` em `crates/corelink-billing/Cargo.toml`; 10 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 58 arquivos Rust rastreados, 18645 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing/src/lib.rs:143` — `pub mod abuse;`; `crates/corelink-billing/src/lib.rs:144` — `pub mod aggregator;`; `crates/corelink-billing/src/lib.rs:145` — `pub mod emit;`; `crates/corelink-billing/src/lib.rs:146` — `pub mod quota;`

### Relações e consumidores declarados
- 18 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: `corelink-server`.

### OKF e riscos
- Direto: `docs/knowledge/crates/billing-commerce.md` — Billing/commerce crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing/src/lib.rs --full`; ausência de match deve permanecer explícita.
- Risco: Fronteira a conferir: `crates/corelink-billing/src/abuse.rs:189` contém `pub use audit::{`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-billing/src/abuse.rs:193` contém `pub use config::{`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-billing/src/abuse.rs:198` contém `pub use error::AbuseError;`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file crates/corelink-billing/src/lib.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path crates/corelink-billing/Cargo.toml -p corelink-billing --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `crates/corelink-billing/Cargo.toml` @ `225860c6b6d03b7ad7bd16e11428f2b48599ee12`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-billing/src/lib.rs` @ `1cb8f80ba9bdaa593f3780717e371ed68f346164`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `docs/knowledge/crates/billing-commerce.md` @ `be788f7829b718bfc0709a4e336f1095d6815bdd`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-billing/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-billing/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-billing/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-billing/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
