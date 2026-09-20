# Preparação: corelink-client-verify

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-client-verify/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-client-verify` em `crates/corelink-client-verify/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 13 arquivos Rust rastreados, 2582 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-client-verify/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-client-verify/src/lib.rs:55` — `pub mod config;`; `crates/corelink-client-verify/src/lib.rs:57` — `pub mod digest;`; `crates/corelink-client-verify/src/lib.rs:59` — `pub mod error;`; `crates/corelink-client-verify/src/lib.rs:61` — `pub mod verifier;`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-client-verify/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-client-verify/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-client-verify/src/lib.rs)

<a id="relacoes"></a>
## Relações

16 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-client-verify-fuzz`, `corelink-crypto`, `corelink-go`, `corelink-py`, `corelink-reapi`, `corelink-wasm`.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-client-verify/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, ffi, stream. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-client-verify/src/config.rs:75` contém `#[cfg_attr(`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-client-verify/src/config.rs:111` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-client-verify/src/digest.rs:11` contém `pub use corelink_hash::{Digest, ParseError, DIGEST_LEN};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-client-verify-fuzz, corelink-crypto, corelink-go, corelink-py, corelink-reapi, corelink-wasm. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-client-verify/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-client-verify/Cargo.toml -p corelink-client-verify --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-client-verify/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-client-verify/Cargo.toml); blob `c5b3571c11b37fabd2ba02e8bbb9cf3962819f62`.
- [crates/corelink-client-verify/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-client-verify/src/lib.rs); blob `70a5467bf919936d4e43c56d4271bb015d85fb96`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
