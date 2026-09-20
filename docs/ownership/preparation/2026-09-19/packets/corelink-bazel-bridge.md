# Preparação: corelink-bazel-bridge

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-bazel-bridge/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-bazel-bridge` em `crates/corelink-bazel-bridge/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 7 arquivos Rust rastreados, 2767 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-bazel-bridge/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-bazel-bridge/src/lib.rs:56` — `pub mod adapter;`; `crates/corelink-bazel-bridge/src/lib.rs:57` — `pub mod digest;`; `crates/corelink-bazel-bridge/src/lib.rs:58` — `pub mod error;`; `crates/corelink-bazel-bridge/src/lib.rs:59` — `pub mod find_missing;`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-bazel-bridge/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-bazel-bridge/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-bazel-bridge/src/lib.rs)

## Relações

10 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

## OKF

- Direto: `docs/knowledge/crates/adapter-hosts.md` — Adapter-host crate cluster (surfaces + KMS)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-bazel-bridge/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-bazel-bridge/src/adapter.rs:294` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-bazel-bridge/src/digest.rs:195` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-bazel-bridge/src/error.rs:132` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-bazel-bridge/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-bazel-bridge/Cargo.toml -p corelink-bazel-bridge --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-bazel-bridge/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-bazel-bridge/Cargo.toml); blob `8b8d4859722d1cfc111d2d290d5250f354d4b7aa`.
- [crates/corelink-bazel-bridge/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-bazel-bridge/src/lib.rs); blob `97d1c683608fa1b8614c0d4ac06f71705413632d`.
- [docs/knowledge/crates/adapter-hosts.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/adapter-hosts.md); blob `2b74eba18fcf4969549a225dc346e9dd8e5768d8`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
