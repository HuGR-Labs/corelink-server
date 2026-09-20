# Preparação: corelink-config-do

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-config-do/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-config-do` em `crates/corelink-config-do/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 1595 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-config-do/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-config-do/src/lib.rs:95` — `pub mod error;`; `crates/corelink-config-do/src/lib.rs:96` — `pub mod hash;`; `crates/corelink-config-do/src/lib.rs:97` — `pub mod metrics;`; `crates/corelink-config-do/src/lib.rs:98` — `pub mod propagation;`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-config-do/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-config-do/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-config-do/src/lib.rs)

## Relações

12 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`.

## OKF

- Direto: `docs/knowledge/crates/container-platform.md` — Container/platform crate cluster
- Direto: `docs/knowledge/storage/d1-config-db.md` — D1 CONFIG_DB
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-config-do/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-config-do/src/lib.rs:93` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-config-do/src/lib.rs:103` contém `pub use error::ConfigError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-config-do/src/lib.rs:104` contém `pub use metrics::{InMemoryMetrics, MetricsObserver, NoopMetrics};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-config-do/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-config-do/Cargo.toml -p corelink-config-do --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-config-do/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-config-do/Cargo.toml); blob `7fc03c93df78a67fb54f48d1d7a1228baf50c67e`.
- [crates/corelink-config-do/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-config-do/src/lib.rs); blob `ca276589c7f24827d764428fdfec86cee81cf8a4`.
- [docs/knowledge/crates/container-platform.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/container-platform.md); blob `e73be0b67d80e00b8d4b1140cec57da6d93dd873`.
- [docs/knowledge/storage/d1-config-db.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/storage/d1-config-db.md); blob `264ffb5648900d3f12e6147e3676df4a3271f62d`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
