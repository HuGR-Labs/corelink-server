# Preparação: corelink-clerk-cf

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-clerk-cf/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-clerk-cf` em `crates/corelink-clerk-cf/Cargo.toml`; 7 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 16 arquivos Rust rastreados, 4998 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-clerk-cf/src/lib.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-clerk-cf/src/lib.rs:36` — `pub mod audit_sink;`; `crates/corelink-clerk-cf/src/lib.rs:37` — `pub mod cf_fetch;`; `crates/corelink-clerk-cf/src/lib.rs:38` — `pub mod cf_kv;`; `crates/corelink-clerk-cf/src/lib.rs:39` — `pub mod clerk_health_do;`

<a id="targets"></a>
## Targets

7 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-clerk-cf/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-clerk-cf/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-clerk-cf/src/lib.rs)

<a id="relacoes"></a>
## Relações

19 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-adapters-cloud`, `corelink-auth`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-clerk-cf/src/lib.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: cf-billing-real, default, tenant-region-real. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-clerk-cf/src/audit_sink.rs:227` contém `#[cfg(target_arch = "wasm32")]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-clerk-cf/src/audit_sink.rs:235` contém `#[cfg(not(target_arch = "wasm32"))]`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-clerk-cf/src/audit_sink.rs:293` contém `#[cfg(target_arch = "wasm32")]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-adapters-cloud, corelink-auth. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-clerk-cf/src/lib.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-clerk-cf/Cargo.toml -p corelink-clerk-cf --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-clerk-cf/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-clerk-cf/Cargo.toml); blob `134efb95aead608a7e2439a5f90108a143224186`.
- [crates/corelink-clerk-cf/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-clerk-cf/src/lib.rs); blob `3f1d8fe1d5ded4fa31d23619d754a5cf7c4f37e9`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
