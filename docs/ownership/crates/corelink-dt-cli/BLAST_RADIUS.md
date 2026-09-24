---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dt-cli
manifest: tools/dt-cli/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: draft
evidence_set: dt-cli-static-source-20260921
---

# corelink-dt-cli — blast radius

Atomic static relations only. Each relation identifies a review boundary, not
a command invocation, provider/API request, CVE transfer, credential use,
alert delivery, deployment, or runtime behavior.

[Arguments](#b01) · [Project](#b02) · [Severity](#b03) · [Handler inputs](#b04) · [Result exits](#b05) · [Fallback](#b06) · [Binary](#b07).

<a id="b01"></a>
## B01 — Arguments-to-dispatch relation

`std::env::args` → second argument selector → `run_inject_mock` or
`run_ossindex_fallback`, with missing or unknown selection exiting `1`.
Impact: changing the selectors or boundary changes reachable local paths.
Falsifier: fewer than two arguments and an unrecognised selector select the
two error branches. Evidence mode: STATIC_SOURCE,
`tools/dt-cli/src/main.rs`. Unknown: caller identity and invocation.

<a id="b02"></a>
## B02 — Project-input-to-UUID relation

`--project` value or `DT_PROJECT_UUID` → `DtProjectUuid::new` → local handler
call input. The flag has precedence; no value exits `1`. When
`DtProjectUuid::new` returns `Err`, its local closure logs and exits `1`; this
does not establish UUID validity. Impact: changing precedence or the local
`Err` branch changes the local value/exit boundary. Falsifier: conflicting
flag/environment values and a `DtProjectUuid::new` `Err` branch distinguish
the paths. Evidence mode: STATIC_SOURCE, `tools/dt-cli/src/main.rs`. Unknown:
value provenance, UUID validity, and dependency effect.

<a id="b03"></a>
## B03 — Severity-to-synthetic-CVE relation

`--severity` or default → `severity_to_cvss` → `SyntheticCve.cvss_score`.
Impact: changing case folding, thresholds, default, or fixed synthetic fields
changes the local call input. Falsifier: `low`, `HIGH`, and an unknown severity
select `2.0`, `7.5`, and `9.5`. Evidence mode: STATIC_SOURCE,
`tools/dt-cli/src/main.rs`. Unknown: a CVE's acceptance, storage, or alerting.

<a id="b04"></a>
## B04 — Gate-and-secret-to-handler-construction relation

`DT_MOCK_INJECTION_ENABLED` and `DT_WEBHOOK_SECRET` → local boolean/byte vector
→ `InMemoryDtWebhookHandler::new`. Only exact `"true"` enables the boolean;
absent secret selects `dev-secret` bytes. Impact: changing either predicate
changes local constructor input. Falsifier: absent environment and `"TRUE"`
distinguish the local branches. Evidence mode: STATIC_RELATION,
`tools/dt-cli/src/main.rs`. Unknown: secret validity, handler internals,
provider behavior, and actual injection.

<a id="b05"></a>
## B05 — Returned-result-to-exit relation

`inject_mock_cve` awaited return → local `match`/`sla_met` branch → exit `0`,
`2`, or `1`. Impact: changing this mapping changes local process-exit
selection. Falsifier: `Ok` with `sla_met`, `Ok` without it, and `Err` select
the three local exits. Evidence mode: STATIC_SOURCE,
`tools/dt-cli/src/main.rs`. This is no input-to-runtime causality claim:
constructor inputs, gate, and secret do not establish the returned result or
runtime behavior. Unknown: handler effect, provider behavior, actual
injection, and observed SLA.

<a id="b06"></a>
## B06 — Fallback-selector-to-stub relation

`"ossindex-fallback"` → `run_ossindex_fallback` → three local log calls → exit
`0`. Impact: changing the function can change only the source-visible stub
surface until a visible implementation is added. Falsifier: the current body
has no provider client call and does not use its argument slice. Evidence mode:
STATIC_SOURCE, `tools/dt-cli/src/main.rs`. Unknown: any real lookup, endpoint,
request payload, response, or data transfer.

<a id="b07"></a>
## B07 — Workspace-member-to-binary relation

root `Cargo.toml` `[workspace].members` entry `tools/dt-cli` → package manifest
`[[bin]]` → `src/main.rs` binary target named `corelink-dt-cli`. Impact: a
member, target name, or path change alters this static build-target relation.
Falsifier: removing the root workspace-members entry or target declaration
disproves it. Evidence mode: STATIC_RELATION, root `Cargo.toml` `[workspace]`
`members` entry; `tools/dt-cli/Cargo.toml`. Unknown: selected features, build,
installation, execution, and all consumers.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
