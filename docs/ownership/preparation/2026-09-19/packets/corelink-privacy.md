# Preparação: corelink-privacy

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-privacy/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-privacy` em `crates/corelink-privacy/Cargo.toml`; 24 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 76 arquivos Rust rastreados, 14138 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-privacy/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-privacy/src/lib.rs:146` — `pub mod breach;`; `crates/corelink-privacy/src/lib.rs:147` — `pub mod consent;`; `crates/corelink-privacy/src/lib.rs:148` — `pub mod dpa;`; `crates/corelink-privacy/src/lib.rs:149` — `pub mod dsr;`

<a id="targets"></a>
## Targets

24 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-privacy/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-privacy/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-privacy/src/lib.rs)

<a id="relacoes"></a>
## Relações

18 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/adr/adr-s11-006-consent-purpose-12-arm-closed-enum.md` — ADR-S11-006 — 12-Arm Closed ConsentPurpose Enum Discipline
- Direto: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-privacy/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-privacy/src/breach.rs:79` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-privacy/src/breach.rs:93` contém `pub use audit_emit::{BreachAuditSink, FailingBreachAuditSink, InMemoryBreachAuditSink};`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-privacy/src/breach.rs:94` contém `pub use error::{BreachAuditSinkError, BreachEmitError};`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-privacy/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-privacy/Cargo.toml -p corelink-privacy --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-privacy/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-privacy/Cargo.toml); blob `db717dbc11d9b3f1ef932b270772ea97e4159c94`.
- [crates/corelink-privacy/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-privacy/src/lib.rs); blob `44ebdbc8d1fb6e048f9cd3d1d0dc9ce84680a56d`.
- [docs/knowledge/adr/adr-s11-006-consent-purpose-12-arm-closed-enum.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/adr/adr-s11-006-consent-purpose-12-arm-closed-enum.md); blob `fd2c85889b536c2326200bfb15daae78586541e8`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
