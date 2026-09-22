# Preparação: e2e-resilience

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-resilience/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-resilience` em `tests/e2e-resilience/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 838 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-resilience/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-resilience/src/lib.rs:61` — `pub struct LogicalClock {`; `tests/e2e-resilience/src/lib.rs:68` — `pub fn new(t0_ms: u64) -> Self {`; `tests/e2e-resilience/src/lib.rs:82` — `pub fn now_ms(&self) -> Result<u64, ResilienceError> {`; `tests/e2e-resilience/src/lib.rs:100` — `pub fn advance_ms(&self, delta_ms: u64) -> Result<u64, ResilienceError> {`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-resilience/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-resilience/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-resilience/src/lib.rs)

<a id="relacoes"></a>
## Relações

4 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/tenancy/storage-quota-header.md` — Storage-quota / byte-accounting header
- Contexto herdado de dependência: `docs/knowledge/crates/operations.md` — Operations crate cluster (GC, replication, ratelimit, SRE)
- Contexto herdado de dependência: `docs/knowledge/tenancy/governance.md` — Tenant governance: rate-limit, customer & user admin
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-resilience/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-resilience/src/lib.rs:51` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-resilience/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-resilience/Cargo.toml -p e2e-resilience --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/crates/operations.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/operations.md); blob `f85d562c6e20b4552ed452a569c38e1f66467440`.
- [docs/knowledge/tenancy/governance.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/tenancy/governance.md); blob `197398fb2e465bfe4aef8f3f0a8d7d31dbac93bf`.
- [docs/knowledge/tenancy/storage-quota-header.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/tenancy/storage-quota-header.md); blob `856d8f27801a5b510077e24d7897ffa35e2ee55e`.
- [tests/e2e-resilience/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-resilience/Cargo.toml); blob `7624e8c45e663ad6e26ef0b886a1d472fa90cc10`.
- [tests/e2e-resilience/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-resilience/src/lib.rs); blob `12ad92f053e0c263021720b6f388cbe20282c723`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
