# Preparação: corelink-privacy-erasure-worker

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-privacy-erasure-worker/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-privacy-erasure-worker` em `crates/corelink-privacy-erasure-worker/Cargo.toml`; 9 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 31 arquivos Rust rastreados, 6904 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-privacy-erasure-worker/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-privacy-erasure-worker/src/lib.rs:210` — `pub mod audit_emit;`; `crates/corelink-privacy-erasure-worker/src/lib.rs:211` — `pub mod backends;`; `crates/corelink-privacy-erasure-worker/src/lib.rs:212` — `pub mod error;`; `crates/corelink-privacy-erasure-worker/src/lib.rs:213` — `pub mod event;`

<a id="targets"></a>
## Targets

9 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-privacy-erasure-worker/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-privacy-erasure-worker/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-privacy-erasure-worker/src/lib.rs)

<a id="relacoes"></a>
## Relações

14 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-clerk-cf`, `corelink-dsr-statuspage-scheduler`, `corelink-privacy`, `corelink-server`, `corelink-statuspage-real`, `e2e-dsr`.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/compliance/dsr-erasure.md` — DSR / right-to-erasure pipeline
- Contexto herdado de dependência: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Contexto herdado de dependência: `docs/knowledge/ops/observability-plane.md` — Observability plane (telemetry / tracing / SLO)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-privacy-erasure-worker/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-privacy-erasure-worker/src/audit_emit.rs:105` contém `impl InMemoryErasureAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-privacy-erasure-worker/src/audit_emit.rs:164` contém `impl ErasureAuditSink for InMemoryErasureAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-privacy-erasure-worker/src/audit_emit.rs:195` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-clerk-cf, corelink-dsr-statuspage-scheduler, corelink-privacy, corelink-server, corelink-statuspage-real, e2e-dsr. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-privacy-erasure-worker/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-privacy-erasure-worker/Cargo.toml -p corelink-privacy-erasure-worker --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-privacy-erasure-worker/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-privacy-erasure-worker/Cargo.toml); blob `a04b0f08337276d91c3e055fb5a3f3528e3ad460`.
- [crates/corelink-privacy-erasure-worker/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-privacy-erasure-worker/src/lib.rs); blob `437136aa365e8ad7b943ba3485012a4f4b5deb2b`.
- [docs/knowledge/compliance/dsr-erasure.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/compliance/dsr-erasure.md); blob `123fa54ff307472e7c438202640276c27fe958cb`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.
- [docs/knowledge/ops/observability-plane.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/observability-plane.md); blob `5ad28a022e76557652b608880387034b0f9fc195`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
