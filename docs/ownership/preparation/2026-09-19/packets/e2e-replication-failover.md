# Preparação: e2e-replication-failover

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-replication-failover/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-replication-failover` em `tests/e2e-replication-failover/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 2 arquivos Rust rastreados, 400 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-replication-failover/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-replication-failover/src/lib.rs:28` — `pub struct E2EFixture {`; `tests/e2e-replication-failover/src/lib.rs:46` — `pub fn new() -> Self {`; `tests/e2e-replication-failover/src/lib.rs:67` — `pub fn init_topology(`; `tests/e2e-replication-failover/src/lib.rs:87` — `pub fn heartbeat_all_healthy(`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-replication-failover/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-replication-failover/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-replication-failover/src/lib.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/planes/replication-failover.md` — Multi-region replication + failover plane
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-replication-failover/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-replication-failover/src/lib.rs:14` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-replication-failover/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-replication-failover/Cargo.toml -p e2e-replication-failover --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/planes/replication-failover.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/planes/replication-failover.md); blob `b71c40a5df34dac3205a71eff9af17ce2971368e`.
- [tests/e2e-replication-failover/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-replication-failover/Cargo.toml); blob `13ed9f963c625e3f06885e1aad711547d4ecee67`.
- [tests/e2e-replication-failover/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/tests/e2e-replication-failover/src/lib.rs); blob `aba5e832046012cdbc11131a79daace51cdfa45a`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
