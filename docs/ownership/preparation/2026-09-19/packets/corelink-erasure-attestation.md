# Preparação: corelink-erasure-attestation

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-erasure-attestation/Cargo.toml`. **Baseline:** `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-erasure-attestation` em `crates/corelink-erasure-attestation/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 13 arquivos Rust rastreados, 1794 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-erasure-attestation/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-erasure-attestation/src/lib.rs:101` — `pub mod attestation;`; `crates/corelink-erasure-attestation/src/lib.rs:102` — `pub mod error;`; `crates/corelink-erasure-attestation/src/lib.rs:103` — `pub mod evidence;`; `crates/corelink-erasure-attestation/src/lib.rs:104` — `pub mod key;`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-erasure-attestation/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-erasure-attestation/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-erasure-attestation/src/lib.rs)

<a id="relacoes"></a>
## Relações

14 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-crypto`, `corelink-server`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md` — ADR-S14-007 — Erasure attestation: Ed25519 + RFC 8785 JCS + 30d key overlap
- Direto: `docs/knowledge/compliance/erasure-attestation.md` — Ed25519/JCS erasure attestation
- Direto: `docs/knowledge/crates/privacy-compliance.md` — Privacy/compliance crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-erasure-attestation/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-erasure-attestation/src/attestation.rs:133` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-erasure-attestation/src/evidence.rs:88` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-erasure-attestation/src/key.rs:223` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-crypto, corelink-server. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-erasure-attestation/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-erasure-attestation/Cargo.toml -p corelink-erasure-attestation --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-erasure-attestation/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-erasure-attestation/Cargo.toml); blob `3a29530e9be58ae2bf6d9f36f365c1938cc093f7`.
- [crates/corelink-erasure-attestation/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/crates/corelink-erasure-attestation/src/lib.rs); blob `8336a891946ea7c8999e4727e09ad183d0f7d30d`.
- [docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md); blob `d18d0859868a6f816d835b5a4f93f5612df575bc`.
- [docs/knowledge/compliance/erasure-attestation.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/compliance/erasure-attestation.md); blob `543a2025d1c490ec8da17de1418014a08f8c337b`.
- [docs/knowledge/crates/privacy-compliance.md](https://github.com/HuGR-Labs/corelink-server/blob/37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5/docs/knowledge/crates/privacy-compliance.md); blob `041debef4182856b0ad013d6aa1b9f9087d9ea56`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
