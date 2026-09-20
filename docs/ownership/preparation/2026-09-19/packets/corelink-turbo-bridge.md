# Preparação: corelink-turbo-bridge

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-turbo-bridge/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-turbo-bridge` em `crates/corelink-turbo-bridge/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 8 arquivos Rust rastreados, 2817 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-turbo-bridge/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-turbo-bridge/src/lib.rs:64` — `pub mod adapter;`; `crates/corelink-turbo-bridge/src/lib.rs:65` — `pub mod audit;`; `crates/corelink-turbo-bridge/src/lib.rs:66` — `pub mod error;`; `crates/corelink-turbo-bridge/src/lib.rs:67` — `pub mod events;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-turbo-bridge/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-turbo-bridge/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-turbo-bridge/src/lib.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/surfaces/turborepo.md` — Turborepo v8 remote-cache surface
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-turbo-bridge/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-turbo-bridge/src/adapter.rs:97` contém `impl core::fmt::Debug for InMemoryKvStore {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-turbo-bridge/src/adapter.rs:103` contém `impl InMemoryKvStore {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-turbo-bridge/src/adapter.rs:113` contém `impl Default for InMemoryKvStore {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-turbo-bridge/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-turbo-bridge/Cargo.toml -p corelink-turbo-bridge --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-turbo-bridge/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-turbo-bridge/Cargo.toml); blob `c531f2af379908107bf8b994077fae19e8103983`.
- [crates/corelink-turbo-bridge/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-turbo-bridge/src/lib.rs); blob `d5b6c8c3bf53f938fdb6dc7c0e2606695a5c29df`.
- [docs/knowledge/surfaces/turborepo.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/surfaces/turborepo.md); blob `2f40d62a4c2fe29b6e2b0acc65ced97df4f57a3b`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
