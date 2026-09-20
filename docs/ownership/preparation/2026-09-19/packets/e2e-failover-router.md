# Preparação: e2e-failover-router

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-failover-router/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `e2e-failover-router` em `tests/e2e-failover-router/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 610 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-failover-router/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-failover-router/src/lib.rs:52` — `pub struct LogicalClock {`; `tests/e2e-failover-router/src/lib.rs:59` — `pub fn new(t0_ms: u64) -> Self {`; `tests/e2e-failover-router/src/lib.rs:73` — `pub fn now_ms(&self) -> Result<u64, HarnessError> {`; `tests/e2e-failover-router/src/lib.rs:85` — `pub fn advance_ms(&self, delta_ms: u64) -> Result<(), HarnessError> {`

## Targets

2 targets enumerados em `../census.json`, registro cujo `manifest` é `tests/e2e-failover-router/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-failover-router/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-failover-router/src/lib.rs)

## Relações

3 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-failover-router/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `tests/e2e-failover-router/src/lib.rs:42` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-failover-router/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-failover-router/Cargo.toml -p e2e-failover-router --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.
- [tests/e2e-failover-router/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-failover-router/Cargo.toml); blob `685fd810daf578ae394768697fde0db85e332ebb`.
- [tests/e2e-failover-router/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-failover-router/src/lib.rs); blob `c9b2b84e362c0ebdc63db67d0b6a1938f42e5dee`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
