# Preparação: e2e-dsr

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-dsr/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-dsr` em `tests/e2e-dsr/Cargo.toml`; 13 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 16 arquivos Rust rastreados, 2099 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-dsr/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-dsr/src/lib.rs:44` — `pub mod helpers;`; `tests/e2e-dsr/src/lib.rs:45` — `pub mod policy;`; `tests/e2e-dsr/src/lib.rs:46` — `pub mod r2;`; `tests/e2e-dsr/src/lib.rs:48` — `pub use helpers::{`

<a id="targets"></a>
## Targets

13 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-dsr/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-dsr/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-dsr/src/lib.rs)

<a id="relacoes"></a>
## Relações

8 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/compliance/dsr-erasure.md` — DSR / right-to-erasure pipeline
- Contexto herdado de dependência: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-dsr/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-dsr/src/lib.rs:40` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-dsr/src/lib.rs:48` contém `pub use helpers::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-dsr/src/lib.rs:53` contém `pub use policy::{PolicyDecision, RestrictionFlag, TenantPolicyLedger};`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-dsr/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-dsr/Cargo.toml -p e2e-dsr --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/compliance/dsr-erasure.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/compliance/dsr-erasure.md); blob `123fa54ff307472e7c438202640276c27fe958cb`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.
- [tests/e2e-dsr/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-dsr/Cargo.toml); blob `2a7e9d070bce08dcd8d2e8f8db6ca184e4cf15f1`.
- [tests/e2e-dsr/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-dsr/src/lib.rs); blob `6ef8cd16c4a9d2e0891fbd67fbe9355f7253cdea`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
