# Preparação: corelink-ac

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-ac/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-ac` em `crates/corelink-ac/Cargo.toml`; 12 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 28 arquivos Rust rastreados, 6844 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-ac/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-ac/src/lib.rs:44` — `pub mod schema;`; `crates/corelink-ac/src/lib.rs:50` — `pub use ac_core::*;`; `crates/corelink-ac/src/lib.rs:57` — `pub mod handler {`; `crates/corelink-ac/src/lib.rs:58` — `pub use corelink_handler_ac::*;`

## Targets

12 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-ac/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-ac/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ac/src/lib.rs)

## Relações

18 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-cas`, `corelink-worker`.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/audit-analytics.md` — Audit/analytics crate cluster
- Contexto herdado de dependência: `docs/knowledge/crates/handler-trait-seam.md` — Handler-trait seam (CAS/AC/customer/erase/admin)
- Contexto herdado de dependência: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-ac/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-ac/src/ac_core.rs:117` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-ac/src/ac_core.rs:127` contém `pub use bounds::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-ac/src/ac_core.rs:131` contém `pub use codec::{decode, encode};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-cas, corelink-worker. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-ac/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-ac/Cargo.toml -p corelink-ac --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-ac/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ac/Cargo.toml); blob `c82ed481e461a5e74b12cccc6304cbbe8ac9c349`.
- [crates/corelink-ac/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ac/src/lib.rs); blob `06b183009a7a81f42073f081020c4d1d6130e07d`.
- [docs/knowledge/crates/audit-analytics.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/audit-analytics.md); blob `864bae2a19336af8486ad5485eaa3764e05db0d9`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.
- [docs/knowledge/crates/handler-trait-seam.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/handler-trait-seam.md); blob `fd51c52a5b57a62c0e9594832e1df4ba052e91a8`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
