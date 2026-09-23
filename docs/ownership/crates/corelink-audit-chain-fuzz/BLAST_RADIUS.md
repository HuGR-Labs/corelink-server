---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-audit-chain-fuzz
manifest: crates/corelink-audit-chain/fuzz/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: "S"
state: draft
evidence_set: corelink-audit-chain-fuzz-structural-normalization-20260921
---

# corelink-audit-chain-fuzz — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) ·
[Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Leitura rápida e escopo

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-009](#rel-009) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008)

The package boundary is a separate Cargo workspace with two fuzz bins and six direct dependencies. Its two local invocation surfaces are a shell target list and a manually dispatched workflow whose schedule is commented out at the pin. This is a static map of harness/configuration text; no target, job, production path, corpus, or runtime was executed or observed.

**Builds assessed:** declared bins `merkle_append` and `jcs_canonicalize`, without resolved features. **Environments not observed:** local fuzz execution, self-hosted runner state, CI results, and runtime. Package source pin: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`, matching this artifact's front matter; integration parent: `d80f245e0be3c61c250249de8292e42a6cd8ef5d`.

<a id="b02"></a>
## B02 — Inventário e método

| População | Fontes / método | Seleção / revisão | Limite do levantamento |
|---|---|---|---|
| Deps declaradas | `fuzz/Cargo.toml:24-30` | Seis normal deps; pin F01 | Declaração, não resolução |
| Inversas Cargo | `git grep` exato do package/target nos arquivos textuais do pin | Package literal ocorre no próprio manifest; targets também em dois invocadores | Cargo metadata/grafo não executado |
| Fora de Cargo | Busca literal no pin por `corelink-audit-chain-fuzz`, `merkle_append`, `jcs_canonicalize` | Shell e workflow analisados; perf-name hits classificados abaixo | Não descobre binding dinâmico nem prova execução |
| Build, operação e teste | `scripts/fuzz-all.sh`; `.github/workflows/fuzz-nightly.yml` | F04/F05, linhas anotadas em R08 | Sem CI, provider, logs ou resultado observado |

**Relação cross-package:** use o identificador de repositório verificado
`repo:1232040291`. Cada relação abaixo tem fingerprint qualificado; relações
`REL-001`/`REL-002` dependem de crates peer. As linhas dos providers agora
registram as mesmas chaves e superfícies SOURCE; owner humano continua
`UNKNOWN`, e resolução/execução permanecem não observadas. A reconciliação de
identidade não deve ser confundida com aprovação de runtime.

<a id="b03"></a>
## B03 — Registro de relações diretas

| ID | Tipo / direção | Superfície | Ativação | Owner do contrato |
|---|---|---|---|---|
| [REL-001](#rel-001) | dependency | `corelink-audit-chain` path | `merkle_append` | Crate provedora |
| [REL-002](#rel-002) | dependency | `corelink-analytics` path | `merkle_append` | Crate provedora |
| [REL-003](#rel-003) | dependency / payload | `serde_json::json!` | `merkle_append` | Upstream package |
| [REL-004](#rel-004) | dependency | `serde_jcs` registry | `jcs_canonicalize` | Upstream package |
| [REL-005](#rel-005) | dependency | `uuid` registry | `merkle_append` | Upstream package |
| [REL-006](#rel-006) | dependency | `libfuzzer-sys` registry | Ambos os bins | Upstream package |
| [REL-007](#rel-007) | build-deploy | `scripts/fuzz-all.sh` | Seleção no script | Script owner unknown |
| [REL-008](#rel-008) | build-deploy | `.github/workflows/fuzz-nightly.yml` | `workflow_dispatch` | Workflow/operator unknown |
| [REL-009](#rel-009) | dependency / parser | `serde_json::from_slice::<Value>` | `jcs_canonicalize` | Upstream package |

<a id="rel-001"></a>
### REL-001 — Hash-chain harness dependency
**Shared identity:** `repo:1232040291:boundary:audit-fuzz-chain-api-001`; provider-side backlink: [corelink-audit-chain B03](../corelink-audit-chain/BLAST_RADIUS.md#b03). **Directions:** dependency `corelink-audit-chain-fuzz→corelink-audit-chain`; data flows harness event→provider append→optional hash/error; impact provider contract change→harness, target failure→invoker.
**Surface:** `HashChainBuilder`, `AuditEvent`, `AuditEventKind`, `ChainHash`; `merkle_append.rs:24-27,94-129`.
**Activation / contract:** target only; path `..`; calls constructors, `append`, `genesis`.

**State / failure:** synthetic in-memory values; `append Err` becomes `None`; no persistence is established.
**Containment / validation:** assertion scope is [INV-001–002](REFERENCE.md#r05); [PROC-001](MAINTENANCE.md#proc-001). No production propagation established.
**Coordination / source:** API contract owner is parent crate; human owner unknown; manifest/target blobs F01/F02 in [R08](REFERENCE.md#r08).
**Reconciliation:** provider-side B03 now records the same key, source endpoints, and `HashChainBuilder`/`AuditEvent` surface. Manifest resolution and target execution remain UNKNOWN. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Region type dependency
**Shared identity:** `repo:1232040291:boundary:audit-fuzz-region-api-001`; provider-side backlink: [corelink-analytics B05](../corelink-analytics/BLAST_RADIUS.md#b05). **Directions:** dependency `corelink-audit-chain-fuzz→corelink-analytics`; data provider→consumer (`Region::Iad` value); impact API change→harness, target failure→invoker.
**Surface:** `corelink_analytics::Region`; `merkle_append.rs:24,59`.
**Activation / contract:** target only; manifest path `../../corelink-analytics`; harness selects one enum variant.

**State / failure:** value is constructed in the harness; no analytics operation or external call is visible in this target.
**Containment / validation:** compile/fuzz failure remains target evidence; [PROC-001](MAINTENANCE.md#proc-001). Runtime use unknown.
**Coordination / source:** contract owner is `corelink-analytics`; human owner unknown; F01/F02 in [R08](REFERENCE.md#r08).
**Reconciliation:** provider-side B05 now records the same key, source endpoints, and `Region::Iad` value surface. Manifest resolution and target execution remain UNKNOWN. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — JSON parsing and value construction
**Identity:** `repo:1232040291:boundary:audit-fuzz-json-payload-001`. **Directions:** dependency `fuzz→serde_json`; data `json!` value→`AuditEvent` payload; impact serde/value-shape change→merkle assertions.
**Surface:** `serde_json::json!({"seq": seq, "seed": payload_seed})`; `merkle_append.rs:28,62`.
**Activation / contract:** `merkle_append` only; declared serde_json requirement `1` in F01.

**State / failure:** per-iteration memory; invalid JSON exits the JCS target early.
**Containment / validation:** no durable state observed; [PROC-001](MAINTENANCE.md#proc-001). Resolver version unknown.
**Coordination / source:** external contract owner unknown; F01–F03 in [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — JCS canonicalizer dependency
**Identity:** `repo:1232040291:boundary:audit-fuzz-jcs-serializer-001`. **Directions:** dependency `fuzz→serde_jcs`; data `Value`→canonical bytes/result; impact dependency change→JCS assertions, failure→invoker.
**Surface:** `serde_jcs::to_vec`; `jcs_canonicalize.rs:33-56`.
**Activation / contract:** only `jcs_canonicalize`; declared requirement `0.2`, resolved version unknown.

**State / failure:** canonical byte vectors live for one iteration; first `Err` returns without assertion.
**Containment / validation:** fixed-point and repeated-output asserts are [INV-003](REFERENCE.md#inv-003); [PROC-001](MAINTENANCE.md#proc-001).
**Coordination / source:** external contract owner unknown; F01/F03 in [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — JSON parser for JCS input
**Identity:** `repo:1232040291:boundary:audit-fuzz-json-parser-001`. **Directions:** dependency `fuzz→serde_json`; data bytes→`serde_json::Value`; impact parser/version change→which inputs reach JCS.
**Surface:** `serde_json::from_slice::<serde_json::Value>`; `jcs_canonicalize.rs:24-28,39-40`.
**Activation / contract:** `jcs_canonicalize` only; invalid JSON returns before JCS.
**State / failure:** `Value` is per-iteration; parser error is an expected early return, not a finding.
**Containment / validation:** [INV-003](REFERENCE.md#inv-003); [PROC-001](MAINTENANCE.md#proc-001); resolver unknown.
**Coordination / source:** external contract owner unknown; F01/F03 in [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — UUID construction dependency
**Identity:** `repo:1232040291:boundary:audit-fuzz-uuid-001`. **Directions:** dependency `fuzz→uuid`; data provider→harness UUID values, then harness→audit-chain constructors; impact API change→merkle target.
**Surface:** `Uuid::from_u128`; `merkle_append.rs:27,54-58,73-74`.
**Activation / contract:** only `merkle_append`; declared `v4`,`v7` features although the inspected source calls `from_u128`.

**State / failure:** deterministic synthetic tenant/event identifiers; random generation is not shown.
**Containment / validation:** input-derived construction only; [PROC-001](MAINTENANCE.md#proc-001). Feature necessity/resolution unknown.
**Coordination / source:** external contract owner unknown; F01/F02 in [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — libFuzzer target interface
**Identity:** `repo:1232040291:boundary:audit-fuzz-libfuzzer-001`. **Directions:** dependency `fuzz→libfuzzer-sys`; data provider→harness byte slice and harness→provider process/assertion outcome; impact macro/runtime dependency change→both bins.
**Surface:** `fuzz_target!` in both target files.
**Activation / contract:** declared `0.4`; each bin is a fuzz target, not test/doc/bench.

**State / failure:** bytes are interpreted locally; panic/assertion is a target failure signal, not production evidence.
**Containment / validation:** selected target only; [PROC-001](MAINTENANCE.md#proc-001). Exact resolved libFuzzer/build-host behavior unknown.
**Coordination / source:** external contract owner unknown; F01–F03 in [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Repository fuzz selector
**Identity:** `repo:1232040291:boundary:audit-fuzz-script-001`. **Directions:** Cargo dependency `not applicable`; data script→package (target/duration args); impact selector change→selected target, target exit→script result.
**Surface:** target pairs in `scripts/fuzz-all.sh:24-45`; execution at `:54-68`.
**Ativação / contract:** explicit shell invocation; runs `cargo +nightly fuzz run`, default 300 seconds.

**State / failure:** local command output; nonzero enters failure list and final nonzero exit.
**Containment / validation:** [PROC-001](MAINTENANCE.md#proc-001); script itself is not run here.
**Coordination / source:** script owner unknown; F05 at [R08](REFERENCE.md#r08); no human escalation verified. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Manual fuzz workflow matrix
**Identity:** `repo:1232040291:boundary:audit-fuzz-workflow-001`. **Directions:** Cargo dependency `not applicable`; workflow→package (matrix names/config); impact config→job selection, target failure→job/artifact path.
**Surface:** workflow matrix `:84-93`, invocation `:169-183`.
**Ativação / contract:** `workflow_dispatch`; schedule lines are commented/parked at pin; target duration is 1800s in workflow env.

**State / failure:** config requests cache by target and failure artifact upload; no actual cache, job, or artifact observed.
**Containment / validation:** not triggered; no CI/host action allowed in this task. Runner/operator unknown.
**Coordination / source:** workflow owner unknown; F04 at [R08](REFERENCE.md#r08); escalation route not verified. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho por RELs | Condição | Efeito causal | Contenção / validação |
|---|---|---|---|---|
| `corelink-audit-chain` API contract | REL-001 ← `merkle_append` | Target built/run | Provider API/source drift can invalidate harness compile or assertions | Stop at target; [PROC-001](MAINTENANCE.md#proc-001) |
| `corelink-analytics` type contract | REL-002 ← `merkle_append` | Target built/run | `Region` contract drift can invalidate target | Stop at target; runtime not established |
| Local fuzz selector | REL-007 → REL-001–006,009 | Script explicitly invoked | Selected source/dependency failure becomes script result | Do not infer production effect |
| Fuzz workflow | REL-008 → REL-001–006,009 | Manual dispatch after current workflow checks | Matrix job config refers to targets, cache and artifact paths | No schedule/run evidence; operator unknown |

**Coverage:** only static target→dependency and invoker paths visible in inspected files. No server, route, storage or database path is inferred. Other transitive paths remain unknown.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | REL / invariant | Consumidor / estado | Validação requerida | Coordenação / recuperação |
|---|---|---|---|---|
| Input mapping/assertion for one target | Corresponding dependency REL and INV | Selected harness only; ephemeral input | Single target in [M04](MAINTENANCE.md#m04) | Preserve corpus/artifact; no production claim |
| Target/dependency/bin declaration | REL-001–006,009 | One or both bins, per manifest use | Run each affected target in M04; resolve graph separately if needed | Review imported contract with provider owner |
| `fuzz-all.sh` target list | REL-007 | Shared script selects target on explicit invocation | Static diff review plus authorized targeted command | Script owner unknown; no runner triggered |
| Workflow matrix/toolchain/cache | REL-008 | Dispatch job config | Config review; do not dispatch as a doc check | Workflow/operator unknown; no schedule change |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| Package tracked paths | 3 | 3 | 0 | 0 within pinned tree |
| Direct declared dependencies | 6 | 6 | 0 | Resolved graph |
| Static in-scope relations | 9 | 9 | 0 | REL-001/002 identities/source surfaces aligned; owner, resolution and runtime unknown |
| Exact target-name hits outside package | 2 invoking files | 2 | Perf/bench name hits | Dynamic invocation/runtime |

**Exclusions:** `.github/workflows/perf-regression.yml`, `scripts/run-perf-baseline.sh`, `scripts/refresh-perf-baseline.sh`, `reports/perf/**`, benchmark source, and changelog also contain target-name strings. Parent `corelink-audit-chain/Cargo.toml` declares benchmark targets with these names; a literal hit does not establish fuzz-package consumption.

Runtime, job history, artifact contents, unrelated dynamic bindings, peer reconciliation and escalation owners remain unknown. No independent cargo census was run.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
