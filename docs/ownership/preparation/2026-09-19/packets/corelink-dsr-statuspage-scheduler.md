# Preparação: corelink-dsr-statuspage-scheduler

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-dsr-statuspage-scheduler/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-dsr-statuspage-scheduler` em `crates/corelink-dsr-statuspage-scheduler/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 8 arquivos Rust rastreados, 2091 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-dsr-statuspage-scheduler/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-dsr-statuspage-scheduler/src/lib.rs:68` — `pub mod audit;`; `crates/corelink-dsr-statuspage-scheduler/src/lib.rs:69` — `pub mod cron_log;`; `crates/corelink-dsr-statuspage-scheduler/src/lib.rs:70` — `pub mod row_source;`; `crates/corelink-dsr-statuspage-scheduler/src/lib.rs:71` — `pub mod scheduler;`

<a id="targets"></a>
## Targets

3 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-dsr-statuspage-scheduler/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-dsr-statuspage-scheduler/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dsr-statuspage-scheduler/src/lib.rs)

<a id="relacoes"></a>
## Relações

13 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-clerk-cf`, `corelink-privacy`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-dsr-statuspage-scheduler/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-dsr-statuspage-scheduler/src/audit.rs:132` contém `impl InMemorySchedulerAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dsr-statuspage-scheduler/src/audit.rs:164` contém `impl SchedulerAuditSink for InMemorySchedulerAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-dsr-statuspage-scheduler/src/audit.rs:175` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-clerk-cf, corelink-privacy. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-dsr-statuspage-scheduler/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-dsr-statuspage-scheduler/Cargo.toml -p corelink-dsr-statuspage-scheduler --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-dsr-statuspage-scheduler/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dsr-statuspage-scheduler/Cargo.toml); blob `ed45992c62d0b03cd242abb09ee674697007ef03`.
- [crates/corelink-dsr-statuspage-scheduler/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-dsr-statuspage-scheduler/src/lib.rs); blob `65d4ec45ad7ea4a6d00b1f0a70958b67da4abede`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
