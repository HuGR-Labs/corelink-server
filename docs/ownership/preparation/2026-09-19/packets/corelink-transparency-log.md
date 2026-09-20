# Preparação: corelink-transparency-log

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-transparency-log/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-transparency-log` em `crates/corelink-transparency-log/Cargo.toml`; 4 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 8 arquivos Rust rastreados, 1132 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-transparency-log/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-transparency-log/src/lib.rs:84` — `pub mod entry;`; `crates/corelink-transparency-log/src/lib.rs:85` — `pub mod error;`; `crates/corelink-transparency-log/src/lib.rs:86` — `pub mod submit;`; `crates/corelink-transparency-log/src/lib.rs:87` — `pub mod witness;`

<a id="targets"></a>
## Targets

4 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-transparency-log/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-transparency-log/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-transparency-log/src/lib.rs)

<a id="relacoes"></a>
## Relações

10 declarações de dependência e 0 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: nenhum na população Cargo examinada.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/compliance/audit-chain.md` — RFC-6962 audit / transparency chain
- Direto: `docs/knowledge/crates/audit-analytics.md` — Audit/analytics crate cluster
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-transparency-log/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-transparency-log/src/entry.rs:166` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-transparency-log/src/lib.rs:82` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-transparency-log/src/lib.rs:89` contém `pub use entry::{RekorHashedRekord, SignedEntry};`; provar seleção e efeito, não inferir runtime do nome.
- Nenhum consumidor Cargo entre os packages elegíveis nesta seleção declarada. Verificar CLI, FFI, workflows, dados e clientes externos antes de alegar isolamento.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-transparency-log/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-transparency-log/Cargo.toml -p corelink-transparency-log --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-transparency-log/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-transparency-log/Cargo.toml); blob `08e00d5c52c21c2bb2563508399d31a1ed1f2ee8`.
- [crates/corelink-transparency-log/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-transparency-log/src/lib.rs); blob `8e9f222412d0860c6c0259df9a6f10331fc5e1a1`.
- [docs/knowledge/compliance/audit-chain.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/compliance/audit-chain.md); blob `e4a587ffccaa3a6e01af52cb039f65d0eb09fc52`.
- [docs/knowledge/crates/audit-analytics.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/crates/audit-analytics.md); blob `864bae2a19336af8486ad5485eaa3764e05db0d9`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
