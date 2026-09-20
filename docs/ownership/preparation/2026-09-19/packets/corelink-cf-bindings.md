# Preparação: corelink-cf-bindings

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-cf-bindings/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-cf-bindings` em `crates/corelink-cf-bindings/Cargo.toml`; 5 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 14 arquivos Rust rastreados, 5840 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-cf-bindings/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-cf-bindings/src/lib.rs:54` — `pub mod cf_d1;`; `crates/corelink-cf-bindings/src/lib.rs:56` — `pub mod cf_do;`; `crates/corelink-cf-bindings/src/lib.rs:58` — `pub mod cf_kv;`; `crates/corelink-cf-bindings/src/lib.rs:60` — `pub mod cf_r2;`

## Targets

5 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-cf-bindings/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-cf-bindings/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-cf-bindings/src/lib.rs)

## Relações

16 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapters-cloud`, `corelink-billing-stripe-materializer`, `corelink-clerk-cf`, `corelink-dsr-statuspage-scheduler`, `corelink-server`.

## OKF

- Direto: `docs/knowledge/crates/container-platform.md` — Container/platform crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-cf-bindings/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-cf-bindings/src/cf_do.rs:25` contém `// re-export at crate root (`pub use crate::durable::*` in`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-cf-bindings/src/cf_r2.rs:33` contém `// at crate root (`pub use crate::r2::*` in `worker/src/lib.rs`) lifts`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-cf-bindings/src/d1_real.rs:306` contém `#[cfg(target_arch = "wasm32")]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapters-cloud, corelink-billing-stripe-materializer, corelink-clerk-cf, corelink-dsr-statuspage-scheduler, corelink-server. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-cf-bindings/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-cf-bindings/Cargo.toml -p corelink-cf-bindings --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-cf-bindings/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-cf-bindings/Cargo.toml); blob `0d870f8b6d658e34d71fd64916473cbc94b7c039`.
- [crates/corelink-cf-bindings/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-cf-bindings/src/lib.rs); blob `c908b3d050663b0d154463bf59f4e785cf1c63a8`.
- [docs/knowledge/crates/container-platform.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/container-platform.md); blob `e73be0b67d80e00b8d4b1140cec57da6d93dd873`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
