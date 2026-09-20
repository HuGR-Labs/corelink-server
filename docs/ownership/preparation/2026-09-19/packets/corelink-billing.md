# Preparação: corelink-billing

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-billing/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-billing` em `crates/corelink-billing/Cargo.toml`; 10 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 58 arquivos Rust rastreados, 18645 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing/src/lib.rs:143` — `pub mod abuse;`; `crates/corelink-billing/src/lib.rs:144` — `pub mod aggregator;`; `crates/corelink-billing/src/lib.rs:145` — `pub mod emit;`; `crates/corelink-billing/src/lib.rs:146` — `pub mod quota;`

<a id="targets"></a>
## Targets

10 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-billing/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-billing/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing/src/lib.rs)

<a id="relacoes"></a>
## Relações

18 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-commerce.md` — Billing/commerce crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-billing/src/abuse.rs:189` contém `pub use audit::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing/src/abuse.rs:193` contém `pub use config::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing/src/abuse.rs:198` contém `pub use error::AbuseError;`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-billing/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-billing/Cargo.toml -p corelink-billing --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-billing/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing/Cargo.toml); blob `225860c6b6d03b7ad7bd16e11428f2b48599ee12`.
- [crates/corelink-billing/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-billing/src/lib.rs); blob `1cb8f80ba9bdaa593f3780717e371ed68f346164`.
- [docs/knowledge/crates/billing-commerce.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/billing-commerce.md); blob `be788f7829b718bfc0709a4e336f1095d6815bdd`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
