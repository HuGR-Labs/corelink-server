# Preparação: corelink-billing-aggregator

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-billing-aggregator/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-billing-aggregator` em `crates/corelink-billing-aggregator/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 8 arquivos Rust rastreados, 3859 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing-aggregator/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing-aggregator/src/lib.rs:178` — `pub mod aggregator;`; `crates/corelink-billing-aggregator/src/lib.rs:179` — `pub mod audit;`; `crates/corelink-billing-aggregator/src/lib.rs:180` — `pub mod chain;`; `crates/corelink-billing-aggregator/src/lib.rs:181` — `pub mod error;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-billing-aggregator/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-billing-aggregator/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-aggregator/src/lib.rs)

<a id="relacoes"></a>
## Relações

12 declarações de dependência e 4 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-runner-aggregate`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing-aggregator/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-billing-aggregator/src/aggregator.rs:465` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-aggregator/src/audit.rs:130` contém `pub use crate::error::AggregatorAuditSinkError as AggregatorAuditEmitError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-aggregator/src/audit.rs:159` contém `impl InMemoryAggregatorAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, corelink-billing-reconcile, corelink-billing-stripe, corelink-runner-aggregate. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-billing-aggregator/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-billing-aggregator/Cargo.toml -p corelink-billing-aggregator --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-billing-aggregator/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-aggregator/Cargo.toml); blob `2530bce59d9f0966fdead36b7fb82013b6957c56`.
- [crates/corelink-billing-aggregator/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-aggregator/src/lib.rs); blob `8e6067efbdfb36deb2618714a3af5f60aaee2c0c`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
