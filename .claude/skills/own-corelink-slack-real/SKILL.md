---
name: own-corelink-slack-real
description: Own the source-visible corelink-slack-real interface, routing, rendering, retry, audit, adapter, and in-memory fake contracts without asserting provider operation.
metadata:
  evidence-set: slack-real-source-static-20260920
  source-commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
  package: corelink-slack-real
  manifest: crates/corelink-slack-real/Cargo.toml
  profile: S
---

# Own corelink-slack-real

Static ownership guide at source commit `6be030999de1f0e0fe62d3a9abb04ec2a4fefde6`.
SOURCE is not a credential read, Slack request, provider response, runtime
observation, deployment, or review. Route operational context only to the
verified [SRE operations hub](../../../docs/knowledge/ops/sre-operations-hub.md);
do not copy, redefine, or revalidate its policy here.

[Scope](#s01) · [Surface](#s02) · [Axioms](#s03) · [Routing](#s04) · [Seams](#s05) · [Unknowns](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Scope and evidence boundary

Own this crate and manifest. Read checked-in source only. Do not inspect webhook
values or infer credentials, delivery, provider acceptance, audit durability,
consumer selection, or deployment.

Activate for manifest, API, registry, rendering, retry, audit, or inquiry
adapter changes. Do not activate for generic Slack questions, provider setup,
credential rotation, or sending messages. Route operations to the verified
owner; stop if authority or provider evidence is missing.

Non-activating examples: unrelated providers, policy lookup, or a consumer-only
change. For crate work, read Reference → matching Blast Radius REL → Maintenance
PROC. Stop before secret access, network/provider operation, or delivery/runtime
claims; record the unknown and escalate.

<a id="s02"></a>
## S02 — Public source surface

Trace root re-exports to `adapter`, `audit`, `channel`, `client`, `http`,
`memory`, `message`, `redact`, `retry`, and `template`. `SharedSlackClient`
and `SlackAuditSink` are interfaces; `SlackHttpClient` and
`InMemorySharedSlackClient` are source-defined implementations. Evidence:
`src/lib.rs`, `src/client.rs`, and `src/audit.rs`.

<a id="s03"></a>
## S03 — Five source axioms

1. `SlackChannel::all()` returns six named variants, each with a distinct literal channel string and environment-variable name in `channel.rs`. 2. `WebhookRegistry::from_env()` omits unset or empty values; `url()` returns `ChannelUnconfigured` for an absent routing key. 3. `SlackMessage::new` truncates headers to 150 characters, while JSON rendering keeps at most ten fields and escapes field/context mrkdwn. 4. `RetryPolicy::decide` accepts 2xx, retries 429, 5xx, or no status only while `attempt < max_retries`, and gives up

on other 4xx. 5. Both local send implementations call their injected audit sink before the source-local success return or fake recording; an audit error propagates.

Each axiom is falsifiable by a source diff changing the named literal, branch,
bound, ordering, or trait call. Evidence: `src/{channel,message,retry,http,memory}.rs`.

<a id="s04"></a>
## S04 — Routing, payload, and retry review

For a channel, template, payload, retry, or error change, trace
`MessageTemplate` or `SlackMessage` → channel key → registry lookup →
`SharedSlackClient::send` branch. Preserve the six-variant taxonomy, field
limit/escaping, retry decision partition, and typed error boundary. Stop when
the conclusion requires a real webhook, provider rule, credential, or response.

<a id="s05"></a>
## S05 — Adapter and fake seams

`InquirySlackAdapter` implements the inquiry crate's `SlackClient` over an
injected `SharedSlackClient`; it source-routes posts to `EnterpriseInquiries`.
The in-memory client and audit sink retain mutex-protected fixture state. Those
facts prove local interface/fake behavior only, never a selected consumer,
cross-system transaction, external dispatch, or durable audit record.

<a id="s06"></a>
## S06 — Explicit unknowns

Unknown: environment values and credential provenance; selected registry
entries; DNS/TLS/HTTP behavior; Slack acceptance, rate limiting, thread IDs,
and delivery; audit implementation/durability; full consumer graph; feature
resolution; tests; deployment; and production operation. Obtain direct,
separately authorized evidence before stating any of them.

<a id="s07"></a>
## S07 — Static handoff

Report baseline, changed paths, exported symbols, the applicable source axiom
and B relation, documentary checks actually run, and unknowns. A structural
checker or whitespace check is neither Cargo/test execution nor provider,
runtime, deployment, or cold-review approval.

[Reference](../../../docs/ownership/crates/corelink-slack-real/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-slack-real/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-slack-real/MAINTENANCE.md#m01).
