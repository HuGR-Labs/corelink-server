# Preparação: corelink-statuspage-real

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-statuspage-real/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-statuspage-real` em `crates/corelink-statuspage-real/Cargo.toml`; 4 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 14 arquivos Rust rastreados, 2683 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-statuspage-real/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-statuspage-real/src/lib.rs:52` — `pub mod audit;`; `crates/corelink-statuspage-real/src/lib.rs:53` — `pub mod backend;`; `crates/corelink-statuspage-real/src/lib.rs:54` — `pub mod dsr_bridge;`; `crates/corelink-statuspage-real/src/lib.rs:60` — `pub mod http;`

<a id="targets"></a>
## Targets

4 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-statuspage-real/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-statuspage-real/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-statuspage-real/src/lib.rs)

<a id="relacoes"></a>
## Relações

12 declarações de dependência e 4 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapters-cloud`, `corelink-clerk-cf`, `corelink-dsr-statuspage-scheduler`, `corelink-ops`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-statuspage-real/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-statuspage-real/src/audit.rs:94` contém `impl InMemoryStatuspageAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-statuspage-real/src/audit.rs:126` contém `impl StatuspageAuditSink for InMemoryStatuspageAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-statuspage-real/src/audit.rs:137` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapters-cloud, corelink-clerk-cf, corelink-dsr-statuspage-scheduler, corelink-ops. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-statuspage-real/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-statuspage-real/Cargo.toml -p corelink-statuspage-real --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-statuspage-real/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-statuspage-real/Cargo.toml); blob `4fe081a3c13c920292a43ff0772e5fa7cd0e4420`.
- [crates/corelink-statuspage-real/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-statuspage-real/src/lib.rs); blob `36268b916b8ebe88116b9584e6083679b4297c10`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
