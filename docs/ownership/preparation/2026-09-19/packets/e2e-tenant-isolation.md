# Preparação: e2e-tenant-isolation

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `tests/e2e-tenant-isolation/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `e2e-tenant-isolation` em `tests/e2e-tenant-isolation/Cargo.toml`; 2 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 10 arquivos Rust rastreados, 3009 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `tests/e2e-tenant-isolation/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `tests/e2e-tenant-isolation/src/lib.rs:78` — `pub mod fakes;`; `tests/e2e-tenant-isolation/src/lib.rs:79` — `pub mod tenants;`; `tests/e2e-tenant-isolation/src/lib.rs:81` — `pub use fakes::{`; `tests/e2e-tenant-isolation/src/lib.rs:87` — `pub use tenants::TenantCtx;`

<a id="targets"></a>
## Targets

2 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `tests/e2e-tenant-isolation/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`tests/e2e-tenant-isolation/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-tenant-isolation/src/lib.rs)

<a id="relacoes"></a>
## Relações

10 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/crates/audit-analytics.md` — Audit/analytics crate cluster
- Contexto herdado de dependência: `docs/knowledge/crates/adapter-hosts.md` — Adapter-host crate cluster (surfaces + KMS)
- Contexto herdado de dependência: `docs/knowledge/storage/byok-envelope-encryption.md` — BYOK envelope encryption at rest (CAS+AC wired, real KMS boundary; owner-runtime gated)
- Roteador existente: `python3 scripts/okf_context.py --file tests/e2e-tenant-isolation/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `tests/e2e-tenant-isolation/src/fakes.rs:9` contém `pub use extended::{`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-tenant-isolation/src/fakes.rs:14` contém `pub use foundation::{AuditAttempt, AuditCapture, DenyKind, FakeError};`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `tests/e2e-tenant-isolation/src/fakes.rs:15` contém `pub use stores::{CasStore, D1Row, D1Store, IdempotencyStore, PatStore, QuotaStore, RateLimiter};`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file tests/e2e-tenant-isolation/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path tests/e2e-tenant-isolation/Cargo.toml -p e2e-tenant-isolation --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [docs/knowledge/crates/adapter-hosts.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/adapter-hosts.md); blob `2b74eba18fcf4969549a225dc346e9dd8e5768d8`.
- [docs/knowledge/crates/audit-analytics.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/audit-analytics.md); blob `864bae2a19336af8486ad5485eaa3764e05db0d9`.
- [docs/knowledge/storage/byok-envelope-encryption.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/storage/byok-envelope-encryption.md); blob `167900440af55dc8bfe0f74614557cb80c260864`.
- [tests/e2e-tenant-isolation/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-tenant-isolation/Cargo.toml); blob `9e03007432df3be8f608c47f79551d40451bca24`.
- [tests/e2e-tenant-isolation/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/tests/e2e-tenant-isolation/src/lib.rs); blob `2823e3c6dd18bdb969ed6cd0af05d1a31229447f`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
