---
schema: corelink-ownership/1.1
document: reference
package: corelink-dt-cli
manifest: tools/dt-cli/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: draft
evidence_set: dt-cli-static-source-20260921
---

# corelink-dt-cli — ownership reference

Static-source reference only. It does not prove a Dependency-Track or OSS
Index request, CVE transfer, credential validity, handler effect, alert,
deployment, or runtime. The verified [OKF SRE operations hub](../../../knowledge/ops/sre-operations-hub.md)
is canonical routing context only; it is not copied, redefined, or revalidated
here.

[Identity](#r01) · [Boundary](#r02) · [Dispatch](#r03) · [Injection](#r04) · [Fallback](#r05) · [Helpers](#r06) · [Logs/exits](#r07) · [Limits](#r08).

<a id="r01"></a>
## R01 — Identity and static evidence

The manifest names `corelink-dt-cli`, declares the `corelink-dt-webhook`,
`thiserror`, serde, tracing, and Tokio workspace dependencies, and declares
the `corelink-dt-cli` binary at `src/main.rs`. The source is a binary entry
point; no `src/lib.rs` is present. A manifest dependency, binary path, or
source-file inventory change falsifies this record. Evidence:
`tools/dt-cli/Cargo.toml`; `tools/dt-cli/src/main.rs`.

<a id="r02"></a>
## R02 — Ownership boundary and five axioms

The package owns local command selection, flag/environment precedence,
severity-to-score mapping, construction of local input values, logging, and
process exit selection. It does not own a provider/API client, a Dependency-
Track or OSS Index endpoint, transport, stored data, caller, credential
management, alert delivery, deployment, or observed execution.

Five axioms: (1) manifest/source prove static declarations only; (2) a
dependency call proves neither the dependency's effect nor a real request; (3)
source order proves neither delivery nor cross-system atomicity; (4)
environment reads prove neither a supplied secret nor credential validity; (5)
the linked verified OKF hub is canonical routing context only and is neither
copied nor redefined here.

Falsifier: a source-visible client, transport, runtime adapter, or additional
entry surface in this package changes the boundary. Evidence:
`tools/dt-cli/Cargo.toml`; `tools/dt-cli/src/main.rs`.

<a id="r03"></a>
## R03 — Entry and dispatch invariant

`main` initializes the tracing formatter, collects `std::env::args`, and exits
`1` after usage output when fewer than two arguments are present. Its selector
routes only `"inject-mock"` to `run_inject_mock` and `"ossindex-fallback"` to
`run_ossindex_fallback`; every other selector logs an error and exits `1`.

Falsifier: an additional selector, a changed minimum-argument predicate, or a
different exit branch changes the local dispatch contract. This identifies no
shell caller or command invocation. Evidence: `tools/dt-cli/src/main.rs`.

<a id="r04"></a>
## R04 — Mock-injection input and outcome invariant

`run_inject_mock` selects `--project` before `DT_PROJECT_UUID`; absence of
both logs and exits `1`. It defaults severity to `critical`, maps case-folded
`critical`, `high`, `medium`, and `low` to `9.5`, `7.5`, `5.0`, and `2.0`, and
maps every other input to `9.5`. When `DtProjectUuid::new` returns `Err`, its
local closure logs and exits `1`; this source branch does not establish UUID
validity.

`DT_MOCK_INJECTION_ENABLED` enables only when exactly `"true"`; absent
`DT_WEBHOOK_SECRET` selects the source-visible `dev-secret` bytes. The function
constructs a fixed-ID `SyntheticCve`, calls `inject_mock_cve`, then exits `0`
for `sla_met`, `2` otherwise, or `1` for an error.

Falsifiers: `--project` versus environment disagreement, severity `LOW`, an
unknown severity, `"TRUE"`, a `DtProjectUuid::new` `Err` branch, and each
returned result branch distinguish the predicates. The dependency call proves
no injection, alert, or measured SLA; UUID validity remains unknown. Evidence:
`tools/dt-cli/src/main.rs`.

<a id="r05"></a>
## R05 — Fallback stub invariant

`run_ossindex_fallback` ignores its argument slice, emits three informational
messages, and exits `0`. Its comments and log text describe hypothetical
production behavior, but the function contains no source-visible provider
client call or data-transfer statement.

Falsifier: adding a client call, branching on input, or changing the immediate
exit changes this record. Comment text is not execution evidence. Evidence:
`tools/dt-cli/src/main.rs`.

<a id="r06"></a>
## R06 — Helper predicates invariant

`get_flag` returns the element after the first two-element window whose first
element equals the requested flag; otherwise it returns `None`. The severity
helper lowercases before its four named mappings and uses `9.5` as its default.

Falsifiers: a flag without a following window element, repeated flags, mixed-
case `HiGh`, and an unrecognised severity distinguish this local parsing and
mapping. Evidence: `tools/dt-cli/src/main.rs`.

<a id="r07"></a>
## R07 — Logging and exit boundary

The visible success/failure signals are process exits and tracing/error output:
missing/unknown/injection error exit `1`, an injection result without
`sla_met` exits `2`, successful injection exits `0`, and the fallback stub
exits `0`. Logs contain local fields including project UUID, CVE ID, score, or
reported latency where the source passes them.

Falsifier: changing an exit literal, branch, or logged local field changes the
local interface. Logs do not establish a collected sink, recipient, or
observed event. Evidence: `tools/dt-cli/src/main.rs`.

<a id="r08"></a>
## R08 — Unknowns and closure

Material unknowns: whether any caller executes this binary and with which
arguments/environment; the dependency implementation's effects, provider
connectivity, credential handling, and alert behavior; and any actual OSS
Index lookup or data transfer. Also unknown: feature selection, deployment,
traffic, runtime results, and unrun tests.

Success: every implementation statement R01–R07 has a falsifier. Completeness:
R01–R07 reconcile with B01–B07. Quality: the five axioms and unknowns remain
explicit. Definition of Done: the four static ownership artifacts and their
structural results are handed off; no operational result or approval follows.
[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
