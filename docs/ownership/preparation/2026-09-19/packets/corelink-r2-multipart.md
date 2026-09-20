# Preparação: corelink-r2-multipart

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-r2-multipart/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-r2-multipart` em `crates/corelink-r2-multipart/Cargo.toml`; 8 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 16 arquivos Rust rastreados, 3998 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-r2-multipart/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-r2-multipart/src/lib.rs:74` — `pub mod adapter;`; `crates/corelink-r2-multipart/src/lib.rs:75` — `pub mod bounds;`; `crates/corelink-r2-multipart/src/lib.rs:76` — `pub mod concurrency;`; `crates/corelink-r2-multipart/src/lib.rs:77` — `pub mod error;`

## Targets

8 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-r2-multipart/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-r2-multipart/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-r2-multipart/src/lib.rs)

## Relações

11 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-cas`.

## OKF

- Direto: `docs/knowledge/storage/chunk-manifest-buckets.md` — Chunk / manifest multipart buckets
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-r2-multipart/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-r2-multipart/src/bounds.rs:72` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-r2-multipart/src/concurrency.rs:140` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-r2-multipart/src/error.rs:139` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-cas. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-r2-multipart/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-r2-multipart/Cargo.toml -p corelink-r2-multipart --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-r2-multipart/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-r2-multipart/Cargo.toml); blob `3962a394e1dba8ddab89e98cd46e465d55ebf146`.
- [crates/corelink-r2-multipart/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-r2-multipart/src/lib.rs); blob `5a887634284550c847d3a6ee9fce5f9a2fa7549b`.
- [docs/knowledge/storage/chunk-manifest-buckets.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/storage/chunk-manifest-buckets.md); blob `d07aabb3c5c45fbde318440826a73286893f2820`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
