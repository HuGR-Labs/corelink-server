---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-slack-real
manifest: crates/corelink-slack-real/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: slack-real-source-static-20260920
---

# corelink-slack-real — blast radius

Each relation is an atomic source or manifest relation: direction names the
static boundary, flow names the code-level input/output, and impact names what
must be reconsidered after a change. None proves invocation, credentials,
network traffic, Slack behavior, durable audit, deployment, or a complete
reverse graph.

[Public surface](#b01) · [Routing](#b02) · [Payload](#b03) · [Retry/audit](#b04) · [Adapter/fakes](#b05) · [Facades/unknowns](#b06) · [HTTP/TLS](#b07).

**Relation index:** [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011).

<a id="b01"></a>
## B01 — Public surface relation

<a id="rel-001"></a>
### REL-001 — Consumer import to root export
**Type/endpoints:** dependency/reverse consumer; downstream source → `src/lib.rs`. **Surface/activation:** public module and re-export names, when a consumer imports them. **Contract/effect:** symbol or signature changes may break source compatibility. **Failure/propagation:** compile/import failure; full consumer set unresolved. **Validation/coordination:** search manifests and imports, then review the named consumer; no execution implied. **Evidence:** `src/lib.rs`; reverse census incomplete.

[Relation index](#b01)

<a id="b02"></a>
## B02 — Message-to-registry routing relation

<a id="rel-002"></a>
### REL-002 — Message channel to registry lookup
**Type/endpoints:** data/runtime-call shape; `SlackMessage.channel` → `WebhookRegistry::url` → `SlackHttpClient::send`. **Surface/activation:** one send after client invocation. **Contract/effect:** a declared key returns its local URL string; absent key returns `ChannelUnconfigured`. **Failure/propagation:** lookup error exits before payload, retry, or audit. **Boundary:** process environment and credential value unknown. **Validation/coordination:** inspect channel arms and lookup; coordinate API-001. **Evidence:** `channel.rs`, `http.rs`.

[Relation index](#b01)

<a id="b03"></a>
## B03 — Template-to-Block-Kit relation

<a id="rel-003"></a>
### REL-003 — Template to local JSON payload
**Type/endpoints:** data; `MessageTemplate` → `SlackMessage` → `serde_json::Value`. **Surface/activation:** `render`/`to_block_kit_json`. **Contract/effect:** local payload has capped header/fields, optional action/context/thread and escaped text. **Failure/propagation:** field or escaping changes alter serialized shape. **Boundary:** provider parsing/acceptance unknown. **Validation/coordination:** inspect render branches and redaction cases; coordinate API-002/INV-002. **Evidence:** `template.rs`, `message.rs`, `redact.rs`.

[Relation index](#b01)

<a id="b04"></a>
## B04 — Retry-to-audit terminal relation

<a id="rel-004"></a>
### REL-004 — Client terminal branch to injected audit sink
**Type/endpoints:** runtime-call; `SlackHttpClient::send` → `RetryPolicy` → `SlackAuditSink` → return. **Surface/activation:** after successful registry lookup. **Contract/effect:** transient-class results retry within bound; terminal branch emits `Sent` or `Failed` before return. **Failure/propagation:** sink error propagates; lookup failure occurs before this relation. **Boundary:** no HTTP request, wait, durability, or transaction observed. **Validation/coordination:** inspect each branch/order; coordinate API-003. **Evidence:** `http.rs`, `retry.rs`, `audit.rs`.

[Relation index](#b01)

<a id="b05"></a>
## B05 — Inquiry adapter and fake-state relation

<a id="rel-005"></a>
### REL-005 — Inquiry trait adapter
**Type/endpoints:** dependency/re-export call; inquiry `SlackClient` → `InquirySlackAdapter` → `SharedSlackClient`. **Surface/activation:** adapter method call. **Contract/effect:** source fixes channel to `EnterpriseInquiries` and maps client errors to strings. **Failure/propagation:** local send error becomes inquiry error. **Boundary:** selected caller/provider unknown. **Validation/coordination:** compare trait signature, channel, mapping; coordinate inquiry owner. **Evidence:** `adapter.rs`, manifest.

[Relation index](#b01)

<a id="rel-006"></a>
### REL-006 — Fake audit to local recording
**Type/endpoints:** data; `InMemorySharedSlackClient` → injected sink → mutex vector. **Surface/activation:** explicit fake `send`. **Contract/effect:** fake appends `RecordedSend` only after audit emit succeeds. **Failure/propagation:** audit error prevents local record. **Boundary:** no durable state or live consumer. **Validation/coordination:** inspect ordering and fake tests. **Evidence:** `memory.rs`, `audit.rs`.

[Relation index](#b01)

<a id="b06"></a>
## B06 — Facade and unknown-consumer relation

<a id="rel-007"></a>
### REL-007 — Ops direct package dependency
**Type/endpoints:** manifest dependency; `crates/corelink-ops/Cargo.toml` → package `corelink-slack-real`. **Surface/activation:** a selected ops target resolves the declared dependency. **Contract/effect:** package identity/path changes can affect dependency resolution and compile compatibility. **Failure/propagation:** a selected target may fail to resolve or compile. **Boundary:** target selection, feature resolution, and runtime use are unknown. **Validation/coordination:** inspect the manifest declaration independently of the facade source. **Evidence:** `crates/corelink-ops/Cargo.toml`.

[Relation index](#b01)

<a id="rel-009"></a>
### REL-009 — Cloud-adapters direct package dependency
**Type/endpoints:** manifest dependency; `crates/corelink-adapters-cloud/Cargo.toml` → package `corelink-slack-real`. **Surface/activation:** a selected cloud-adapters target resolves the declared dependency. **Contract/effect:** package identity/path changes can affect dependency resolution and compile compatibility. **Failure/propagation:** a selected target may fail to resolve or compile. **Boundary:** target selection, feature resolution, and runtime use are unknown. **Validation/coordination:** inspect the manifest declaration independently of the facade source. **Evidence:** `crates/corelink-adapters-cloud/Cargo.toml`.

[Relation index](#b06)

<a id="rel-010"></a>
### REL-010 — Ops Slack facade re-export
**Type/endpoints:** source re-export; `corelink-ops::slack` → `corelink_slack_real::*`. **Surface/activation:** importing through the `corelink-ops` facade. **Contract/effect:** this source path forwards package symbols; implementation remains in `corelink-slack-real`. **Failure/propagation:** changing the re-export target or required upstream symbol can break imports through this facade. **Boundary:** downstream imports and runtime use are unknown. **Validation/coordination:** compare `src/slack.rs` with root exports; do not infer this relation from the manifest edge alone. **Evidence:** `crates/corelink-ops/src/slack.rs`, `crates/corelink-slack-real/src/lib.rs`.
[Relation index](#b06)

<a id="rel-011"></a>
### REL-011 — Cloud-adapters Slack facade re-export
**Type/endpoints:** source re-export; `corelink-adapters-cloud::slack` → `corelink_slack_real::*`. **Surface/activation:** importing through the cloud-adapters facade. **Contract/effect:** this source path forwards package symbols; implementation remains in `corelink-slack-real`. **Failure/propagation:** changing the re-export target or required upstream symbol can break imports through this facade. **Boundary:** downstream imports and runtime use are unknown. **Validation/coordination:** compare `src/slack.rs` with root exports; do not infer this relation from the manifest edge alone. **Evidence:** `crates/corelink-adapters-cloud/src/slack.rs`, `crates/corelink-slack-real/src/lib.rs`.
[Relation index](#b06)

<a id="b07"></a>
## B07 — HTTP and TLS dependency boundary

[Relation index](#b01)

<a id="rel-008"></a>
### REL-008 — `reqwest` blocking/rustls client dependency
**Type/endpoints:** dependency; `corelink-slack-real` manifest → third-party `reqwest`. **Surface/activation:** `SlackHttpClient` construction/send on a caller path. **Contract/effect:** manifest enables blocking, JSON, and rustls features; `http.rs` uses the client/request/response API. **Failure/propagation:** transport and status errors enter local retry/error branches. **Boundary:** actual resolver, TLS negotiation, endpoint, and request remain unknown. **Validation/coordination:** inspect manifest feature declaration and `http.rs`; no graph/runtime proof. **Evidence:** `Cargo.toml`, `http.rs`.

[Relation index](#b01)

[Relation index](#b01)

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-slack-real/SKILL.md#s01).
