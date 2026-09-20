# Preparação: corelink-adapter-host

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-adapter-host/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-adapter-host` em `crates/corelink-adapter-host/Cargo.toml`; 24 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 88 arquivos Rust rastreados, 21113 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-adapter-host/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-adapter-host/src/lib.rs:62` — `pub mod brew;`; `crates/corelink-adapter-host/src/lib.rs:63` — `pub mod cargo;`; `crates/corelink-adapter-host/src/lib.rs:64` — `pub mod npm;`; `crates/corelink-adapter-host/src/lib.rs:65` — `pub mod oci;`

## Targets

24 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-adapter-host/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-adapter-host/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-adapter-host/src/lib.rs)

## Relações

47 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

## OKF

- Direto: `docs/knowledge/crates/adapter-hosts.md` — Adapter-host crate cluster (surfaces + KMS)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-adapter-host/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-adapter-host/src/brew.rs:61` contém `pub use bridge::{BrewCasBridge, BrewTenantBridge};`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-adapter-host/src/brew.rs:62` contém `pub use config::BrewAdapterConfig;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-adapter-host/src/brew.rs:63` contém `pub use error::BrewAdapterError;`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-adapter-host/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-adapter-host/Cargo.toml -p corelink-adapter-host --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-adapter-host/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-adapter-host/Cargo.toml); blob `805e65fb79a1c00019e8051d7b8694769ae35351`.
- [crates/corelink-adapter-host/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-adapter-host/src/lib.rs); blob `de5f5c3585ffecef3d7dba46c0a6c14605f7251d`.
- [docs/knowledge/crates/adapter-hosts.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/adapter-hosts.md); blob `2b74eba18fcf4969549a225dc346e9dd8e5768d8`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
