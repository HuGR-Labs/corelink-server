---
name: own-corelink-dt-webhook
description: Maintain source-scoped ownership records for corelink-dt-webhook without representing its handler, alert labels, or fakes as an operated webhook or provider result.
metadata:
  evidence-set: corelink-dt-webhook-source-static-20260921
  source-commit: 398e586ccef712477f2a4ce51e026443b67e5747
  package: corelink-dt-webhook
  manifest: crates/corelink-dt-webhook/Cargo.toml
  profile: S
  evidence: source-static
---

# Own corelink-dt-webhook

Static ownership guide at source commit
`398e586ccef712477f2a4ce51e026443b67e5747`. SOURCE is checked-in manifest,
Rust, and test text, not an observed webhook, endpoint, credential, alert,
provider response, runtime, deployment, or review. The verified canonical
[SBOM/Dependency-Track ADR](../../../docs/knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md)
is a routing reference only; do not copy, revalidate, or redefine it here.

[Scope](#s01) · [Surface](#s02) · [Axioms](#s03) · [Relations](#s04) · [Seams](#s05) · [Unknowns](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Scope and evidence boundary

Own this guide and the three records for `corelink-dt-webhook`. Read its
manifest, local Rust source, and checked-in tests only. Do not inspect secrets,
environment values, Dependency-Track, Slack, email, PagerDuty, an endpoint,
network traffic, queues, metrics backends, deployment, or production state.

<a id="s02"></a>
## S02 — Public static surface

`src/lib.rs` declares `dlq`, `handler`, `hmac`, `metrics`, `severity`, and
`types`; it re-exports `InMemoryDtWebhookHandler` and the public domain types.
`DtWebhookHandler` is an async interface with `handle_webhook` and
`inject_mock_cve`. These names and signatures are source surface, not proof an
HTTP route, a caller, or an implementation outside this crate exists.

<a id="s03"></a>
## S03 — Five source axioms

1. `verify_signature` accepts only a `sha256=`-prefixed decodable value whose computed bytes compare equal through `ConstantTimeEq`; any changed prefix, decode branch, or comparison falsifies this source predicate. 2. For finite inputs, `classify_cvss` calls `score.clamp(0.0, 10.0)` and partitions at `9.0`, `7.0`, `4.0`, and `0.0`; a changed bound or branch falsifies the taxonomy. NaN behavior is unknown from this static source. 3. `routing_channels` maps Critical to three named variants, High to two, Medium

to Slack, and Low/Info to empty; a changed match arm falsifies it. 4. `InMemoryDlq::push` rejects when `len() >= DLQ_CAP`, whose declaration is `1_000`; changing the comparison or constant falsifies the bounded queue. 5. `InMemoryDtWebhookHandler::handle_webhook` verifies its stored signature before patch suppression, classification, and its local channel loop; a reordered branch falsifies this local control-flow statement.

Each axiom is static and falsifiable by the named source edit. It does not
establish reception, authentication, classification, queuing, alerting, or
metric emission in any environment.

<a id="s04"></a>
## S04 — Relation review method

Record one directed source relation at a time: root to exported modules,
handler trait to types, raw bytes to HMAC result, score to severity/channel
vector, handler state to in-memory records, manifest to declared targets, or
the known inverse edges from `tools/dt-cli`, `tools/dt-reconcile`, and
`corelink-ops` to this package. Name the input, local branch/call, output,
impact, and evidence boundary. Stop when a conclusion needs resolution,
invocation, configuration, an additional consumer, or a remote system; report
it as unknown instead.

<a id="s05"></a>
## S05 — Fake and fixture seams

`InMemoryDtWebhookHandler` and `InMemoryDlq` hold `Arc<Mutex<...>>` local
state. The handler's channel routine returns locally; its vectors and
`MetricsSnapshot` are inspectable source fixtures. `[[test]]` and `[[example]]`
entries declare paths only. None proves an example ran, a test passed, a queue
persisted, a webhook arrived, or a channel accepted anything.

<a id="s06"></a>
## S06 — Explicit unknowns

Known inverse source edges: `tools/dt-cli` and `tools/dt-reconcile` declare
this dependency and their `main.rs` sources use the in-memory handler;
`corelink-ops` declares the dependency and re-exports the public API at
`dt::webhook`. Unknown: resolved dependencies/features; additional callers
and trait implementors;
secret origin/rotation and environment values; route registration; parsing of
external payloads; Dependency-Track availability; provider configuration,
acceptance, rate limits, delivery, and incident creation; queue durability;
metrics collection; test execution; deployment; and runtime operation.

<a id="s07"></a>
## S07 — Static handoff

Report the baseline, changed paths, root exports, applicable axiom, direct B
relation, checker and whitespace results actually run, canonical ADR route,
and remaining unknowns. A structural checker and whitespace diff are
documentary checks only; they are not Cargo, test, network, runtime, provider,
deployment, approval, or independent-review evidence.

[Reference](../../../docs/ownership/crates/corelink-dt-webhook/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-dt-webhook/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-dt-webhook/MAINTENANCE.md#m01).
