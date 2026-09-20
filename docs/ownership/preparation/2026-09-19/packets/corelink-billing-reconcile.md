# Preparação: corelink-billing-reconcile

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-billing-reconcile/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-billing-reconcile` em `crates/corelink-billing-reconcile/Cargo.toml`; 4 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 12 arquivos Rust rastreados, 4630 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs`, `crates/corelink-billing-reconcile/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing-reconcile/src/lib.rs:196` — `pub mod audit;`; `crates/corelink-billing-reconcile/src/lib.rs:197` — `pub mod drift;`; `crates/corelink-billing-reconcile/src/lib.rs:198` — `pub mod error;`; `crates/corelink-billing-reconcile/src/lib.rs:199` — `pub mod event;`

<a id="targets"></a>
## Targets

4 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-billing-reconcile/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs)
- [`crates/corelink-billing-reconcile/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-reconcile/src/lib.rs)

<a id="relacoes"></a>
## Relações

10 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-billing-reconcile/src/audit.rs:164` contém `pub use crate::error::ReconcileAuditSinkError as ReconcileAuditEmitError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-reconcile/src/audit.rs:193` contém `impl InMemoryReconcileAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-reconcile/src/audit.rs:234` contém `impl ReconcileAuditSink for InMemoryReconcileAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-billing-reconcile/Cargo.toml -p corelink-billing-reconcile --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-billing-reconcile/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-reconcile/Cargo.toml); blob `ee72831ffb989af3283d115bb6b8a3f9694cf443`.
- [crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-reconcile/src/bin/billing-reconcile-run.rs); blob `064e3ed3839207bdf358f29e2fec1cfc31a17d45`.
- [crates/corelink-billing-reconcile/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-reconcile/src/lib.rs); blob `077b8c7087dff2b3c918575db43cbc515435fab0`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
