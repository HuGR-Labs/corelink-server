# Preparação: corelink-tier-selection

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-tier-selection/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-tier-selection` em `crates/corelink-tier-selection/Cargo.toml`; 4 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 11 arquivos Rust rastreados, 3157 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-tier-selection/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-tier-selection/src/lib.rs:95` — `pub mod audit;`; `crates/corelink-tier-selection/src/lib.rs:96` — `pub mod dpa;`; `crates/corelink-tier-selection/src/lib.rs:97` — `pub mod error;`; `crates/corelink-tier-selection/src/lib.rs:98` — `pub mod ledger;`

<a id="targets"></a>
## Targets

4 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-tier-selection/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-tier-selection/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-tier-selection/src/lib.rs)

<a id="relacoes"></a>
## Relações

7 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `corelink-billing-stripe-materializer`, `corelink-server`, `corelink-stripe-real`, `e2e-billing-flow`, `e2e-signup-flow`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/billing-commerce.md` — Billing/commerce crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-tier-selection/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-tier-selection/src/audit.rs:156` contém `impl InMemoryTierSelectionAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-tier-selection/src/audit.rs:194` contém `impl TierSelectionAuditSink for InMemoryTierSelectionAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-tier-selection/src/dpa.rs:33` contém `impl InMemoryDpaGate {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, corelink-billing-stripe-materializer, corelink-server, corelink-stripe-real, e2e-billing-flow, e2e-signup-flow. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-tier-selection/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-tier-selection/Cargo.toml -p corelink-tier-selection --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-tier-selection/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-tier-selection/Cargo.toml); blob `ac742611a84dfdbc64a368762b5e92cb4bf6a836`.
- [crates/corelink-tier-selection/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-tier-selection/src/lib.rs); blob `ca4ff79f9e120570f81e2c46a8d97496a59cc275`.
- [docs/knowledge/crates/billing-commerce.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/billing-commerce.md); blob `be788f7829b718bfc0709a4e336f1095d6815bdd`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
