# Preparação: corelink-runner-aggregate

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-runner-aggregate/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-runner-aggregate` em `crates/corelink-runner-aggregate/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 3 arquivos Rust rastreados, 834 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs`, `crates/corelink-runner-aggregate/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-runner-aggregate/src/lib.rs:66` — `pub enum RunnerAggregateError {`; `crates/corelink-runner-aggregate/src/lib.rs:93` — `pub struct StagedRunnerEvent {`; `crates/corelink-runner-aggregate/src/lib.rs:111` — `pub struct PriorChainHead {`; `crates/corelink-runner-aggregate/src/lib.rs:120` — `pub struct RunnerAggregateInput {`

## Targets

3 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-runner-aggregate/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs)
- [`crates/corelink-runner-aggregate/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-aggregate/src/lib.rs)

## Relações

8 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-runner-aggregate/src/lib.rs:41` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-runner-aggregate/src/lib.rs:404` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-runner-aggregate/tests/runner_aggregate_run_bin.rs:28` contém `const BIN: &str = env!("CARGO_BIN_EXE_runner-aggregate-run");`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-runner-aggregate/Cargo.toml -p corelink-runner-aggregate --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-runner-aggregate/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-aggregate/Cargo.toml); blob `e05439cc7906b7ca76a64f6d62bfd39306ec6e1a`.
- [crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-aggregate/src/bin/runner-aggregate-run.rs); blob `77af291ba6e4694e68c102f1fa7ab967caa9c012`.
- [crates/corelink-runner-aggregate/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runner-aggregate/src/lib.rs); blob `e2a0fb502a2f68129dc2e4c52cd6a9bfb2c9be62`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
