# Preparação: corelink-billing-stripe

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-billing-stripe/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-billing-stripe` em `crates/corelink-billing-stripe/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 11 arquivos Rust rastreados, 4121 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-billing-stripe/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-billing-stripe/src/lib.rs:166` — `pub mod adapter;`; `crates/corelink-billing-stripe/src/lib.rs:167` — `pub mod audit;`; `crates/corelink-billing-stripe/src/lib.rs:168` — `pub mod error;`; `crates/corelink-billing-stripe/src/lib.rs:169` — `pub mod event;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-billing-stripe/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-billing-stripe/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-billing-stripe/src/lib.rs)

<a id="relacoes"></a>
## Relações

16 declarações de dependência e 3 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `corelink-billing-reconcile`, `e2e-billing-flow`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-commerce.md` — Billing/commerce crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-billing-stripe/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-billing-stripe/src/adapter.rs:270` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-stripe/src/audit.rs:135` contém `pub use crate::error::StripeAuditSinkError as StripeAuditEmitError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-billing-stripe/src/audit.rs:163` contém `impl InMemoryStripeAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, corelink-billing-reconcile, e2e-billing-flow. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-billing-stripe/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-billing-stripe/Cargo.toml -p corelink-billing-stripe --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-billing-stripe/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-billing-stripe/Cargo.toml); blob `595bc01e3c22bf4d0e7ae4d9c75aa4cb346e127a`.
- [crates/corelink-billing-stripe/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-billing-stripe/src/lib.rs); blob `2aef56ced08989fafd6302fc6adaf11dc6d7b81f`.
- [docs/knowledge/crates/billing-commerce.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/billing-commerce.md); blob `be788f7829b718bfc0709a4e336f1095d6815bdd`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
