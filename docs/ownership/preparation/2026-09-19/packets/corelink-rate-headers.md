# Preparação: corelink-rate-headers

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-rate-headers/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-rate-headers` em `crates/corelink-rate-headers/Cargo.toml`; 3 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 10 arquivos Rust rastreados, 4730 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-rate-headers/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-rate-headers/src/lib.rs:232` — `pub mod audit;`; `crates/corelink-rate-headers/src/lib.rs:233` — `pub mod circuit;`; `crates/corelink-rate-headers/src/lib.rs:234` — `pub mod error;`; `crates/corelink-rate-headers/src/lib.rs:235` — `pub mod headers;`

<a id="targets"></a>
## Targets

3 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-rate-headers/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-rate-headers/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-rate-headers/src/lib.rs)

<a id="relacoes"></a>
## Relações

3 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-billing`, `e2e-resilience`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/tenancy/storage-quota-header.md` — Storage-quota / byte-accounting header
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-rate-headers/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Fronteira a conferir: `crates/corelink-rate-headers/src/audit.rs:161` contém `impl InMemoryCircuitAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-rate-headers/src/audit.rs:202` contém `impl CircuitAuditSink for InMemoryCircuitAuditSink {`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-rate-headers/src/audit.rs:235` contém `#[cfg(test)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-billing, e2e-resilience. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-rate-headers/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-rate-headers/Cargo.toml -p corelink-rate-headers --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-rate-headers/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-rate-headers/Cargo.toml); blob `4e5afd673f5ac32a262b22b74382ef0fc3944b46`.
- [crates/corelink-rate-headers/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-rate-headers/src/lib.rs); blob `0e8c011c8f381860c82a67f103c2b537726d142e`.
- [docs/knowledge/tenancy/storage-quota-header.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/tenancy/storage-quota-header.md); blob `856d8f27801a5b510077e24d7897ffa35e2ee55e`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
