# Preparação: corelink-tracing

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-tracing/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-tracing` em `crates/corelink-tracing/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 3217 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-tracing/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-tracing/src/lib.rs:122` — `pub mod audit;`; `crates/corelink-tracing/src/lib.rs:123` — `pub mod context;`; `crates/corelink-tracing/src/lib.rs:124` — `pub mod error;`; `crates/corelink-tracing/src/lib.rs:125` — `pub mod exporter;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-tracing/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-tracing/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-tracing/src/lib.rs)

<a id="relacoes"></a>
## Relações

7 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-telemetry`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/observability-plane.md` — Observability plane (telemetry / tracing / SLO)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-tracing/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-tracing/src/audit.rs:108` contém `pub use crate::error::TracingAuditSinkError as TracingAuditEmitError;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-tracing/src/audit.rs:133` contém `impl InMemoryTracingAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-tracing/src/audit.rs:174` contém `impl TracingAuditSink for InMemoryTracingAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-telemetry. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-tracing/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-tracing/Cargo.toml -p corelink-tracing --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-tracing/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-tracing/Cargo.toml); blob `c80a90b8f45aec0bfc9dabd3cdade8d5a099de85`.
- [crates/corelink-tracing/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-tracing/src/lib.rs); blob `4d67172dac125b5c0dbe0d26c6b47ac9f3babc95`.
- [docs/knowledge/ops/observability-plane.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/observability-plane.md); blob `5ad28a022e76557652b608880387034b0f9fc195`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
