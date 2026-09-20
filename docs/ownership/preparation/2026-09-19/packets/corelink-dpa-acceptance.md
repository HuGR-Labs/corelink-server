# Preparação: corelink-dpa-acceptance

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-dpa-acceptance/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-dpa-acceptance` em `crates/corelink-dpa-acceptance/Cargo.toml`; 8 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 16 arquivos Rust rastreados, 2015 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-dpa-acceptance/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-dpa-acceptance/src/lib.rs:69` — `pub mod audit;`; `crates/corelink-dpa-acceptance/src/lib.rs:70` — `pub mod error;`; `crates/corelink-dpa-acceptance/src/lib.rs:71` — `pub mod jwt;`; `crates/corelink-dpa-acceptance/src/lib.rs:72` — `pub mod locale;`

## Targets

8 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-dpa-acceptance/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-dpa-acceptance/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-dpa-acceptance/src/lib.rs)

## Relações

13 declarações de dependência e 3 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-privacy`, `corelink-server`, `e2e-signup-flow`.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-dpa-acceptance/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-dpa-acceptance/src/audit.rs:98` contém `impl InMemoryDpaAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dpa-acceptance/src/audit.rs:121` contém `impl DpaAuditSink for InMemoryDpaAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dpa-acceptance/src/lib.rs:64` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-privacy, corelink-server, e2e-signup-flow. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-dpa-acceptance/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-dpa-acceptance/Cargo.toml -p corelink-dpa-acceptance --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-dpa-acceptance/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-dpa-acceptance/Cargo.toml); blob `773e415aad9021caaee51173a5e8e3a05d32129c`.
- [crates/corelink-dpa-acceptance/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-dpa-acceptance/src/lib.rs); blob `1fd217724a6c64b9f19ab0dc92bf7850605b3362`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
