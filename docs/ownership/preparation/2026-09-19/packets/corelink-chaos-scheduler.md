# Preparação: corelink-chaos-scheduler

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-chaos-scheduler/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-chaos-scheduler` em `crates/corelink-chaos-scheduler/Cargo.toml`; 4 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 7 arquivos Rust rastreados, 1821 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-chaos-scheduler/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-chaos-scheduler/src/lib.rs:60` — `pub mod catalog;`; `crates/corelink-chaos-scheduler/src/lib.rs:61` — `pub mod runner;`; `crates/corelink-chaos-scheduler/src/lib.rs:62` — `pub mod types;`; `crates/corelink-chaos-scheduler/src/lib.rs:65` — `pub use catalog::{canonical_catalog, distinct_fm_count, lookup};`

<a id="targets"></a>
## Targets

4 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-chaos-scheduler/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-chaos-scheduler/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-chaos-scheduler/src/lib.rs)

<a id="relacoes"></a>
## Relações

1 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ops`, `e2e-chaos`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-chaos-scheduler/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-chaos-scheduler/src/catalog.rs:130` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-chaos-scheduler/src/lib.rs:58` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-chaos-scheduler/src/lib.rs:65` contém `pub use catalog::{canonical_catalog, distinct_fm_count, lookup};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ops, e2e-chaos. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-chaos-scheduler/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-chaos-scheduler/Cargo.toml -p corelink-chaos-scheduler --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-chaos-scheduler/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-chaos-scheduler/Cargo.toml); blob `c153fefe9f40f10c87d42397422f8656348a30d5`.
- [crates/corelink-chaos-scheduler/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-chaos-scheduler/src/lib.rs); blob `b60565cf6e2531b9dc97e9cbb88eaac160117924`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
