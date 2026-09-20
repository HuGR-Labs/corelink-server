# Preparação: corelink-billing-emit

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-billing-emit/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-billing-emit` em `crates/corelink-billing-emit/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 8 arquivos Rust rastreados, 3428 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing-emit/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing-emit/src/lib.rs:138` — `pub mod audit;`; `crates/corelink-billing-emit/src/lib.rs:139` — `pub mod emitter;`; `crates/corelink-billing-emit/src/lib.rs:140` — `pub mod error;`; `crates/corelink-billing-emit/src/lib.rs:141` — `pub mod event;`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-billing-emit/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-billing-emit/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-emit/src/lib.rs)

## Relações

11 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `corelink-billing-aggregator`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-runner-aggregate`, `corelink-server`.

## OKF

- Direto: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing-emit/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-billing-emit/src/audit.rs:136` contém `pub use crate::error::BillingAuditSinkError as BillingAuditEmitError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-emit/src/audit.rs:166` contém `impl InMemoryBillingAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-emit/src/audit.rs:207` contém `impl BillingAuditSink for InMemoryBillingAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, corelink-billing-aggregator, corelink-billing-reconcile, corelink-billing-stripe, corelink-runner-aggregate, corelink-server. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-billing-emit/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-billing-emit/Cargo.toml -p corelink-billing-emit --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-billing-emit/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-emit/Cargo.toml); blob `42cb84a54e4a4cb3d9f1642a63cad0249b9dd106`.
- [crates/corelink-billing-emit/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-emit/src/lib.rs); blob `96c2416565dd3caf7bc646a2dc6ac824ed7c437e`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
