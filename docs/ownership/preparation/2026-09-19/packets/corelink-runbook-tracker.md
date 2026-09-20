# Preparação: corelink-runbook-tracker

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-runbook-tracker/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-runbook-tracker` em `crates/corelink-runbook-tracker/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 1 arquivos Rust rastreados, 676 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-runbook-tracker/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-runbook-tracker/src/lib.rs:58` — `pub enum TrackerError {`; `crates/corelink-runbook-tracker/src/lib.rs:79` — `pub struct RunbookId(String);`; `crates/corelink-runbook-tracker/src/lib.rs:88` — `pub fn new(raw: impl Into<String>) -> Result<Self, TrackerError> {`; `crates/corelink-runbook-tracker/src/lib.rs:106` — `pub fn as_str(&self) -> &str {`

<a id="targets"></a>
## Targets

1 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-runbook-tracker/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-runbook-tracker/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runbook-tracker/src/lib.rs)

<a id="relacoes"></a>
## Relações

5 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-cli`, `corelink-ops`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-runbook-tracker/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-runbook-tracker/src/lib.rs:37` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-runbook-tracker/src/lib.rs:360` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-runbook-tracker/src/lib.rs:602` contém `std::env::var("PROPTEST_CASES")`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-cli, corelink-ops. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-runbook-tracker/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-runbook-tracker/Cargo.toml -p corelink-runbook-tracker --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-runbook-tracker/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runbook-tracker/Cargo.toml); blob `40d42d3a98fa07df2acae4ef49472d81da5b9877`.
- [crates/corelink-runbook-tracker/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-runbook-tracker/src/lib.rs); blob `c3c770e37961eef6451c51f047a28c669dc017c0`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
