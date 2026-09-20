# Preparação: corelink-handler-cas-erase

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-handler-cas-erase/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-handler-cas-erase` em `crates/corelink-handler-cas-erase/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 4 arquivos Rust rastreados, 425 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-handler-cas-erase/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-handler-cas-erase/src/lib.rs:46` — `pub mod error;`; `crates/corelink-handler-cas-erase/src/lib.rs:47` — `pub mod handler;`; `crates/corelink-handler-cas-erase/src/lib.rs:49` — `pub use error::CasEraseError;`; `crates/corelink-handler-cas-erase/src/lib.rs:50` — `pub use handler::{`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-handler-cas-erase/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-handler-cas-erase/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-handler-cas-erase/src/lib.rs)

<a id="relacoes"></a>
## Relações

2 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/handler-trait-seam.md` — Handler-trait seam (CAS/AC/customer/erase/admin)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-handler-cas-erase/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-handler-cas-erase/src/handler.rs:191` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-handler-cas-erase/src/lib.rs:42` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-handler-cas-erase/src/lib.rs:49` contém `pub use error::CasEraseError;`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-handler-cas-erase/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-handler-cas-erase/Cargo.toml -p corelink-handler-cas-erase --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-handler-cas-erase/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-handler-cas-erase/Cargo.toml); blob `b10187370fcf1b3eda906b7e8012d085e79752ea`.
- [crates/corelink-handler-cas-erase/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-handler-cas-erase/src/lib.rs); blob `7adfd1a9134e34ff17643338108bf01b84407707`.
- [docs/knowledge/crates/handler-trait-seam.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/handler-trait-seam.md); blob `fd51c52a5b57a62c0e9594832e1df4ba052e91a8`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
