# Preparação: e2e-billing-flow

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-billing-flow/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `e2e-billing-flow` em `tests/e2e-billing-flow/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 3 arquivos Rust rastreados, 1519 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-billing-flow/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-billing-flow/src/lib.rs:48` — `pub mod harness;`; `tests/e2e-billing-flow/src/lib.rs:50` — `pub use harness::{`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `tests/e2e-billing-flow/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-billing-flow/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-billing-flow/src/lib.rs)

## Relações

8 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/billing-commerce.md` — Billing/commerce crate cluster
- Contexto herdado de dependência: `docs/knowledge/compliance/dsr-erasure.md` — DSR / right-to-erasure pipeline
- Contexto herdado de dependência: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-billing-flow/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `tests/e2e-billing-flow/src/harness.rs:908` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-billing-flow/src/lib.rs:44` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-billing-flow/src/lib.rs:50` contém `pub use harness::{`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-billing-flow/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-billing-flow/Cargo.toml -p e2e-billing-flow --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [docs/knowledge/compliance/dsr-erasure.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/compliance/dsr-erasure.md); blob `123fa54ff307472e7c438202640276c27fe958cb`.
- [docs/knowledge/crates/billing-commerce.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-commerce.md); blob `be788f7829b718bfc0709a4e336f1095d6815bdd`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.
- [tests/e2e-billing-flow/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-billing-flow/Cargo.toml); blob `1ae56c9eb087110cfe2ead4cf657d0f87cf8fcc4`.
- [tests/e2e-billing-flow/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-billing-flow/src/lib.rs); blob `63aef8d0a99b17ba39fb007b738c79d58027ffa3`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
