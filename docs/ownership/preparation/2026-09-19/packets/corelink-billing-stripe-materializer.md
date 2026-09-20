# Preparação: corelink-billing-stripe-materializer

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-billing-stripe-materializer/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-billing-stripe-materializer` em `crates/corelink-billing-stripe-materializer/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 17 arquivos Rust rastreados, 5317 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing-stripe-materializer/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing-stripe-materializer/src/lib.rs:60` — `pub mod clock;`; `crates/corelink-billing-stripe-materializer/src/lib.rs:69` — `pub use audit::{`; `crates/corelink-billing-stripe-materializer/src/lib.rs:74` — `pub use clock::SystemMatClock;`; `crates/corelink-billing-stripe-materializer/src/lib.rs:76` — `pub use clock::WasmWorkerMatClock;`

<a id="targets"></a>
## Targets

3 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-billing-stripe-materializer/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-billing-stripe-materializer/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-stripe-materializer/src/lib.rs)

<a id="relacoes"></a>
## Relações

17 declarações de dependência e 3 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `corelink-clerk-cf`, `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Direto: `docs/knowledge/launch/money-path.md` — Money-path: checkout + billing ingest
- Direto: `docs/knowledge/security/money-path-review.md` — Money-path security review
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing-stripe-materializer/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: cf-billing-real, default. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-billing-stripe-materializer/src/audit.rs:147` contém `impl InMemoryBillingAuditEmitter {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-stripe-materializer/src/audit.rs:180` contém `impl BillingAuditEmitter for InMemoryBillingAuditEmitter {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-stripe-materializer/src/audit.rs:248` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, corelink-clerk-cf, corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-billing-stripe-materializer/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-billing-stripe-materializer/Cargo.toml -p corelink-billing-stripe-materializer --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-billing-stripe-materializer/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-stripe-materializer/Cargo.toml); blob `c1e2437ed3fec7e4c623b7b5fc360d3b1df534c1`.
- [crates/corelink-billing-stripe-materializer/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing-stripe-materializer/src/lib.rs); blob `6e22db69bae70bd1230645cabfbd5f3006926782`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.
- [docs/knowledge/launch/money-path.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/launch/money-path.md); blob `2d520bca0ddc2945be002ad9559a5a6a579b26d6`.
- [docs/knowledge/security/money-path-review.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/security/money-path-review.md); blob `1674f640958dbb2cbe39f71412a223b4e2942c3e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
