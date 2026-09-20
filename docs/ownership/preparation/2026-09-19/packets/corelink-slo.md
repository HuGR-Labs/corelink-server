# Preparação: corelink-slo

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-slo/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-slo` em `crates/corelink-slo/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 10 arquivos Rust rastreados, 3141 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-slo/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-slo/src/lib.rs:135` — `pub mod alert;`; `crates/corelink-slo/src/lib.rs:136` — `pub mod audit;`; `crates/corelink-slo/src/lib.rs:137` — `pub mod calculator;`; `crates/corelink-slo/src/lib.rs:138` — `pub mod decision;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-slo/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-slo/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-slo/src/lib.rs)

<a id="relacoes"></a>
## Relações

4 declarações de dependência e 11 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-handler-ac`, `corelink-handler-admin`, `corelink-handler-cas`, `corelink-handler-customer`, `corelink-ops`, `corelink-privacy-erasure-worker`, `corelink-region`, `corelink-replica-worker`, `corelink-server`, `corelink-telemetry`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/observability-plane.md` — Observability plane (telemetry / tracing / SLO)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-slo/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-slo/src/alert.rs:351` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-slo/src/audit.rs:157` contém `impl InMemorySloAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-slo/src/audit.rs:198` contém `impl SloAuditSink for InMemorySloAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-handler-ac, corelink-handler-admin, corelink-handler-cas, corelink-handler-customer, corelink-ops, corelink-privacy-erasure-worker. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-slo/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-slo/Cargo.toml -p corelink-slo --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-slo/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-slo/Cargo.toml); blob `cd12aa05323b31644e3b2efe6231bc49825e761d`.
- [crates/corelink-slo/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-slo/src/lib.rs); blob `ab88f25561880927d8919c0052cb4d887ca13c69`.
- [docs/knowledge/ops/observability-plane.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/observability-plane.md); blob `5ad28a022e76557652b608880387034b0f9fc195`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
