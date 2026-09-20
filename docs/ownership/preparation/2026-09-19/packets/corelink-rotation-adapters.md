# Preparação: corelink-rotation-adapters

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-rotation-adapters/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-rotation-adapters` em `crates/corelink-rotation-adapters/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 11 arquivos Rust rastreados, 2619 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-rotation-adapters/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-rotation-adapters/src/lib.rs:101` — `pub mod adapter;`; `crates/corelink-rotation-adapters/src/lib.rs:102` — `pub mod admin_signing;`; `crates/corelink-rotation-adapters/src/lib.rs:103` — `pub mod audit_chain;`; `crates/corelink-rotation-adapters/src/lib.rs:104` — `pub mod byok;`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-rotation-adapters/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-rotation-adapters/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-rotation-adapters/src/lib.rs)

## Relações

5 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`.

## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-rotation-adapters/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-rotation-adapters/src/erasure_attestation.rs:298` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-rotation-adapters/src/lib.rs:97` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-rotation-adapters/src/lib.rs:111` contém `pub use adapter::{is_valid_read_state, is_valid_write_state, RotationAdapter};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-rotation-adapters/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-rotation-adapters/Cargo.toml -p corelink-rotation-adapters --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-rotation-adapters/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-rotation-adapters/Cargo.toml); blob `af7ef105a3038dadc141885fa1ea01b5ea7e42a1`.
- [crates/corelink-rotation-adapters/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-rotation-adapters/src/lib.rs); blob `e836fa642302d299835a9c08b4b0d299703f5f9f`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
