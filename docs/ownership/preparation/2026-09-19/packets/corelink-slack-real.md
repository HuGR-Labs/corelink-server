# Preparação: corelink-slack-real

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-slack-real/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-slack-real` em `crates/corelink-slack-real/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 13 arquivos Rust rastreados, 2467 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-slack-real/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-slack-real/src/lib.rs:52` — `pub mod adapter;`; `crates/corelink-slack-real/src/lib.rs:53` — `pub mod audit;`; `crates/corelink-slack-real/src/lib.rs:54` — `pub mod channel;`; `crates/corelink-slack-real/src/lib.rs:55` — `pub mod client;`

## Targets

3 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-slack-real/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-slack-real/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-slack-real/src/lib.rs)

## Relações

10 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapters-cloud`, `corelink-ops`.

## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-slack-real/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-slack-real/src/adapter.rs:88` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-slack-real/src/audit.rs:80` contém `impl InMemorySlackAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-slack-real/src/audit.rs:112` contém `impl SlackAuditSink for InMemorySlackAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapters-cloud, corelink-ops. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-slack-real/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-slack-real/Cargo.toml -p corelink-slack-real --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-slack-real/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-slack-real/Cargo.toml); blob `96d792fe0e2b9636cd5a8cda4c92074825979509`.
- [crates/corelink-slack-real/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-slack-real/src/lib.rs); blob `dec9b13beecdd724018e00087c74b2fdc7081dc4`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
