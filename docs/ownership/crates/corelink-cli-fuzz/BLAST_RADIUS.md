---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-cli-fuzz
manifest: tools/cli/fuzz/Cargo.toml
source_commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
profile: "S"
state: draft
evidence_set: corelink-cli-fuzz-structural-normalization-20260921
---

# corelink-cli-fuzz — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) ·
[Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Leitura rápida e escopo

O package contém cinco harnesses independentes que chamam a fachada pura do
`corelink-cli`. O maior risco é tratar um bin declarado ou script como prova de
execução, ou mudar a API pai sem recensear os targets. A fronteira crítica é
`tools/cli/fuzz` → `tools/cli/src/{lib,config,auth,error}.rs`; a propagação externa
é para workflows, script global, specs S15 e possíveis corpora/artifacts. Não há
runtime de produção, endpoint, banco ou deployment próprio observado.

**Builds avaliados:** package pinado e cinco bins declarados; nenhum build/resolve
executado. **Ambientes não observados:** runner, cargo-fuzz, corpus, CI run,
release e consumidores externos. Dependência declarada, chamada estática, wiring
e runtime permanecem estados separados.

<a id="b02"></a>
## B02 — Inventário e método

| População | Fontes / método | Seleção / revisão | Limite do levantamento |
|---|---|---|---|
| Declarações Cargo | `Cargo.toml`, `Cargo.lock`, fontes dos cinco bins | package source pin `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`; the independent lockfile records the parent package at version `0.1.2` | declarado não é resolvido nem executado |
| Dependência local | `corelink-cli = { path = ".." }`, `tools/cli/Cargo.toml` | inspeção inversa por `rg`/Git | não substitui `cargo tree` |
| API/call sites | `fuzz_api`, `config`, `auth`, `error`, targets | símbolos e linhas citados | não prova chamada em runtime |
| Build/wiring | `scripts/fuzz-all.sh`, `.github/workflows/*`, sealed WI-S15-006 | busca fora de Cargo | execução, secrets e runners não observados; alguns paths são stale |
| Dados/artifacts | paths `corpus`, `artifacts`, lockfile e docs | existência estática | não há prova de corpus materializado |

**Estados:** implementação significa código presente; wired significa seleção
declarada por script/workflow; `runtime_verified` exige resultado observável
pinado. A ausência de uma ocorrência em busca não prova ausência global.

<a id="b03"></a>
## B03 — Registro completo de relações diretas

| ID | Tipo / direção | Superfície | Ativação | Owner do contrato |
|---|---|---|---|---|
| [REL-001](#rel-001) | dependency fuzz → CLI | path dep `corelink-cli` | resolução do manifesto | `corelink-cli` |
| [REL-002](#rel-002) | API harness → parser | `parse_config_toml` | `config_toml` | `corelink-cli` |
| [REL-003](#rel-003) | API harness → config | `apply_config_key` | `cli_input`, `config_toml` | `corelink-cli` |
| [REL-004](#rel-004) | API harness → auth | `validate_pat_shape` | `cli_input`, `auth_resolution` | `corelink-cli` |
| [REL-005](#rel-005) | API harness → redaction | `count_pat_leaks` | `secret_redaction_check` | `corelink-cli`/CTRL-CRED-001 |
| [REL-006](#rel-006) | input fuzz → config/opaque JSON parser | `CorelinkConfig`, `serde_json::Value` | `json_deserialize` | `corelink-cli` |
| [REL-007](#rel-007) | inverse test surface ↔ parent helpers | `lib_tests`, `integration.rs` | parent test selection | `corelink-cli` |
| [REL-008](#rel-008) | build → lockfile | `Cargo.lock`, direct deps | package resolution | package maintainer |
| [REL-009](#rel-009) | wiring script → target | `scripts/fuzz-all.sh` names | manual/script invocation | CI maintainer |
| [REL-010](#rel-010) | wiring workflow → target | nightly matrix excludes CLI | workflow dispatch only | CI maintainer |
| [REL-011](#rel-011) | spec → harness | WI-S15-006 §6.1 | conformance review | S15 contract owner |
| [REL-012](#rel-012) | artifact → operator | corpus/crash dirs | cargo-fuzz run | runner/operator, unverified |
| [REL-013](#rel-013) | parent CLI → config | login persistence | `login` | `corelink-cli` |
| [REL-014](#rel-014) | parent CLI → auth | generic `resolve_pat` path | non-login commands | `corelink-cli` |
| [REL-015](#rel-015) | parent CLI → config | client tenant cache | client construction | `corelink-cli` |
| [REL-016](#rel-016) | parent CLI → config | config subcommand write | `config set` | `corelink-cli` |
| [REL-017](#rel-017) | parent CLI → auth | login PAT validation | `login` | `corelink-cli` |
| [REL-018](#rel-018) | parent CLI → config | doctor config load | `doctor` | `corelink-cli` |
| [REL-019](#rel-019) | parent CLI → config | Bazel config load | `bazel-init` | `corelink-cli` |
| [REL-020](#rel-020) | parent CLI → config | telemetry config load | every completed command | `corelink-cli` |
| [REL-021](#rel-021) | parent CLI → config | config subcommand reads | `config get/list` | `corelink-cli` |
| [REL-022](#rel-022) | parent CLI → config | whoami persistence | `whoami` | `corelink-cli` |
| [REL-023](#rel-023) | parent CLI → auth | audit export `resolve_pat` | audit production export | `corelink-cli` |
| [REL-024](#rel-024) | parent CLI → auth | audit tail `resolve_pat` | audit tail | `corelink-cli` |
| [REL-025](#rel-025) | parent CLI → config | `config_cmd::run_apply` → `config::apply_file` | `config apply --file` | `corelink-cli` |

<a id="rel-001"></a>
### REL-001 — path dependency into the CLI facade
**Identidade compartilhada:** `repo:1232040291:boundary:cli-fuzz-parent-001`; peer `corelink-cli` B06 deve reutilizar esta chave e apontar de volta para REL-001.

**Dependência / fluxo / impacto:** manifest fuzz declara `corelink-cli`; target importa `corelink_cli::...`; parent API change breaks bins.
**Superfície:** `tools/cli/fuzz/Cargo.toml:26`, `tools/cli/src/lib.rs:53-122`.
**Ativação:** resolução/build de qualquer bin. **Contrato:** path local, sem version pin independente.
**Estado / efeitos:** sem estado persistente; compile failure ou semântica de oráculo divergente.
**Falha / propagação:** parent rename/signature change → target compile/error or false assertion.
**Contenção:** build/test gate do package; não executado. **Validação:** metadata/target build futuro; coordenação `corelink-cli`. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — config_toml para parser TOML
**Identidade:** `repo:1232040291:boundary:cli-fuzz-config-001`.
**Dependência / fluxo / impacto:** bytes → API-001 → `CorelinkConfig`; TOML
parser/config-shape change alters accepted corpus and the local config object.
**Superfície:** `config_toml.rs:26-40`, `lib.rs:61-69`.
**Ativação:** target `config_toml`; **contrato:** `Result`, UTF-8 guard, no filesystem.
**Estado / efeitos:** config local e round-trip tentado; nenhum write durável.
**Falha / propagação:** panic/parse drift/serialization failure → target abort or untested gap.
**Contenção:** branch `if let Ok`, but no semantic assertion. **Validação:** isolated target plus minimized input; not run. [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — key/value fuzz para config dispatch
**Identidade:** `repo:1232040291:boundary:cli-fuzz-config-002`.
**Dependência / fluxo / impacto:** bytes → key/value → API-002 → in-memory mutation.
**Superfície:** `cli_input.rs:25-44`, `config.rs:228-247`.
**Ativação:** target `cli_input` or `config_toml` probes. **Contrato:** errors are structured; unknown keys reject.
**Estado / efeitos:** local `CorelinkConfig`; no `save`, no path access.
**Falha / propagação:** dispatch/schema change → changed error or panic claim; parent config consumers may differ.
**Contenção:** `Result` ignored by target. **Validação:** target and parent integration tests; not run. [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — PAT shape validation
**Identidade:** `repo:1232040291:boundary:cli-fuzz-auth-001`.
**Dependência / fluxo / impacto:** UTF-8 `value` → API-003 → PAT shape result; `cli_input` uses its deterministic key/value split, not argv parsing.
**Superfície:** `auth_resolution.rs:20-29`, `cli_input.rs:46-50`, `auth.rs:35-40`.
**Ativação:** two targets; **contrato:** delegates to `corelink_pat::parse_plaintext`, no cryptographic verification.
**Estado / efeitos:** no network/state; boolean determinism assertion.
**Falha / propagação:** parser format change → accepted corpus/CLI login behavior; server contract must coordinate.
**Contenção/validação:** malformed maps to `PatMalformed`; isolated fuzz and parent auth tests are not run. [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — captured errors to secret scanner
**Identidade:** `repo:1232040291:boundary:cli-fuzz-redaction-001`.
**Dependência / fluxo / impacto:** `ConfigError` Display from API-001/002 → captured String → API-004 → zero assertion.
**Superfície:** `secret_redaction_check.rs:25-52`, `lib.rs:96-122`.
**Ativação:** redaction target. **Contrato:** PAT-shaped count must equal zero for exercised errors.
**Estado / efeitos:** ephemeral String; no output sink other than panic text on failure.
**Falha / propagação:** positive count aborts target; `CliError`, auth resolution, formatter, filesystem/network and general stderr remain uncovered.
**Contenção/validação:** only two pure helpers and UTF-8 key/value input; preserve input/output; not run. [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — JSON config and opaque value parsing
**Identidade:** `repo:1232040291:boundary:cli-fuzz-json-001`.
**Dependência / fluxo / impacto:** UTF-8 bytes → config or generic `Value`; reverse parsing only, not the output formatter/schema.
**Superfície:** `json_deserialize.rs:20-30`.
**Ativação:** `json_deserialize` bin. **Contrato:** invalid JSON returns `Err`;
success serializes a local config/value only.
**Estado/propagação:** local values only; serde/config shape drift changes accepted input; output impact needs separate review.
**Contenção/validação:** `if let Ok` and ignored result; target plus formatter/schema review when requested; not run. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — parent tests and helper surfaces
**Identidade:** `repo:1232040291:boundary:cli-fuzz-tests-001`.
**Dependência / fluxo / impacto:** parent unit/integration tests exercise overlapping APIs; fuzz is an additional harness, not their replacement.
**Superfície:** `tools/cli/src/{auth,config,lib}.rs`, especialmente
`tools/cli/src/lib.rs:123-167` (`lib_tests`), mais
`tools/cli/tests/integration.rs`.
**Ativação:** parent test selection, currently unexecuted. **Contrato:** tests may assert exact error/default behavior.
**Estado / efeitos:** test-only; no production path. **Falha / propagação:** parent change can leave fuzz stale even if tests pass.
**Contenção:** separate package gate required. **Validação:** run both selections when authorized; no result here. [Relation index](#b03)


<a id="rel-013"></a>
### REL-013 — login config inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-login-config-001`.
**Dependência / fluxo / impacto:** login persists PAT and tenant config; it does not call this fuzz package.
**Superfície:** `tools/cli/src/commands/login.rs:53-69`.
**Ativação:** `login --token`; **Contrato:** parent writes filesystem; helpers are filesystem-free.
**Estado / efeitos:** config file changes; fuzz results do not prove login behavior.
**Falha / propagação:** persistence drift can leave the harness green.
**Contenção/validação:** parent integration tests plus target-specific fuzz run; not executed. [Relation index](#b03)


<a id="rel-014"></a>
### REL-014 — generic PAT resolution inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-resolve-pat-generic-001`.
**Dependência / fluxo / impacto:** generic `run` path calls `resolve_pat`; fuzz calls only the shape helper.
**Superfície:** `tools/cli/src/main.rs:587-590`.
**Ativação:** non-login commands after early dispatch. **Contrato:** env/config I/O and `CliError` are outside the harness.
**Falha / propagação:** auth semantics can drift while shape fuzz remains green. **Validação:** parent auth tests; not run. [Relation index](#b03)


<a id="rel-015"></a>
### REL-015 — config load inverse consumers
**Identidade:** `repo:1232040291:boundary:cli-fuzz-config-load-001`.
**Dependência / fluxo / impacto:** `CorelinkClient::new` reads cached tenant config; fuzz helpers do not touch its path.
**Superfície:** `tools/cli/src/client.rs:99-105`.
**Ativação:** client construction after PAT resolution. **Contrato:** load errors are best-effort here; helper is filesystem-free.
**Falha / propagação:** tenant cache format/path changes affect client behavior beyond fuzz scope. **Validação:** client tests; not run. [Relation index](#b03)


<a id="rel-016"></a>
### REL-016 — config subcommand inverse consumers
**Identidade:** `repo:1232040291:boundary:cli-fuzz-config-command-001`.
**Dependência / fluxo / impacto:** `config set` calls `set_key`; fuzz does not persist config.
**Superfície:** `tools/cli/src/commands/config_cmd.rs:11-12`.
**Ativação:** `config set`; **contrato:** command writes and redaction are parent behavior.
**Falha / propagação:** write dispatch changes can evade fuzz scope. **Validação:** parent config tests; not run. [Relation index](#b03)


<a id="rel-017"></a>
### REL-017 — login PAT validation inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-login-pat-001`.
**Dependência / fluxo / impacto:** login calls `validate_pat_shape` before persistence; this is not `resolve_pat`.
**Superfície:** `tools/cli/src/commands/login.rs:15,48-50`.
**Ativação:** `login --token`; **contrato:** explicit token is validated before `config::save`.
**Falha / propagação:** PAT parser drift changes login acceptance while auth-resolution fuzz may remain green. **Validação:** parent login tests; not run. [Relation index](#b03)


<a id="rel-018"></a>
### REL-018 — doctor config inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-doctor-config-001`.
**Dependência / fluxo / impacto:** doctor aliases `config::load`; fuzz helpers do not inspect permissions or file state.
**Superfície:** `tools/cli/src/doctor.rs:30-32`.
**Ativação:** doctor checks. **Contrato:** load failure and permission behavior remain parent-owned.
**Falha / propagação:** config access drift changes diagnostics outside fuzz scope. **Validação:** doctor tests; not run. [Relation index](#b03)


<a id="rel-019"></a>
### REL-019 — Bazel config inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-bazel-config-001`.
**Dependência / fluxo / impacto:** `bazel-init` loads endpoint/tenant config; fuzz helpers are filesystem-free.
**Superfície:** `tools/cli/src/commands/bazel_init.rs:223-234`.
**Ativação:** `bazel-init`; **contrato:** config path/env fallback is parent behavior.
**Falha / propagação:** endpoint/tenant config drift changes generated credentials/config. **Validação:** Bazel command check; not run. [Relation index](#b03)


<a id="rel-020"></a>
### REL-020 — telemetry config inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-telemetry-config-001`.
**Dependência / fluxo / impacto:** completed commands best-effort load telemetry config; fuzz does not emit telemetry.
**Superfície:** `tools/cli/src/main.rs:520-529`.
**Ativação:** every completed command after `run`; **contrato:** config load precedes opt-in guard and emission decision.
**Falha / propagação:** default/UUID/permission drift changes telemetry decision outside fuzz scope. **Validação:** telemetry tests; not run. [Relation index](#b03)


<a id="rel-021"></a>
### REL-021 — config read inverse consumers
**Identidade:** `repo:1232040291:boundary:cli-fuzz-config-read-001`.
**Dependência / fluxo / impacto:** `config get/list` read parent config and sanitized output; fuzz parses in memory only.
**Superfície:** `tools/cli/src/commands/config_cmd.rs:18-25`.
**Ativação:** `config get/list`; **contrato:** read path and PAT redaction are parent behavior.
**Falha / propagação:** output/redaction drift can evade fuzz scope. **Validação:** config command tests; not run. [Relation index](#b03)


<a id="rel-022"></a>
### REL-022 — whoami config inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-whoami-config-001`.
**Dependência / fluxo / impacto:** whoami refreshes cached tenant config; fuzz does not persist it.
**Superfície:** `tools/cli/src/commands/whoami.rs:38-46`.
**Ativação:** `whoami`; **contrato:** best-effort load/save is parent behavior.
**Falha / propagação:** tenant cache drift can evade harness scope. **Validação:** whoami tests; not run. [Relation index](#b03)


<a id="rel-023"></a>
### REL-023 — audit export auth inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-audit-export-auth-001`.
**Dependência / fluxo / impacto:** production audit export resolves PAT before constructing its client.
**Superfície:** `tools/cli/src/main.rs:763-771`.
**Ativação:** audit export without `--fixture`; **contrato:** auth/network are outside fuzz scope.
**Falha / propagação:** auth drift changes export behavior. **Validação:** audit command check; not run. [Relation index](#b03)


<a id="rel-024"></a>
### REL-024 — audit tail auth inverse consumer
**Identidade:** `repo:1232040291:boundary:cli-fuzz-audit-tail-auth-001`.
**Dependência / fluxo / impacto:** audit tail resolves PAT before constructing its client.
**Superfície:** `tools/cli/src/main.rs:775-783`.
**Ativação:** `audit tail`; **contrato:** auth/network are outside fuzz scope.
**Falha / propagação:** auth drift changes tail behavior. **Validação:** audit command check; not run. [Relation index](#b03)


<a id="rel-025"></a>
### REL-025 — config apply inverse relation
**Identidade:** `repo:1232040291:boundary:cli-fuzz-config-apply-001`.
**Dependência / fluxo / impacto:** `config_cmd::run_apply` calls `config::apply_file`; it reads TOML, flattens leaves, loads config, applies leaves and saves. The fuzz API covers only in-memory mutation.
**Superfície:** `tools/cli/src/commands/config_cmd.rs:36-43`; `tools/cli/src/config.rs:455-469`.
**Ativação:** `config apply --file <path>`; **contrato:** parent owns file I/O, flattening and persistence; API-002 covers mutation.
**Falha / propagação:** file, leaf or persistence drift escapes fuzz. **Validação:** parent valid/invalid TOML and read-back tests; not run. [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — manifest and lockfile resolution
**Identidade:** `repo:1232040291:boundary:cli-fuzz-lock-001`.
**Dependência / fluxo / impacto:** direct deps `libfuzzer-sys`, `arbitrary`, `toml`, `serde_json`, parent resolve via lockfile.
**Superfície:** `Cargo.toml:24-64`, `Cargo.lock:228-260`.
**Ativação:** package resolution/build. **Contrato:** locked graph is expected; exact feature closure not independently resolved.
**State/effect:** dependency artifacts only. **Failure:** version drift can alter parser/security behavior or build availability.
**Containment:** lockfile review and offline metadata. **Validation:** `cargo metadata --locked --offline`; not run. [Relation index](#b03)


<a id="rel-009"></a>
### REL-009 — global fuzz script selection
**Identidade:** `repo:1232040291:boundary:cli-fuzz-script-001`.
**Dependência / fluxo / impacto:** script names five CLI targets but constructs `crates/${crate}` paths.
**Superfície:** `scripts/fuzz-all.sh:27-34,48-51`; expected package is `tools/cli/fuzz`.
**Ativação:** manual script invocation. **Contract:** target list is the sole registry by comment, but path is stale for this package.
**State/effect:** no state unless run. **Failure:** target not found or wrong cwd; no execution evidence.
**Containment:** fix/justify script before using it as wiring proof. **Validation:** shell review only here. [Relation index](#b03)


<a id="rel-010"></a>
### REL-010 — nightly workflow selection boundary
**Identidade:** `repo:1232040291:boundary:cli-fuzz-workflow-001`.
**Dependência / flow / impact:** `fuzz-nightly.yml` matrix lists BYOK, tenant-path, audit-chain and AC, not CLI targets.
**Surface:** `.github/workflows/fuzz-nightly.yml:1-80`.
**Activation:** workflow dispatch; **contract:** matrix selection is explicit and CLI omission is observable.
**State/effect:** no CLI runtime job. **Failure:** assuming nightly coverage hides unrun targets.
**Containment:** record `not wired` until a matching workflow is verified. **Validation:** YAML inspection; no run. [Relation index](#b03)


<a id="rel-011"></a>
### REL-011 — S15 fuzz contract
**Identidade:** `repo:1232040291:boundary:cli-fuzz-spec-001`.
**Dependência / flow / impact:** WI-S15-006 defines 1M, no-panic and redaction criteria; generic `fuzz/fuzz_targets/` and stale rustdoc paths are not this package.
**Surface/inventory:** `_spec_contract.md:203,305`; `sprint.md:41,64`; `WI-S15-006:27,38-39,50-60`; `PRR-S15.md:220`; exact WI path is `specs/04_sprints/_sealed/S15/work_items/WI-S15-006-fuzz-cargo-1m-apple-notarize-authenticode-2-oss-proof-conversion.md`.
**Activation:** contract/review planning. **Contract:** context, not execution evidence.
**State/effect:** documentary. **Failure:** target/source/path mismatch; reconcile exact source/path before approval.
**Validation:** source/spec review; no S15 gate executed. [Relation index](#b03)


<a id="rel-012"></a>
### REL-012 — corpus and crash artifact boundary
**Identidade:** `repo:1232040291:boundary:cli-fuzz-artifacts-001`.
**Dependência / flow / impact:** cargo-fuzz may read corpus and emit crash/leak/timeout artifacts; none are versioned here.
**Surface:** conventional `tools/cli/fuzz/corpus/` and `artifacts/` paths; existence not verified as evidence.
**Activation:** authorized local/CI fuzz run. **Contract:** preserve input, target, command, toolchain and redacted output.
**State/effect:** temporary or retained artifact. **Failure:** deletion loses reproduction; no recovery if sole copy.
**Containment:** copy/hash before minimization. **Validation:** PROC-004; not executed. [Relation index](#b03)


<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho por RELs | Condição | Efeito causal | Contenção / validação |
|---|---|---|---|---|
| `corelink-cli` API | REL-001 → REL-002/003/004/005/006/013/014/015/016/017/018/019/020/021/022/023/024/025 | parent API/config/error changes | compile, acceptance or false-negative drift | parent review plus isolated target |
| CLI auth/config behavior | REL-003/004 → REL-013/014/015/016/017/018/019/020/021/022/023/024/025 | helper semantics changed | fuzz no longer models CLI behavior | integration tests and API review |
| CI fuzz evidence | REL-009/010 → REL-012 | script/workflow run selected | absent/wrong path produces no evidence | verify path, target, run ID and artifact |
| S15 release gate | REL-011 → REL-002–006 | acceptance claim used | 0 panic/leak claim overstated | require target-specific observed evidence |
| JSON input consumers | REL-006 only | config/input shape changes | fuzz parser accepts/rejects different JSON; no output-schema conclusion follows | explicit formatter/schema review; no runtime proof |

**Cobertura:** each direct boundary above has an atomic record. Caminhos
alternativos são parent tests, script selection and workflow matrix; they do not
converge automatically. O package has no proven path to deployed server runtime.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | API / INV / REL afetados | Consumidores / estado | Validação necessária | Coordenação / recuperação |
|---|---|---|---|---|
| Alterar target input split/UTF-8 guard | API-006, INV-001/003, REL-002–006 | corpus compatibility, local process | structural check + target smoke + preserved repro | package owner; restore old input contract if needed |
| Alterar `fuzz_api` signature/semantics | API-001–004, REL-001–006,013/014/017/023/024 | five bins and parent behavior | parent tests, package resolve, target matrix | `corelink-cli` owner; roll forward contract |
| Alterar config/parser dependency | API-001/005, INV-005, REL-008/013/015/016/018/019/020/021/022/025 | TOML/JSON acceptance, config set/get/list/apply and telemetry load, plus lockfile | locked metadata, negative fixtures, config get/list/apply read-back, telemetry/redaction review and target | dependency owner; do not edit lock blindly |
| Add/remove bin | API-006, REL-009/010/011 | registry, CI and S15 count | manifest census, script/workflow reconciliation | CI/spec owner; update all indexes |
| Change redaction scanner | API-004, INV-004, REL-005 | leak detection sensitivity | positive/negative scanner fixtures, target run | security/CLI owner; preserve old scanner evidence |
| Preserve a crash/corpus | REL-012, M05 | artifact and reproduction state | hash, target, command, redacted output | operator authorization; never delete sole repro |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| manifest/targets | 6 | 6 | 0 | 0 |
| direct Cargo deps | 4 named + parent | 1 dependency + API relations | 0 | resolution/features |
| parent API call sites | 4 helpers + config type | fuzz + login/whoami/config set/get/list/apply/client/doctor/Bazel/telemetry/audit consumers | 0 | complete inverse census |
| scripts/workflows | 2 direct surfaces | 2 relations | 0 | run history/owner |
| corpus/artifacts | 0 observed | 1 relation | 0 | existence/retention |
| runtime/deploy | 0 observed | boundary note | none applicable | all runtime reachability |

**Exclusões enumeradas:** deployment, provider, network, production data and
secrets are outside this package and require another owner; no Cargo dependency
was silently omitted. **Diferenças para o censo independente:** the prepared
population calls this the tenth independent fuzz package; package-specific
consumer census and locked feature resolution remain pending. **Não observado:**
any successful build, fuzz run, CI run, release, corpus evolution or runtime.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
