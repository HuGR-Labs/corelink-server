<!-- corelink-ownership:v1:manifest=tools/cli/fuzz/Cargo.toml -->
# [ownership] corelink-cli-fuzz: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-cli-fuzz / tools/cli/fuzz/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto `tools/cli/fuzz/Cargo.toml` foi inspecionado diretamente; declara `[workspace]` próprio e `[package.metadata] cargo-fuzz=true` na baseline `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Isso não prova todos os targets nem o wiring. [Manifesto independente](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/cli/fuzz/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Levantamento completo de dependências e consumidores ainda não executado.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file tools/cli/fuzz/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** `tools/cli/fuzz/Cargo.toml` foi confirmado como package com workspace próprio. Registrar relação com `tools/cli`, corpus/targets, limites de recursos, timeout, reprodução/minimização de caso local e diferenças em relação aos testes da crate-mãe. Não duplicar seu manual.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-cli-fuzz`; manifesto `tools/cli/fuzz/Cargo.toml`; skill `own-corelink-cli-fuzz`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/cli/fuzz/Cargo.toml#L3-L4); blob Git reportado `239da43bfd77a6b9a43b4c4074807dfc1d57ef45`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tools/cli/fuzz/Cargo.toml).

- [workspace] próprio presente.
- [package.metadata] declara cargo-fuzz=true.
- publish=false; edition=2021; license=UNLICENSED.
- Associação ao diretório pai é ownership estrutural, não prova de dependência ou cobertura.

| Target declarado | Tipo | Path declarado |
|---|---|---|
| `cli_input` | bin | `fuzz_targets/cli_input.rs` |
| `config_toml` | bin | `fuzz_targets/config_toml.rs` |
| `json_deserialize` | bin | `fuzz_targets/json_deserialize.rs` |
| `auth_resolution` | bin | `fuzz_targets/auth_resolution.rs` |
| `secret_redaction_check` | bin | `fuzz_targets/secret_redaction_check.rs` |

Esta tabela contém os targets explicitamente inspecionados, não um inventário automático completo.

**Chaves de dependência inspecionadas:** `corelink-cli`, `libfuzzer-sys`, `arbitrary`, `toml`, `serde_json`. Resolução de herança, aliases, features e inversas continua pendente.

**Arestas locais declaradas:** `corelink-cli` via `path=..`.

**Política do harness:** `unsafe_code=forbid`; versão `0.0.0`; `[workspace]` próprio.

### Pontos específicos a investigar

- Workspace independente fora dos nove excludes iniciais. O manifesto usa toml 0.8, enquanto a CLI declara toml 1.1: inspecionar qual parser cada target exercita.
- Verificar corpus, entradas realmente consumidas, timeout/memória, reprodução e minimização local; nenhum fuzz target foi executado nesta rodada.

**Consulta inicial proposta, não executada nesta rodada:**
```sh
cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path tools/cli/fuzz/Cargo.toml
```

**Pendência:** completar fontes de implementação/OKF, consumidores, contratos, target/features, gates e prova comportamental. O manifesto é preparação, não a documentação final.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-cli-fuzz` em `tools/cli/fuzz/Cargo.toml`; 5 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 5 arquivos Rust rastreados, 204 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tools/cli/fuzz/fuzz_targets/auth_resolution.rs`, `tools/cli/fuzz/fuzz_targets/cli_input.rs`, `tools/cli/fuzz/fuzz_targets/config_toml.rs`, `tools/cli/fuzz/fuzz_targets/json_deserialize.rs`, `tools/cli/fuzz/fuzz_targets/secret_redaction_check.rs`.

### Relações e consumidores declarados
- 5 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: nenhum na população Cargo examinada.

### OKF e riscos
- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file tools/cli/fuzz/fuzz_targets/auth_resolution.rs --full`; ausência de match deve permanecer explícita.
- Risco: Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.
- Risco: O harness tem workspace próprio. Testar o alvo original e registrar limites/corpus, sem creditar cobertura da crate-mãe somente pela localização da pasta.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1 --manifest-path tools/cli/fuzz/Cargo.toml` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file tools/cli/fuzz/fuzz_targets/auth_resolution.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path tools/cli/fuzz/Cargo.toml -p corelink-cli-fuzz --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `tools/cli/fuzz/Cargo.toml` @ `239da43bfd77a6b9a43b4c4074807dfc1d57ef45`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `tools/cli/fuzz/fuzz_targets/auth_resolution.rs` @ `eb6d2e2d80e0a62a81f2be6db4e7d04a77fbda44`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `tools/cli/fuzz/fuzz_targets/cli_input.rs` @ `86f04eb50d93d4d450a5c120ec45e5999d6829a4`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `tools/cli/fuzz/fuzz_targets/config_toml.rs` @ `1c4453b2a9fe2e82f5b7a4fa7fef934b62b9d0cd`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-cli-fuzz/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-cli-fuzz/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-cli-fuzz/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-cli-fuzz/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
