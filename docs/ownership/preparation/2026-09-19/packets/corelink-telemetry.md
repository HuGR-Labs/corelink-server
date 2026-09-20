# Preparação: corelink-telemetry

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-telemetry/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-telemetry` em `crates/corelink-telemetry/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 42 arquivos Rust rastreados, 10876 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-telemetry/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-telemetry/src/lib.rs:83` — `pub mod canary;`; `crates/corelink-telemetry/src/lib.rs:84` — `pub mod lighthouse;`; `crates/corelink-telemetry/src/lib.rs:85` — `pub mod logpush;`; `crates/corelink-telemetry/src/lib.rs:86` — `pub mod otel;`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-telemetry/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-telemetry/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-telemetry/src/lib.rs)

<a id="relacoes"></a>
## Relações

12 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/observability-plane.md` — Observability plane (telemetry / tracing / SLO)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-telemetry/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-telemetry/src/canary.rs:131` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-telemetry/src/canary.rs:150` contém `pub use assertion::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-telemetry/src/canary.rs:153` contém `pub use audit::{`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-telemetry/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-telemetry/Cargo.toml -p corelink-telemetry --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-telemetry/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-telemetry/Cargo.toml); blob `13e606040fd38568e02c899584cfcc198f22fef8`.
- [crates/corelink-telemetry/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-telemetry/src/lib.rs); blob `1b5e6047daa5785a395823959b694836eb29470f`.
- [docs/knowledge/ops/observability-plane.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/observability-plane.md); blob `5ad28a022e76557652b608880387034b0f9fc195`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
