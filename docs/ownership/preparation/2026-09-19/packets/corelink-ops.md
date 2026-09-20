# Preparação: corelink-ops

**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.

**Manifesto:** `crates/corelink-ops/Cargo.toml`. **Baseline:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)

<a id="fatos"></a>
## Fatos

- Package `corelink-ops` em `crates/corelink-ops/Cargo.toml`; 48 targets devolvidos pelo Cargo na baseline.
- População fonte própria: 141 arquivos Rust rastreados, 27850 linhas físicas, excluídas raízes de packages aninhados.
- Entradas confirmadas: `crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs`, `crates/corelink-ops/src/admin/dry_run/bin/rb_fm_205_dry_run.rs`, `crates/corelink-ops/src/admin/dry_run/bin/rb_fm_206_dry_run.rs`, `crates/corelink-ops/src/lib.rs`, `crates/corelink-ops/src/supply_chain/verify/bin/cli.rs`.
- Declarações de navegação (amostra, não API completa): `crates/corelink-ops/src/lib.rs:211` — `pub mod admin;`; `crates/corelink-ops/src/lib.rs:212` — `pub mod alerts;`; `crates/corelink-ops/src/lib.rs:213` — `pub mod chaos;`; `crates/corelink-ops/src/lib.rs:214` — `pub mod config;`

<a id="targets"></a>
## Targets

48 targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `crates/corelink-ops/Cargo.toml`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.

- [`crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs)
- [`crates/corelink-ops/src/admin/dry_run/bin/rb_fm_205_dry_run.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/admin/dry_run/bin/rb_fm_205_dry_run.rs)
- [`crates/corelink-ops/src/admin/dry_run/bin/rb_fm_206_dry_run.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/admin/dry_run/bin/rb_fm_206_dry_run.rs)
- [`crates/corelink-ops/src/lib.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/lib.rs)
- [`crates/corelink-ops/src/supply_chain/verify/bin/cli.rs`](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/supply_chain/verify/bin/cli.rs)

<a id="relacoes"></a>
## Relações

41 declarações de dependência e 2 registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.

Consumidores declarados: `corelink-dsr-statuspage-scheduler`, `e2e-byok-revoke`.

<a id="okf"></a>
## OKF

- Direto: `docs/knowledge/ops/sre-operations-hub.md` — SRE operations hub (corelink-ops + satellites)
- Roteador existente: `python3 scripts/okf_context.py --file crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs --full`; ausência de match deve permanecer explícita.

<a id="riscos"></a>
## Riscos

- Investigar ativação e compatibilidade das features declaradas: default, production. Não assumir que --all-features é válido.
- Fronteira a conferir: `crates/corelink-ops/src/admin.rs:26` contém `pub use corelink_handler_admin::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-ops/src/admin.rs:33` contém `pub use corelink_dual_approval::*;`; provar seleção e efeito, não inferir runtime do nome.
- Fronteira a conferir: `crates/corelink-ops/src/admin/api.rs:82` contém `#![forbid(unsafe_code)]`; provar seleção e efeito, não inferir runtime do nome.
- Coordenar contratos com consumidores declarados corelink-dsr-statuspage-scheduler, e2e-byok-revoke. Aresta Cargo não certifica chamada ou produção.

<a id="comandos"></a>
## Comandos

- **EXECUTED / READ_ONLY:** `cargo metadata --locked --offline --no-deps --format-version=1`. Invocação do workspace correspondente retornou o package e seus targets; não é comando individual de teste nem grafo resolvido.
- **NOT_EXECUTED / READ_ONLY:** `python3 scripts/okf_context.py --file crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs --full`. seleção de conceitos; match vazio não é conclusão de ausência
- **NOT_EXECUTED / READ_ONLY_RESOLUTION:** `cargo tree --locked --offline --manifest-path crates/corelink-ops/Cargo.toml -p corelink-ops --target x86_64-unknown-linux-gnu --edges normal,build`. seleção Linux de análise, não prova de build implantado; confrontar com a matriz real da crate

<a id="fontes"></a>
## Fontes

- [crates/corelink-ops/Cargo.toml](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/Cargo.toml); blob `e5c51bb18378cb47c2df1b66df163b0e1b22eaff`.
- [crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/admin/dry_run/bin/rb_fm_201_dry_run.rs); blob `628a476e67e0e191fbaeeb418db8744644ed9f62`.
- [crates/corelink-ops/src/admin/dry_run/bin/rb_fm_205_dry_run.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/admin/dry_run/bin/rb_fm_205_dry_run.rs); blob `cb8d48f1163c3101d8084e33c849682f11bb5a5a`.
- [crates/corelink-ops/src/admin/dry_run/bin/rb_fm_206_dry_run.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/admin/dry_run/bin/rb_fm_206_dry_run.rs); blob `016869b236e9d0b3080e86ec79f503fcc954891c`.
- [crates/corelink-ops/src/lib.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/lib.rs); blob `440470d297248bab67ce6cf7da370f28c7e8b771`.
- [crates/corelink-ops/src/supply_chain/verify/bin/cli.rs](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-ops/src/supply_chain/verify/bin/cli.rs); blob `6e41c8d8be80df14494bc4213c4fcc8e4612eb00`.
- [docs/knowledge/ops/sre-operations-hub.md](https://github.com/HuGR-Labs/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/docs/knowledge/ops/sre-operations-hub.md); blob `3b4bc52a8504cbf4daee27b37d5e94306f2c80bc`.

O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.

[Voltar ao índice](../index.md)
