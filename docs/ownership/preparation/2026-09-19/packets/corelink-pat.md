# Preparação: corelink-pat

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-pat/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-pat` em `crates/corelink-pat/Cargo.toml`; 9 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 17 arquivos Rust rastreados, 3370 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-pat/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-pat/src/lib.rs:96` — `pub mod argon;`; `crates/corelink-pat/src/lib.rs:97` — `pub mod error;`; `crates/corelink-pat/src/lib.rs:98` — `pub mod format;`; `crates/corelink-pat/src/lib.rs:99` — `pub mod mint;`

<a id="targets"></a>
## Targets

9 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-pat/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-pat/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-pat/src/lib.rs)

<a id="relacoes"></a>
## Relações

15 declarações de dependência e 5 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-auth`, `corelink-cli`, `corelink-server`, `corelink-worker`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/auth-pat.md` — Auth/PAT crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-pat/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-pat/src/lib.rs:94` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-pat/src/lib.rs:105` contém `pub use argon::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-pat/src/lib.rs:109` contém `pub use error::PatError;`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-auth, corelink-cli, corelink-server, corelink-worker. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-pat/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-pat/Cargo.toml -p corelink-pat --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-pat/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-pat/Cargo.toml); blob `ecea00dee03d3d9e3306c2dc695c620c8b81a210`.
- [crates/corelink-pat/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-pat/src/lib.rs); blob `05e007ea176e5badfd12cb92d1ad85f895b799c6`.
- [docs/knowledge/crates/auth-pat.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/auth-pat.md); blob `0ad15d33ac4635397cec63073c47b3d06afd96c9`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
