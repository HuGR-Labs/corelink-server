# Preparação: corelink-core

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-core/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-core` em `crates/corelink-core/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 780 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-core/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-core/src/lib.rs:58` — `pub mod errors;`; `crates/corelink-core/src/lib.rs:59` — `pub mod time;`; `crates/corelink-core/src/lib.rs:60` — `pub mod types;`; `crates/corelink-core/src/lib.rs:64` — `pub use errors::{CoreError, DigestParseError};`

## Targets

1 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-core/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-core/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-core/src/lib.rs)

## Relações

6 declarações de dependência e 5 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ac`, `corelink-adapter-host`, `corelink-cas`, `corelink-server`.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-core/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-core/src/errors.rs:14` contém `pub use digest_parse::DigestParseError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-core/src/errors.rs:54` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-core/src/lib.rs:37` contém `//! via `pub use corelink_core::*` and drop the scattered duplicates.`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ac, corelink-adapter-host, corelink-cas, corelink-server. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-core/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-core/Cargo.toml -p corelink-core --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-core/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-core/Cargo.toml); blob `a76458267dc5ed5e13ded2deb906a2126ba57377`.
- [crates/corelink-core/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-core/src/lib.rs); blob `b4e75d718ae2d7ea00798c018a16dacb2ea55097`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
