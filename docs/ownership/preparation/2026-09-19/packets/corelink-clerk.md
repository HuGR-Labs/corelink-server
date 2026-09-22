# Preparação: corelink-clerk

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-clerk/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-clerk` em `crates/corelink-clerk/Cargo.toml`; 10 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 20 arquivos Rust rastreados, 4772 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-clerk/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-clerk/src/lib.rs:114` — `pub mod adapter;`; `crates/corelink-clerk/src/lib.rs:115` — `pub mod config;`; `crates/corelink-clerk/src/lib.rs:117` — `pub mod env_config;`; `crates/corelink-clerk/src/lib.rs:118` — `pub mod error;`

<a id="targets"></a>
## Targets

10 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-clerk/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-clerk/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-clerk/src/lib.rs)

<a id="relacoes"></a>
## Relações

20 declarações de dependência e 5 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-auth`, `corelink-clerk`, `corelink-clerk-cf`, `corelink-worker`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/auth-pat.md` — Auth/PAT crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-clerk/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, http-fetcher, jwt-adapter, test-utils. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-clerk/src/config.rs:268` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-clerk/src/env_config.rs:231` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-clerk/src/fakes.rs:27` contém `impl std::fmt::Debug for InMemoryKvCache {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-auth, corelink-clerk, corelink-clerk-cf, corelink-worker. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-clerk/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-clerk/Cargo.toml -p corelink-clerk --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-clerk/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-clerk/Cargo.toml); blob `39273401e870c591c591dd7258b9eb44167384c2`.
- [crates/corelink-clerk/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-clerk/src/lib.rs); blob `66d6db966cc4c6f6fa261e9f3db5f8bbb8bb7ef0`.
- [docs/knowledge/crates/auth-pat.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/auth-pat.md); blob `0ad15d33ac4635397cec63073c47b3d06afd96c9`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
