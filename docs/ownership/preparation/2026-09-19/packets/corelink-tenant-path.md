# Preparação: corelink-tenant-path

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/tenant-path/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-tenant-path` em `crates/tenant-path/Cargo.toml`; 6 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 9 arquivos Rust rastreados, 1245 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/tenant-path/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/tenant-path/src/lib.rs:37` — `pub mod error;`; `crates/tenant-path/src/lib.rs:42` — `pub use cache::{TdkVersion, TenantPrefixCache, CACHE_CAPACITY};`; `crates/tenant-path/src/lib.rs:43` — `pub use error::DeriveError;`; `crates/tenant-path/src/lib.rs:44` — `pub use prefix::{derive_prefix, TenantDerivationKey, TenantPrefix, TENANT_PREFIX_LEN};`

## Targets

6 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/tenant-path/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/tenant-path/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/tenant-path/src/lib.rs)

## Relações

9 declarações de dependência e 8 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-auth`, `corelink-r2-multipart`, `corelink-reapi`, `corelink-server`, `corelink-tenant-path-fuzz`, `corelink-worker`, `corelink-worker-fuzz`, `e2e-tenant-isolation`.

## OKF

- Direto: `docs/knowledge/tenancy/isolation.md` — Tenant isolation via idFromName(tenant_id)
- Roteador existente: `python3 scripts/okf_context.py --file crates/tenant-path/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/tenant-path/src/cache.rs:184` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/tenant-path/src/lib.rs:35` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/tenant-path/src/lib.rs:42` contém `pub use cache::{TdkVersion, TenantPrefixCache, CACHE_CAPACITY};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-auth, corelink-r2-multipart, corelink-reapi, corelink-server, corelink-tenant-path-fuzz, corelink-worker. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/tenant-path/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/tenant-path/Cargo.toml -p corelink-tenant-path --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/tenant-path/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/tenant-path/Cargo.toml); blob `22260549d8bbb62dcbc415ae976cf610ec095c72`.
- [crates/tenant-path/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/tenant-path/src/lib.rs); blob `ed40ce405d38f398b6537f17531f0fd05be3dfdb`.
- [docs/knowledge/tenancy/isolation.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/tenancy/isolation.md); blob `1f0976efcf940ea2ecbb9f5b3c8dab8ec7c634ae`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
