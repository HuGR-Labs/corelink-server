# Preparação: corelink-stripe-real

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-stripe-real/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-stripe-real` em `crates/corelink-stripe-real/Cargo.toml`; 10 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 22 arquivos Rust rastreados, 6475 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-stripe-real/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-stripe-real/src/lib.rs:74` — `pub mod client;`; `crates/corelink-stripe-real/src/lib.rs:75` — `pub mod clock;`; `crates/corelink-stripe-real/src/lib.rs:76` — `pub mod dlq;`; `crates/corelink-stripe-real/src/lib.rs:77` — `pub mod error;`

<a id="targets"></a>
## Targets

10 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-stripe-real/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-stripe-real/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-stripe-real/src/lib.rs)

<a id="relacoes"></a>
## Relações

18 declarações de dependência e 5 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapters-cloud`, `corelink-billing`, `corelink-billing-stripe-materializer`, `corelink-server`, `e2e-signup-flow`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-pipeline.md` — Billing usage→charge pipeline (emit/reconcile/aggregate/materialize + stripe-real egress)
- Direto: `docs/knowledge/launch/stripe-activation-webhook.md` — Stripe activation webhook — the money-path activation half
- Direto: `docs/knowledge/security/money-path-review.md` — Money-path security review
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-stripe-real/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, live-integration. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-stripe-real/src/client.rs:976` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-stripe-real/src/clock.rs:116` contém `#[cfg(not(target_arch = "wasm32"))]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-stripe-real/src/clock.rs:120` contém `#[cfg(not(target_arch = "wasm32"))]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapters-cloud, corelink-billing, corelink-billing-stripe-materializer, corelink-server, e2e-signup-flow. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-stripe-real/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-stripe-real/Cargo.toml -p corelink-stripe-real --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-stripe-real/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-stripe-real/Cargo.toml); blob `db2c9d4c48a5b3ec4d7c02db9acef19a37273970`.
- [crates/corelink-stripe-real/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-stripe-real/src/lib.rs); blob `c8e468f847caa16e7568b69842cae03dcb0004e8`.
- [docs/knowledge/crates/billing-pipeline.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-pipeline.md); blob `e81604ff79a7b904e705844047b215d9fd7264a4`.
- [docs/knowledge/launch/stripe-activation-webhook.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/launch/stripe-activation-webhook.md); blob `5f4b299716833c6adce3ab86d94df72aac4990b9`.
- [docs/knowledge/security/money-path-review.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/security/money-path-review.md); blob `1674f640958dbb2cbe39f71412a223b4e2942c3e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
