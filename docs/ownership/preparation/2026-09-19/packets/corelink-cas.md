# Preparação: corelink-cas

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-cas/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-cas` em `crates/corelink-cas/Cargo.toml`; 25 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 70 arquivos Rust rastreados, 20011 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-cas/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-cas/src/lib.rs:86` — `pub mod chunker;`; `crates/corelink-cas/src/lib.rs:87` — `pub mod dedup;`; `crates/corelink-cas/src/lib.rs:88` — `pub mod edge;`; `crates/corelink-cas/src/lib.rs:89` — `pub mod eviction;`

<a id="targets"></a>
## Targets

25 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-cas/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-cas/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-cas/src/lib.rs)

<a id="relacoes"></a>
## Relações

21 declarações de dependência e 3 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-cf-bindings`, `corelink-reapi`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/cas-ac-core.md` — CAS/AC core crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-cas/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-cas/src/chunker.rs:68` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-cas/src/chunker.rs:76` contém `pub use error::ChunkerError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-cas/src/chunker.rs:77` contém `pub use kind::{Chunker, ChunkerKind, ChunkerStep};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-cf-bindings, corelink-reapi. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-cas/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-cas/Cargo.toml -p corelink-cas --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-cas/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-cas/Cargo.toml); blob `a54143de52036f2842319b7f2ec3111add8fa46a`.
- [crates/corelink-cas/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-cas/src/lib.rs); blob `5eef35d4a9c8ad713604a6df5c7493d88b27248e`.
- [docs/knowledge/crates/cas-ac-core.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/cas-ac-core.md); blob `01b501172f40115174fe6c88d8942af5f3590b51`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
