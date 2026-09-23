---
name: own-corelink-dt-cli
description: >-
  Route static ownership changes to the corelink-dt-cli binary's argument,
  environment, severity, exit-status, and webhook-handler call boundaries.
metadata:
  schema: "corelink-ownership/1.1"
  profile: "S"
  package: "corelink-dt-cli"
  manifest: "tools/dt-cli/Cargo.toml"
  source-commit: "398e586ccef712477f2a4ce51e026443b67e5747"
  evidence-set: "dt-cli-static-source-20260921"
---

# Ownership — corelink-dt-cli

Static-source routing guide. It establishes no Dependency-Track or OSS Index
request, CVE transfer, credential validity, alert delivery, deployment, or
runtime execution. The verified [OKF SRE operations hub](../../../docs/knowledge/ops/sre-operations-hub.md)
is canonical routing context only; this record neither copies nor redefines it.

[Trigger](#s01) · [Boundary](#s02) · [Dispatch](#s03) · [Inject](#s04) · [Fallback](#s05) · [Handoff](#s06) · [Closure](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Manifest, `main.rs`, arguments, environment reads, or exit handling changes | Route to these four static ownership records | `tools/dt-cli/Cargo.toml`; `tools/dt-cli/src/main.rs` | A real provider/API request, credential, alert, or execution result is requested |

<a id="s02"></a>
## S02 — Boundary and axioms

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Scope is uncertain | Retain only the binary declaration, local parsing, values, logs, exit paths, and dependency call boundary | [R02](../../../docs/ownership/crates/corelink-dt-cli/REFERENCE.md#r02) | Infer a transfer, provider behavior, caller, or runtime result from a name or comment |

Five axioms: (1) manifest and source establish declarations only; (2) a
dependency call establishes neither its external implementation nor an actual
request; (3) local source order establishes neither delivery nor distributed
atomicity; (4) environment reads establish neither secret presence nor
credential validity; (5) the linked verified OKF hub is canonical routing
context only and is not copied, redefined, or revalidated here.

<a id="s03"></a>
## S03 — Command dispatch

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Subcommand or argument handling changes | Preserve missing-argument and unknown-command exit `1`, plus the two exact selectors | [R03](../../../docs/ownership/crates/corelink-dt-cli/REFERENCE.md#r03), [B01](../../../docs/ownership/crates/corelink-dt-cli/BLAST_RADIUS.md#b01) | Claim shell invocation, authorization, or a caller from parsing alone |

<a id="s04"></a>
## S04 — Mock-injection path

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| `inject-mock`, project/severity parsing, environment handling, synthetic value, handler construction, or result branch changes | Trace precedence, exact severity mapping, the `DtProjectUuid::new` `Err` branch to exit `1` (without asserting UUID validity), the `"true"` gate, fixed synthetic CVE, and separately the returned-result-to-`0`/`2`/`1` mapping | [R04](../../../docs/ownership/crates/corelink-dt-cli/REFERENCE.md#r04), [B02](../../../docs/ownership/crates/corelink-dt-cli/BLAST_RADIUS.md#b02)–[B05](../../../docs/ownership/crates/corelink-dt-cli/BLAST_RADIUS.md#b05) | Claim that a CVE was injected, an alert occurred, a secret is valid, or an SLA was observed |

<a id="s05"></a>
## S05 — Fallback stub boundary

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| `ossindex-fallback` changes | Preserve the source-visible log-and-exit-`0` behavior and distinguish comments from implementation | [R05](../../../docs/ownership/crates/corelink-dt-cli/REFERENCE.md#r05), [B06](../../../docs/ownership/crates/corelink-dt-cli/BLAST_RADIUS.md#b06) | Treat a comment describing production behavior as a provider request or data transfer |

<a id="s06"></a>
## S06 — Static handoff

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Static assessment is complete | Report baseline, changed local contract, atomic relations, structural checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-dt-cli/MAINTENANCE.md#m06) | Call documentary checking execution, semantic approval, or production proof |

<a id="s07"></a>
## S07 — Closure

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Record is handed off | Keep all five axioms and unresolved external claims visible | [R08](../../../docs/ownership/crates/corelink-dt-cli/REFERENCE.md#r08) | Turn a static handoff into a claim of runtime completion |

[Reference](../../../docs/ownership/crates/corelink-dt-cli/REFERENCE.md#r01) · [Impact map](../../../docs/ownership/crates/corelink-dt-cli/BLAST_RADIUS.md#b01) · [Start](#s01)
