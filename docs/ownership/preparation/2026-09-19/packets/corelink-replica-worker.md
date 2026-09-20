# Preparação: corelink-replica-worker

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-replica-worker/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-replica-worker` em `crates/corelink-replica-worker/Cargo.toml`; 6 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 15 arquivos Rust rastreados, 3580 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-replica-worker/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-replica-worker/src/lib.rs:100` — `pub mod aggregator;`; `crates/corelink-replica-worker/src/lib.rs:101` — `pub mod audit;`; `crates/corelink-replica-worker/src/lib.rs:102` — `pub mod cardinality;`; `crates/corelink-replica-worker/src/lib.rs:103` — `pub mod coverage;`

<a id="targets"></a>
## Targets

6 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-replica-worker/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-replica-worker/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-replica-worker/src/lib.rs)

<a id="relacoes"></a>
## Relações

4 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-failover-router`, `corelink-ops`, `corelink-replication`, `corelink-replication-coordinator`, `e2e-failover-router`, `e2e-replication-failover`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-replica-worker/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-replica-worker/src/aggregator.rs:125` contém `impl InMemoryOfflineAggregator {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-replica-worker/src/aggregator.rs:164` contém `impl OfflineAggregator for InMemoryOfflineAggregator {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-replica-worker/src/audit.rs:146` contém `impl InMemoryReplicaAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-failover-router, corelink-ops, corelink-replication, corelink-replication-coordinator, e2e-failover-router, e2e-replication-failover. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-replica-worker/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-replica-worker/Cargo.toml -p corelink-replica-worker --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-replica-worker/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-replica-worker/Cargo.toml); blob `05dd8e80a0daec400a8f8f402019ef368f30c5b3`.
- [crates/corelink-replica-worker/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-replica-worker/src/lib.rs); blob `d56ee1095eb03417fe635b69cf5555c0d63b6a7f`.
- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
