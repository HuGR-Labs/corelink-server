# Preparação: corelink-hash

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-hash/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-hash` em `crates/corelink-hash/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 11 arquivos Rust rastreados, 1227 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-hash/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-hash/src/lib.rs:56` — `pub use digest::{Digest, DIGEST_LEN};`; `crates/corelink-hash/src/lib.rs:57` — `pub use error::{HashMismatch, ParseError, COR_CAS_DIGEST_MISMATCH};`; `crates/corelink-hash/src/lib.rs:58` — `pub use store::BlobStoreWrite;`; `crates/corelink-hash/src/lib.rs:59` — `pub use verified_body::VerifiedBody;`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-hash/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-hash/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-hash/src/lib.rs)

<a id="relacoes"></a>
## Relações

9 declarações de dependência e 13 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ac`, `corelink-bazel-bridge`, `corelink-cas`, `corelink-client-verify`, `corelink-crypto`, `corelink-hash-fuzz`, `corelink-meta`, `corelink-meta-fuzz`, `corelink-reapi`, `corelink-server`, `corelink-worker`, `corelink-worker-fuzz`, `e2e-signup-flow`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-hash/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-hash/src/lib.rs:49` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-hash/src/lib.rs:56` contém `pub use digest::{Digest, DIGEST_LEN};`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-hash/src/lib.rs:57` contém `pub use error::{HashMismatch, ParseError, COR_CAS_DIGEST_MISMATCH};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ac, corelink-bazel-bridge, corelink-cas, corelink-client-verify, corelink-crypto, corelink-hash-fuzz. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-hash/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-hash/Cargo.toml -p corelink-hash --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-hash/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-hash/Cargo.toml); blob `0b836118960c8f21c50473dc0e37461fd5b47bd7`.
- [crates/corelink-hash/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-hash/src/lib.rs); blob `403f7e2e4ef8c7bb30a47e7d7eee3ce94a785730`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
