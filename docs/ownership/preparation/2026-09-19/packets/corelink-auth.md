# Preparação: corelink-auth

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-auth/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-auth` em `crates/corelink-auth/Cargo.toml`; 12 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 37 arquivos Rust rastreados, 7749 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-auth/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-auth/src/lib.rs:136` — `pub mod clerk;`; `crates/corelink-auth/src/lib.rs:137` — `pub mod clerk_cf;`; `crates/corelink-auth/src/lib.rs:138` — `pub mod pat;`; `crates/corelink-auth/src/lib.rs:139` — `pub mod schema;`

## Targets

12 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-auth/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-auth/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-auth/src/lib.rs)

## Relações

20 declarações de dependência e 1 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-reapi`.

## OKF

- Direto: `docs/knowledge/crates/auth-pat.md` — Auth/PAT crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-auth/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Investigar ativação e compatibilidade das features declaradas: clerk-jwt-adapter, default, tower-middleware. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-auth/src/clerk.rs:11` contém `pub use corelink_clerk::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-auth/src/clerk_cf.rs:13` contém `pub use corelink_clerk_cf::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-auth/src/lib.rs:133` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-reapi. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-auth/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-auth/Cargo.toml -p corelink-auth --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-auth/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-auth/Cargo.toml); blob `fa745a73c62675cd7d1fb50eb6a2f6d4fc449c50`.
- [crates/corelink-auth/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-auth/src/lib.rs); blob `c0ae769abadc375c1e2995d2ebceab1b4704640b`.
- [docs/knowledge/crates/auth-pat.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/auth-pat.md); blob `0ad15d33ac4635397cec63073c47b3d06afd96c9`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
