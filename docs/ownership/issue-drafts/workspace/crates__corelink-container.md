<!-- corelink-ownership:v1:manifest=crates/corelink-container/Cargo.toml -->
# [ownership] corelink-server: skill, referência, blast radius e manutenção

## Identidade e preparação

| Campo | Valor |
|---|---|
| Package / manifesto | corelink-server / crates/corelink-container/Cargo.toml |
| Baseline de código | cca798ff5bc2df660ecf2570ed243eb9775ff3d0 |
| Contrato compartilhado | **Pendente:** publicar o contrato `1.1-candidate` de `STANDARD.md`, concluir sua revisão e fixar a revisão imutável. O destino proposto é `docs/ownership/STANDARD.md`. |
| Perfil e evidência de capacidade | A medir: S padrão; H só conforme CO-1 §3. Não escolher automaticamente pelo nome. |
| Preparação | DRAFT — nome confirmado na fonte; emissão bloqueada por censo Cargo, preparação semântica, contrato e duplicatas/backlog pendentes. |

## Pacote específico da crate

**Papel e superfícies verificadas:** Manifesto confirma package `corelink-server`, pasta `corelink-container`, `autobins=false`, binaries `corelink-server` e `corelink-gc-sweep-production`. Inspecionadas as primeiras 140 linhas, não todas as dependências. [Fonte de identidade/censo](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/Cargo.toml).
**Targets / features / aliases:** Separar identidade do package e os dois bins. A matriz inclui defaults e flags byok-*-real/cf-r2-real/cf-billing-real; conferir seleções válidas, sem ligar --all-features indiscriminadamente.
**Dependências diretas e consumidores:** Levantamento completo de dependências e consumidores ainda não executado.
**Contexto canônico OKF:** Consultar `scripts/okf_context.py --file crates/corelink-container/Cargo.toml` e arquivos de implementação depois de enumerá-los; confirmar os conceitos em `docs/knowledge/index.md`. A consulta não foi executada neste estudo.
**Riscos que não podem desaparecer na documentação:** Composition root: distinguir trait, construção do adapter, montagem de rota, target do artefato e efeito de runtime. Não generalizar o estado de um binário para todos os deployments.
**Gates existentes e ambiente:** Inventário por `cargo metadata --locked --offline --no-deps --format-version=1`; resolve, testes e gates específicos serão registrados com target/features reais. Nenhum teste CoreLink foi executado por este pacote.
**Lacunas do pacote:** Completar a inspeção individual, os consumidores inversos e relações fora de Cargo. Não substituir o pacote de execução por este rascunho preliminar.

<!-- source-preparation-v1.2:start -->
## Evidência de preparação — revisão 1.2

**Identidade:** `corelink-server`; manifesto `crates/corelink-container/Cargo.toml`; skill `own-corelink-server`.
**Fonte imutável:** [declaração do package](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/Cargo.toml#L1-L2); blob Git reportado `e9feace6365776c6a4141ff59efa70fe1b36e158`.
**Alcance:** identidade confirmada por leitura de fonte. Cargo não foi executado; esta seção não aprova a issue nem certifica runtime.

### Contexto extraído do manifesto

**Fonte desses campos:** [manifesto inspecionado](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-container/Cargo.toml).

- Diretório corelink-container; package corelink-server; autobins=false.
- Bins explícitos corelink-server e corelink-gc-sweep-production.

| Target declarado | Tipo | Path declarado |
|---|---|---|
| `corelink-server` | bin | `src/main.rs` |
| `corelink-gc-sweep-production` | bin | `src/bin/gc_sweep.rs` |

Esta tabela contém os targets explicitamente inspecionados, não um inventário automático completo.

### Pontos específicos a investigar

- Não criar ownership separado por binary; cobrir os dois targets no package.
- Conferir combinações BYOK/target e as duas entradas reais antes de prescrever all-features ou deploy.

**Pendência:** completar fontes de implementação/OKF, consumidores, contratos, target/features, gates e prova comportamental. O manifesto é preparação, não a documentação final.

<!-- source-preparation-v1.2:end -->


## Contexto hidratado — preparação current-main

Fonte de preparação: `origin/main@0389714d`; seed source commit: `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Esta seção é SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED.

### Fatos verificados
- Package `corelink-server` em `crates/corelink-container/Cargo.toml`; 17 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 409 arquivos Rust rastreados, 148602 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-container/src/bin/gc_sweep.rs`, `crates/corelink-container/src/lib.rs`, `crates/corelink-container/src/main.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-container/src/lib.rs:70` — `pub mod byok;`; `crates/corelink-container/src/lib.rs:77` — `pub mod adapter_cache;`; `crates/corelink-container/src/lib.rs:80` — `pub mod adapter_kv;`; `crates/corelink-container/src/lib.rs:84` — `pub mod adapter_oci_kv;`

### Relações e consumidores declarados
- 75 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.  Consumidores declarados: nenhum na população Cargo examinada.

### OKF e riscos
- Direto: `docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md` — ADR-S14-001 — Multi-region Terraform module + per-region KV namespace + DO EU jurisdiction
- Direto: `docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md` — ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)
- Direto: `docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md` — ADR-S14-007 — Erasure attestation: Ed25519 + RFC 8785 JCS + 30d key overlap
- Risco: Investigar ativação e compatibilidade das features declaradas: byok-aws-real, byok-azure-real, byok-gcp-real, byok-vault-real, cf-billing-real, cf-r2-real, default. Não assumir que --all-features é válido.
- Risco: Fronteira a conferir: `crates/corelink-container/src/adapter_cache.rs:487` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Risco: Fronteira a conferir: `crates/corelink-container/src/adapter_kv.rs:193` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.

### Comandos candidatos — não executados
- `cargo metadata --locked --offline --no-deps --format-version=1` — READ_ONLY; NOT_EXECUTED.
- `python3 scripts/okf_context.py --file crates/corelink-container/src/bin/gc_sweep.rs --full` — READ_ONLY; NOT_EXECUTED.
- `cargo tree --locked --offline --manifest-path crates/corelink-container/Cargo.toml -p corelink-server --target x86_64-unknown-linux-gnu --edges normal,build` — READ_ONLY_RESOLUTION; NOT_EXECUTED.

### Evidência de fonte
- `crates/corelink-container/Cargo.toml` @ `e9feace6365776c6a4141ff59efa70fe1b36e158`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-container/src/bin/gc_sweep.rs` @ `47b9bfb144f3254f3ba246f6989e4317b252877d`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-container/src/lib.rs` @ `ec2845586ca59da9120ba16b879ce68823feb849`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
- `crates/corelink-container/src/main.rs` @ `b9971c55030d59152549da904187c753cf0310a4`; source `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

Limites: relações semânticas profundas, seleção de build, runtime, owner e publicação continuam desconhecidos ou bloqueados.

## Dependência comum e critério de emissão

CO-COMMON responde pelo schema, gates e integração de índices/backlog. O pacote
1.1 define relação compartilhada, três direções e certificação por procedimento.
Este rascunho permanece BLOCKED_FOR_PUBLICATION até censo, seed, piloto, freeze
e deduplicação/backlog estarem comprovados. O marcador v1 permanece estável.

## Entregas obrigatórias

- [ ] `.claude/skills/own-corelink-server/SKILL.md`.
- [ ] `docs/ownership/crates/corelink-server/REFERENCE.md`, único e indexado.
- [ ] `docs/ownership/crates/corelink-server/BLAST_RADIUS.md`, único, com todas as relações de fronteira conciliadas.
- [ ] `docs/ownership/crates/corelink-server/MAINTENANCE.md`, único, com procedimentos verificáveis.

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
