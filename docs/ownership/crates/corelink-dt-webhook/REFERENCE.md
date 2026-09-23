---
schema: corelink-ownership/1.1
document: reference
package: corelink-dt-webhook
manifest: crates/corelink-dt-webhook/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: draft
evidence_set: corelink-dt-webhook-source-static-20260921
---

# corelink-dt-webhook — reference

This S-profile reference records static manifest and Rust facts. It does not
establish a webhook endpoint or receipt, secrets, Dependency-Track operation,
provider request/acceptance/delivery, queue persistence, metrics collection,
deployment, test execution, or runtime behavior. The verified canonical
[SBOM/Dependency-Track ADR](../../../knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md)
is a route only and is neither repeated nor revalidated here.

[Identity](#r01) · [Root](#r02) · [Types/trait](#r03) · [HMAC](#r04) · [Severity](#r05) · [Handler](#r06) · [DLQ/metrics](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Package identity and evidence boundary

`crates/corelink-dt-webhook/Cargo.toml` names `corelink-dt-webhook` and
declares direct dependencies `thiserror`, `serde`, `serde_json`, `hmac`,
`sha2`, `hex`, `tracing`, `async-trait`, and `subtle`; it declares `proptest`,
`tokio`, and `tracing-subscriber` for development. It also declares two test
targets and three examples. These are selection/path declarations, not proof
that any dependency resolves, target compiles, test/example runs, or system
communicates.

<a id="r02"></a>
## R02 — Root module and export contract

`src/lib.rs` forbids unsafe code, declares six public modules, re-exports
`InMemoryDtWebhookHandler`, and re-exports the public types listed in its
`pub use types` block. It defines the `DtWebhookHandler: Send + Sync` trait
with async `handle_webhook` and `inject_mock_cve` signatures. Falsifiable
invariant: adding, removing, or renaming a declaration, export, method, or
parameter changes this source-visible Rust surface.

<a id="r03"></a>
## R03 — Domain type and error relation

`types.rs` defines the opaque `DtProjectUuid`, event/component/vulnerability
structures, non-exhaustive event/severity/channel enums, result structures,
and the non-exhaustive `DtWebhookError`. `DtProjectUuid::new` rejects an empty
or whitespace-only string. Falsifiable invariant: changing that guard, any
public field/type, enum arm, or error variant changes the local construction or
matching contract. Type/comment text does not prove a real project, event, or
error condition.

<a id="r04"></a>
## R04 — HMAC source relation

`hmac::verify_signature` strips `sha256=`, hex-decodes the remainder, computes
HMAC-SHA256 over its supplied secret/body bytes, and uses `ct_eq` before
returning `Ok(())`; each failure maps to `DtWebhookError::HmacInvalid`.
`sign` formats a computed value with the same prefix. Falsifiable invariant:
changing the prefix, algorithm/type alias, failure mapping, or comparison
branch changes this local byte-processing contract. It is not evidence a
request header, secret, or validation event existed.

<a id="r05"></a>
## R05 — Severity and channel-vector relation

For finite supplied values, `classify_cvss` calls
`score.clamp(0.0, 10.0)`, then selects Critical at `>=9.0`, High at `>=7.0`,
Medium at `>=4.0`, Low at `>0.0`, otherwise Info. NaN behavior is unknown from
this static source.
`routing_channels` returns the named channel vector for each severity: three,
two, one, or empty respectively. Falsifiable invariant: a threshold, branch,
or vector-arm change alters source classification/routing output. No remote
alert is inferred from those values.

<a id="r06"></a>
## R06 — In-memory handler relation

`InMemoryDtWebhookHandler` stores its secret/signature, delivery vector, DLQ,
metric snapshot, and mock flag in `HandlerState` behind `Arc<Mutex<_>>`; its
injected clock is an `Arc<dyn Fn() -> SystemTime + Send + Sync>`, not
mutex-guarded state. `handle_webhook` returns `Result<AlertDelivered,
DtWebhookError>`: its poisoned-state, serialization, missing/invalid-signature,
and local patch-suppression exits return `Err`.

After the remaining local branches serialize, verify, classify/route, invoke
`deliver_to_channel`, and update the local snapshot/vector state, its
successful outcome is
`Ok(AlertDelivered)`. `inject_mock_cve` returns `MockInjectionDisabled` when
its stored flag is false. These are source branches and in-memory seams, not
an external handler, delivery, SLA observation, or staging outcome.

<a id="r07"></a>
## R07 — DLQ and metric declaration relation

`InMemoryDlq` wraps a `VecDeque<DlqEntry>` in `Arc<Mutex<_>>`; `push` returns
`DlqCapacityExceeded` before insertion when length is at least `DLQ_CAP`.
`metrics.rs` declares six metric-name constants, `SLA_BUDGET_MS = 900_000`,
`DLQ_CAP = 1_000`, six bucket values, and `MetricsSnapshot`. Falsifiable
invariant: changing a cap, constant literal, collection operation, or snapshot
field changes local source representation. This is not proof of durable DLQ or
observed metric data.

<a id="r08"></a>
## R08 — Evidence limits and explicit unknowns

Known inverse source edges: `tools/dt-cli` and `tools/dt-reconcile` declare a
direct dependency and their `src/main.rs` sources import APIs and construct/use
the in-memory handler; `crates/corelink-ops` declares the dependency and
re-exports the crate from `src/dt.rs` as `corelink_ops::dt::webhook`. These
manifest/import/re-export facts do not establish target selection,
compilation, invocation, or reachability.

Unknown: resolved graph/features/targets; additional consumers and trait
selection; secret source, value, rotation, and environment access; endpoint
registration; external payload parsing; Dependency-Track state; HTTP/DNS/TLS;
Slack/email/PagerDuty configuration, requests, acceptance, rate limiting, or
delivery; queue persistence/replay; metrics backend/values; test execution;
deployment; and runtime operation. The canonical ADR remains a route only; it
does not fill any unknown from this package's static evidence.

[Ownership guide](../../../../.claude/skills/own-corelink-dt-webhook/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
