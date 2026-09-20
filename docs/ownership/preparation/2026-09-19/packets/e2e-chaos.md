# Preparação: e2e-chaos

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-chaos/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `e2e-chaos` em `tests/e2e-chaos/Cargo.toml`; 13 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 13 arquivos Rust rastreados, 1246 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-chaos/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-chaos/src/lib.rs:70` — `pub enum ChaosE2eDrill {`; `tests/e2e-chaos/src/lib.rs:92` — `pub fn experiment_id(self) -> &'static str {`; `tests/e2e-chaos/src/lib.rs:113` — `pub fn make_experiment(drill: ChaosE2eDrill) -> ChaosExperiment {`; `tests/e2e-chaos/src/lib.rs:217` — `pub struct ChaosTestTelemetry {`

## Targets

13 targets enumerados em `../census.json`, registro cujo `manifest` é `tests/e2e-chaos/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-chaos/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-chaos/src/lib.rs)

## Relações

2 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-chaos/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `tests/e2e-chaos/src/lib.rs:45` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-chaos/src/lib.rs:450` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-chaos/tests/prop_seed_replay_identical.rs:30` contém `std::env::var("PROPTEST_CASES")`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-chaos/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-chaos/Cargo.toml -p e2e-chaos --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.
- [tests/e2e-chaos/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-chaos/Cargo.toml); blob `df80446c32ba222ae2ff66ada6b6ff5ae50bfe40`.
- [tests/e2e-chaos/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-chaos/src/lib.rs); blob `3e985bee6c4c39075a2df09adedfb8ae7514c98d`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
