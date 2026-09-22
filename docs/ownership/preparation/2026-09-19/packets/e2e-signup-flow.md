# Preparação: e2e-signup-flow

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-signup-flow/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-signup-flow` em `tests/e2e-signup-flow/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 1413 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-signup-flow/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-signup-flow/src/lib.rs:40` — `pub mod helpers;`; `tests/e2e-signup-flow/src/lib.rs:41` — `pub mod r2;`; `tests/e2e-signup-flow/src/lib.rs:43` — `pub use helpers::{`; `tests/e2e-signup-flow/src/lib.rs:47` — `pub use r2::{InMemoryR2Client, R2Error, R2StatReport};`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-signup-flow/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-signup-flow/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-signup-flow/src/lib.rs)

<a id="relacoes"></a>
## Relações

14 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Contexto herdado de dependência: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Contexto herdado de dependência: `docs/knowledge/launch/stripe-activation-webhook.md` — Stripe activation webhook — the money-path activation half
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-signup-flow/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-signup-flow/src/helpers.rs:498` contém `impl TierAuditSnapshotExt for InMemoryTierSelectionAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-signup-flow/src/lib.rs:36` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-signup-flow/src/lib.rs:43` contém `pub use helpers::{`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-signup-flow/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-signup-flow/Cargo.toml -p e2e-signup-flow --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.
- [docs/knowledge/launch/stripe-activation-webhook.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/launch/stripe-activation-webhook.md); blob `5f4b299716833c6adce3ab86d94df72aac4990b9`.
- [tests/e2e-signup-flow/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-signup-flow/Cargo.toml); blob `67c5e30446e8ac0274a33498d13a0c5d5d106c10`.
- [tests/e2e-signup-flow/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-signup-flow/src/lib.rs); blob `e87fc46f91ad21ff6e8d0935ef5445136bcebd6b`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
