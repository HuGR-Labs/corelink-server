# Preparação: corelink-replication-coordinator

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-replication-coordinator/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-replication-coordinator` em `crates/corelink-replication-coordinator/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 2045 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-replication-coordinator/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-replication-coordinator/src/lib.rs:120` — `pub mod audit;`; `crates/corelink-replication-coordinator/src/lib.rs:121` — `pub mod coordinator;`; `crates/corelink-replication-coordinator/src/lib.rs:122` — `pub mod error;`; `crates/corelink-replication-coordinator/src/lib.rs:123` — `pub mod heartbeat;`

## Targets

3 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-replication-coordinator/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-replication-coordinator/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-replication-coordinator/src/lib.rs)

## Relações

5 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-replication`, `e2e-replication-failover`.

## OKF

- Direto: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-replication-coordinator/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-replication-coordinator/src/audit.rs:88` contém `impl InMemoryCoordinatorAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-replication-coordinator/src/audit.rs:102` contém `impl CoordinatorAuditSink for InMemoryCoordinatorAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-replication-coordinator/src/audit.rs:140` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-replication, e2e-replication-failover. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-replication-coordinator/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-replication-coordinator/Cargo.toml -p corelink-replication-coordinator --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-replication-coordinator/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-replication-coordinator/Cargo.toml); blob `7c651aa4798abed83bcae61a2821e268109e80ed`.
- [crates/corelink-replication-coordinator/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-replication-coordinator/src/lib.rs); blob `912d83ab0826045aa07ceacc37ba1ebfe5d4d27a`.
- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
