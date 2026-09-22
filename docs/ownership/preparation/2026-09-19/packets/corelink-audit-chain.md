# Preparação: corelink-audit-chain

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-audit-chain/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-audit-chain` em `crates/corelink-audit-chain/Cargo.toml`; 8 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 29 arquivos Rust rastreados, 15593 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-audit-chain/src/bin/verifier.rs`, `crates/corelink-audit-chain/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-audit-chain/src/lib.rs:139` — `pub mod archive_producer;`; `crates/corelink-audit-chain/src/lib.rs:140` — `pub mod audit;`; `crates/corelink-audit-chain/src/lib.rs:141` — `pub mod chain;`; `crates/corelink-audit-chain/src/lib.rs:142` — `pub mod epoch;`

<a id="targets"></a>
## Targets

8 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-audit-chain/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-audit-chain/src/bin/verifier.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-audit-chain/src/bin/verifier.rs)
- [`crates/corelink-audit-chain/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-audit-chain/src/lib.rs)

<a id="relacoes"></a>
## Relações

31 declarações de dependência e 7 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-audit`, `corelink-audit-chain-fuzz`, `corelink-billing-stripe-materializer`, `corelink-clerk-cf`, `corelink-cli`, `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/compliance/audit-chain.md` — RFC-6962 audit / transparency chain
- Direto: `docs/knowledge/crates/audit-analytics.md` — Audit/analytics crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-audit-chain/src/bin/verifier.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, live-pg, neon-real. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-audit-chain/src/archive_producer.rs:713` contém `impl InMemoryArchiveSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-audit-chain/src/archive_producer.rs:745` contém `impl ArchiveSink for InMemoryArchiveSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-audit-chain/src/archive_producer.rs:775` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-audit, corelink-audit-chain-fuzz, corelink-billing-stripe-materializer, corelink-clerk-cf, corelink-cli, corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-audit-chain/src/bin/verifier.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-audit-chain/Cargo.toml -p corelink-audit-chain --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-audit-chain/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-audit-chain/Cargo.toml); blob `358c932baab32c7ecc36ca238a8c42625e82a9af`.
- [crates/corelink-audit-chain/src/bin/verifier.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-audit-chain/src/bin/verifier.rs); blob `6fb35eec5761f654415c81847aeb5ec1336c084f`.
- [crates/corelink-audit-chain/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-audit-chain/src/lib.rs); blob `ab35886b8dda949c3b2a26ad16c01f0fbbdef839`.
- [docs/knowledge/compliance/audit-chain.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/compliance/audit-chain.md); blob `e4a587ffccaa3a6e01af52cb039f65d0eb09fc52`.
- [docs/knowledge/crates/audit-analytics.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/audit-analytics.md); blob `864bae2a19336af8486ad5485eaa3764e05db0d9`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
