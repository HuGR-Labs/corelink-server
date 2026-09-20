# Preparação: corelink-crypto

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-crypto/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

## Fatos

- Package `corelink-crypto` em `crates/corelink-crypto/Cargo.toml`; 1 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 6 arquivos Rust rastreados, 208 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-crypto/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-crypto/src/lib.rs:79` — `pub mod blake3;`; `crates/corelink-crypto/src/lib.rs:80` — `pub mod client_verify;`; `crates/corelink-crypto/src/lib.rs:81` — `pub mod ct_eq;`; `crates/corelink-crypto/src/lib.rs:82` — `pub mod ed25519;`

## Targets

1 targets enumerados em `../census.json`, registro cujo `manifest` é `crates/corelink-crypto/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-crypto/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-crypto/src/lib.rs)

## Relações

6 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-ac`, `corelink-cas`.

## OKF

- Nenhum conceito OKF declara diretamente fonte própria deste package no levantamento de source_files; não equivale a ausência de documentação semântica.
- Contexto herdado de dependência: `docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md` — ADR-S14-007 — Erasure attestation: Ed25519 + RFC 8785 JCS + 30d key overlap
- Contexto herdado de dependência: `docs/knowledge/compliance/erasure-attestation.md` — Ed25519/JCS erasure attestation
- Contexto herdado de dependência: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-crypto/src/lib.rs --full`; ausência de match deve permanecer explícita.

## Riscos

- Fronteira a conferir: `crates/corelink-crypto/src/blake3.rs:7` contém `pub use corelink_hash::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-crypto/src/client_verify.rs:12` contém `pub use corelink_client_verify::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-crypto/src/ct_eq.rs:12` contém `pub use ::subtle::{Choice, ConstantTimeEq};`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-ac, corelink-cas. Aresta Cargo não certifica chamada ou produção.

## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-crypto/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-crypto/Cargo.toml -p corelink-crypto --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

## Fontes

- [crates/corelink-crypto/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-crypto/Cargo.toml); blob `99e9c09f6b40ec56d0e231164590e87dbca140c7`.
- [crates/corelink-crypto/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-crypto/src/lib.rs); blob `f6edcfe77061306d84c486a0084cf584a0b25c20`.
- [docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md); blob `d18d0859868a6f816d835b5a4f93f5612df575bc`.
- [docs/knowledge/compliance/erasure-attestation.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/compliance/erasure-attestation.md); blob `543a2025d1c490ec8da17de1418014a08f8c337b`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
