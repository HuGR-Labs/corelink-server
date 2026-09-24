<!-- corelink-ownership:v1:manifest=crates/corelink-privacy-pseudonymize/Cargo.toml -->
# [ownership] corelink-privacy-pseudonymize: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-privacy-pseudonymize / crates/corelink-privacy-pseudonymize/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto `crates/corelink-privacy-pseudonymize/Cargo.toml` consta em `workspace.members` na baseline `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Isso não prova todos os targets nem o wiring. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/Cargo.toml).
**Targets / features / aliases:** Nome confirmado no manifesto. Completar aliases, todos os targets e features com Cargo e fonte; não confundir nome de package com binary/lib.
**Dependências diretas e consumidores:** Levantamento completo de dependências e consumidores ainda não executado.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file crates/corelink-privacy-pseudonymize/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** Identificar os contratos e os riscos específicos após leitura da implementação; não inferir comportamento a partir do nome da pasta.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-privacy-pseudonymize`; manifesto `crates/corelink-privacy-pseudonymize/Cargo.toml`; skill `own-corelink-privacy-pseudonymize`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-privacy-pseudonymize/Cargo.toml#L1-L2); blob Git reportado `c5b8034c7b23b2be4553eeab0e1371f5efa7f895`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

**Pendência específica de preparação:** apenas a identidade foi verificada nesta unidade. O pacote semântico (fontes/OKF, targets, dependências e consumidores, riscos e gates) ainda precisa ser preenchido.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-privacy-pseudonymize` em `crates/corelink-privacy-pseudonymize/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 521 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-privacy-pseudonymize/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-privacy-pseudonymize/src/lib.rs:109` — `pub struct PseudonymizationMarker {`; `crates/corelink-privacy-pseudonymize/src/lib.rs:122` — `pub fn from_hash(hash: PseudonymHash) -> Self {`; `crates/corelink-privacy-pseudonymize/src/lib.rs:134` — `pub struct PseudonymHash([u8; 32]);`; `crates/corelink-privacy-pseudonymize/src/lib.rs:151` — `pub fn to_hex(&self) -> String {`

### Relações e consumidores declarados
- 7 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: `corelink-privacy`, `corelink-privacy-erasure-worker`.

### OKF e riscos
- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-privacy-pseudonymize/src/lib.rs --full`; ausência de match deve permanecer explícita.
- Risco: Fronteira a conferir: `crates/corelink-privacy-pseudonymize/src/lib.rs:75` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-privacy-pseudonymize/src/lib.rs:220` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-privacy-pseudonymize/tests/pseudonymization_invariants.rs:44` contém `std::env::var("PROPTEST_CASES")`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file crates/corelink-privacy-pseudonymize/src/lib.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path crates/corelink-privacy-pseudonymize/Cargo.toml -p corelink-privacy-pseudonymize --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `crates/corelink-privacy-pseudonymize/Cargo.toml` @ `c5b8034c7b23b2be4553eeab0e1371f5efa7f895`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-privacy-pseudonymize/src/lib.rs` @ `ad8c40cd623c4e145a7d5ad17c439273537fe72d`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-privacy-pseudonymize/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-privacy-pseudonymize/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-privacy-pseudonymize/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-privacy-pseudonymize/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
