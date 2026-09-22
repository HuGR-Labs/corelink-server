# Preparação: corelink-dual-approval

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-dual-approval/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-dual-approval` em `crates/corelink-dual-approval/Cargo.toml`; 8 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 15 arquivos Rust rastreados, 3495 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-dual-approval/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-dual-approval/src/lib.rs:120` — `pub mod audit;`; `crates/corelink-dual-approval/src/lib.rs:121` — `pub mod collusion;`; `crates/corelink-dual-approval/src/lib.rs:122` — `pub mod error;`; `crates/corelink-dual-approval/src/lib.rs:123` — `pub mod gate;`

<a id="targets"></a>
## Targets

8 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-dual-approval/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-dual-approval/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dual-approval/src/lib.rs)

<a id="relacoes"></a>
## Relações

13 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-dual-approval/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-dual-approval/src/audit.rs:134` contém `impl InMemoryAdminOpAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dual-approval/src/audit.rs:143` contém `impl Default for InMemoryAdminOpAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dual-approval/src/audit.rs:149` contém `impl AdminOpAuditSink for InMemoryAdminOpAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-dual-approval/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-dual-approval/Cargo.toml -p corelink-dual-approval --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-dual-approval/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dual-approval/Cargo.toml); blob `e5768c57539ac0003001fdc5e1188f183f7f6326`.
- [crates/corelink-dual-approval/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dual-approval/src/lib.rs); blob `7d544552aa01f84eda9b0134b03a0ce7e0ed4ffa`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
