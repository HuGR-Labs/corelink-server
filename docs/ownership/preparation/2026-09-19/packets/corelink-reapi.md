# Preparação: corelink-reapi

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-reapi/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-reapi` em `crates/corelink-reapi/Cargo.toml`; 15 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 37 arquivos Rust rastreados, 13675 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-reapi/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-reapi/src/lib.rs:73` — `pub mod audit;`; `crates/corelink-reapi/src/lib.rs:74` — `pub mod capabilities;`; `crates/corelink-reapi/src/lib.rs:75` — `pub mod error_map;`; `crates/corelink-reapi/src/lib.rs:76` — `pub mod find_missing;`

<a id="targets"></a>
## Targets

15 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-reapi/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-reapi/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/src/lib.rs)

<a id="relacoes"></a>
## Relações

39 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapter-host`, `corelink-reapi-fuzz`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Direto: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-reapi/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, host-server. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-reapi/build.rs:14` contém `if std::env::var_os("CARGO_FEATURE_HOST_SERVER").is_none() {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-reapi/src/audit.rs:323` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-reapi/src/capabilities.rs:114` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapter-host, corelink-reapi-fuzz. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-reapi/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-reapi/Cargo.toml -p corelink-reapi --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-reapi/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/Cargo.toml); blob `eb9ab5b3d0f168ed83021815d7094600c76264bd`.
- [crates/corelink-reapi/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-reapi/src/lib.rs); blob `a4e6596b8741aded884e05b445df1b13edf1419b`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.
- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
