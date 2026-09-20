# Preparação: corelink-byok

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-byok/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-byok` em `crates/corelink-byok/Cargo.toml`; 26 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 64 arquivos Rust rastreados, 18660 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-byok/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-byok/src/lib.rs:168` — `pub(crate) mod byok_core;`; `crates/corelink-byok/src/lib.rs:195` — `pub use byok_core::*;`; `crates/corelink-byok/src/lib.rs:206` — `pub mod revocation {`; `crates/corelink-byok/src/lib.rs:207` — `pub use crate::byok_revocation::*;`

<a id="targets"></a>
## Targets

26 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-byok/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-byok/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-byok/src/lib.rs)

<a id="relacoes"></a>
## Relações

31 declarações de dependência e 6 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapters-vault`, `corelink-byok-fuzz`, `corelink-ops`, `corelink-server`, `e2e-byok-revoke`, `e2e-tenant-isolation`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/crates/adapter-hosts.md` — Adapter-host crate cluster (surfaces + KMS)
- Direto: `docs/knowledge/storage/byok-envelope-encryption.md` — BYOK envelope encryption at rest (CAS+AC wired, real KMS boundary; owner-runtime gated)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-byok/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: _internal-aws, _internal-azure, _internal-gcp, _internal-vault, _matrix-test, aws, azure, default, gcp, production-azure, production-gcp, real-aws, real-vault, vault. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-byok/benches/envelope_roundtrip.rs:37` contém `impl KmsProvider for InMemoryKmsProvider {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-byok/src/byok_aws.rs:84` contém `#[cfg(not(target_arch = "wasm32"))]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-byok/src/byok_aws.rs:85` contém `pub use real::AwsKmsRealProvider;`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapters-vault, corelink-byok-fuzz, corelink-ops, corelink-server, e2e-byok-revoke, e2e-tenant-isolation. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-byok/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-byok/Cargo.toml -p corelink-byok --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-byok/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-byok/Cargo.toml); blob `8028e49de649bf20150d59938b8fa09dec7b0dfa`.
- [crates/corelink-byok/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-byok/src/lib.rs); blob `61f5bb2b8d0df1524da7c72a2bf218aa8575deca`.
- [docs/knowledge/crates/adapter-hosts.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/adapter-hosts.md); blob `2b74eba18fcf4969549a225dc346e9dd8e5768d8`.
- [docs/knowledge/storage/byok-envelope-encryption.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/storage/byok-envelope-encryption.md); blob `167900440af55dc8bfe0f74614557cb80c260864`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
