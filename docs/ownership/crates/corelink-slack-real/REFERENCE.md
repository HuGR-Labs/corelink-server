---
schema: corelink-ownership/1.1
document: reference
package: corelink-slack-real
manifest: crates/corelink-slack-real/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: slack-real-source-static-20260920
---

# corelink-slack-real — reference

This S-profile reference records static manifest and source facts. It does not
establish a webhook credential, a Slack request, delivery, provider behavior,
audit persistence, consumer selection, deployment, or runtime observation.
The verified [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md)
is a routing reference only and is not restated or revalidated here.

[Identity](#r01) · [Exports](#r02) · [Routing](#r03) · [Payload](#r04) · [Retry](#r05) · [Client/audit](#r06) · [Fakes](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Package identity and evidence boundary

`crates/corelink-slack-real/Cargo.toml` names `corelink-slack-real`. Its direct
dependencies are `thiserror`, `serde`, `serde_json`, `reqwest` with blocking,
JSON, and rustls features, and `corelink-enterprise-inquiry`; `wiremock`,
`tokio`, `proptest`, `rand`, and `rand_chacha` are dev-dependencies. A manifest
declaration is a static selection input, not evidence that code resolves,
compiles, executes, reads a credential, or contacts a provider.

<a id="r02"></a>
## R02 — Root export contract

`src/lib.rs` declares ten public modules and re-exports `InquirySlackAdapter`,
audit types/sink, channel/registry types, client outcome/error/trait,
`SlackHttpClient`, in-memory client/record, message types, redactor, retry
types, and `MessageTemplate`. It forbids unsafe code and denies missing docs.
Falsifiable invariant: adding, removing, or renaming a root module or re-export
changes this source-visible API surface.

<a id="r03"></a>
## R03 — Channel registry relation

`SlackChannel` has six source variants: `AlertsSev1`, `AlertsSev2`,
`EnterpriseInquiries`, `BreachNotifications`, `OncallHandoff`, and
`LighthouseCustomers`. `as_str`, `env_var`, and `all` match those six cases.
`WebhookRegistry` stores strings in a `BTreeMap`; `from_env` inserts only
nonempty process-environment values, and a lookup of an absent channel returns
`WebhookRegistryError::ChannelUnconfigured`. This is routing-table code, not a
credential inventory or configured-channel observation.

<a id="r04"></a>
## R04 — Structured payload relation

`MessageTemplate` has six variants whose `channel()` arms select the six
channels; `render()` constructs a `SlackMessage` with its local fields/actions.
`SlackMessage::new` applies a 150-character header cap. `to_block_kit_json`
renders a header, at most ten section fields, optional action elements, a
context block, a fallback `text`, and optional `thread_ts`; field and context
text pass through `escape_mrkdwn`. Falsifiable invariant: changing a template
arm, cap, `.take(10)`, or escape branch changes the rendered source contract.

**Contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003).

<a id="r05"></a>
## R05 — Retry decision relation

`RetryPolicy::r2_4_default` returns three retries, 250ms initial backoff, and
an 8s cap. `decide` returns `Success` for 2xx; it returns `Retry` for 429,
5xx, or `None` only before `max_retries`, otherwise `GiveUp`; other statuses
give up immediately. `backoff_for` uses
`1u32.checked_shl(attempt).unwrap_or(u32::MAX)`, then a saturating millisecond
multiply and a `max_backoff` minimum. These are pure source branches, not
evidence of observed provider statuses, waiting, or retry execution.

<a id="r06"></a>
## R06 — Client, error, and audit seam

`SharedSlackClient::send` returns `SendOutcome` or `SlackClientError`, whose
source variants cover registry, exhausted transport, permanent rejection, and
audit failure. `SlackHttpClient::send` can return the registry error at its
initial `self.registry.url(message.channel)?`, before payload construction,
retry, or audit.

After that lookup succeeds, the source builds JSON, classifies each loop result
with `RetryPolicy`, and emits either a `Sent` or `Failed` `SlackAuditEvent`
before returning that post-registry terminal branch. `SlackAuditSink` is
injected; an emit error propagates through `?`. This proves source ordering and
interface shape, not HTTP activity or external atomicity.

<a id="r07"></a>
## R07 — Inquiry adapter and local fakes

`InquirySlackAdapter` implements `corelink_enterprise_inquiry::SlackClient`
over `Arc<dyn SharedSlackClient>`, constructs an `EnterpriseInquiries` message,
and maps local `SlackClientError` variants to the inquiry error string.
`InMemorySharedSlackClient` emits a local `Sent` event before appending a
`RecordedSend` to its mutex-protected vector; `InMemorySlackAuditSink` appends
events to another mutex-protected vector. These source-defined fakes are not a
provider adapter, a durable audit backend, or evidence of a call.

**Contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003).

<a id="api-001"></a>
### API-001 — Channel registry
**Symbols:** `SlackChannel::{all,as_str,env_var}`, `WebhookRegistry::{from_env,url}`. **Input/precondition:** one declared channel and environment lookup by `from_env`. **Output/errors/effects:** local map omits unset/empty values; `url` returns `ChannelUnconfigured` when absent. **Compatibility:** six variants and literals are source keys. **Links/evidence:** INV-001; REL-002; `channel.rs`.

[Contract index](#r04)

<a id="api-002"></a>
### API-002 — Rendered message
**Symbols:** `MessageTemplate::{channel,render}`, `SlackMessage::{new,to_block_kit_json}`. **Input/precondition:** template fields and channel; header input may exceed constructor cap. **Output/effects:** local Block Kit JSON with at most ten fields and escaped text. **Errors/compatibility:** JSON fields and caps affect source payload shape. **Links/evidence:** INV-002; REL-003; `template.rs`, `message.rs`, `redact.rs`.

[Contract index](#r04)

<a id="api-003"></a>
### API-003 — Client send and audit
**Symbols:** `SharedSlackClient::send`, `SlackHttpClient::send`, `RetryPolicy::{decide,backoff_for}`, `SlackAuditSink::emit`. **Input/precondition:** message and registry URL resolve before request loop. **Output/errors/effects:** success or typed failure; terminal post-lookup branches emit through the injected sink before return. **Compatibility:** retry classification and error variants are source contracts. **Links/evidence:** INV-003; REL-004/007/008; `client.rs`, `http.rs`, `retry.rs`, `audit.rs`.

[Contract index](#r04)

<a id="api-004"></a>
### API-004 — Inquiry adapter
**Symbols:** `InquirySlackAdapter` and `corelink_enterprise_inquiry::SlackClient` implementation. **Input/precondition:** inquiry post and injected `SharedSlackClient`. **Output/errors/effects:** routes to `EnterpriseInquiries` and maps local errors to inquiry error strings. **Compatibility:** mapping and channel are compile-time integration points. **Links/evidence:** REL-005; `adapter.rs`; selected consumer unknown.

[Contract index](#r04)

<a id="api-005"></a>
### API-005 — Public error and redaction surface
**Symbols:** `WebhookRegistryError::ChannelUnconfigured { channel }`, `SlackClientError::{Registry,TransportExhausted { attempts, reason },PermanentReject { status, reason },AuditFailed}`, `SlackAuditError::EmitFailed(String)`, `redact_webhook`. **Input/precondition:** absent channel, exhausted transport, rejected response, audit error, or webhook URL.

**Output/effects:** errors are non-exhaustive and retain the listed fields; a URL beginning `https://hooks.slack.com/services/` with at least three slash-delimited components becomes that prefix plus the first two components and `/***`. All other inputs return `<redacted>`. Error display strings and redaction output are caller-visible; preserve the opaque fallback for malformed or foreign URLs. **Links/evidence:** REL-002/004/006; `channel.rs`, `client.rs`, `audit.rs`, `redact.rs`.

[Contract index](#r04)

<a id="inv-001"></a>
### INV-001 — Registry keys are the six declared channels
**Predicate:** `all()` lists six variants and `as_str`/`env_var` are exhaustive. **Enforcement:** `channel.rs`; registry keys by `SlackChannel`. **Violation:** a missing/duplicate mapping or mismatched lookup key. **Verification:** inspect match arms and channel tests; execution unknown.

[Contract index](#r04)

<a id="inv-002"></a>
### INV-002 — Rendered message bounds remain enforced
**Predicate:** header cap is 150 characters, JSON emits at most ten fields, and field/context text is escaped. **Enforcement:** `SlackMessage::new`, `to_block_kit_json`, `escape_mrkdwn`. **Violation:** a bound/escape branch is bypassed. **Verification:** source branches and matching test source; execution unknown.

[Contract index](#r04)

<a id="inv-003"></a>
### INV-003 — Retry set and terminal audit order remain explicit
**Predicate:** only 429, 5xx, or absent status retry below the configured bound; terminal post-lookup branches emit audit before return. **Enforcement:** `RetryPolicy::decide`, `SlackHttpClient::send`. **Violation:** another status retries or return precedes emit. **Verification:** source branch/order inspection; runtime unknown.

[Contract index](#r04)

<a id="r08"></a>
## R08 — Evidence limits and explicit unknowns

Unknown: resolved dependencies/features; all consumers; environment values;
credential origin, availability, and rotation; real registry contents; DNS,
TLS, HTTP and Slack responses; request acceptance/delivery/rate limiting;
thread identity; audit backend/durability; test execution; deployment; and
runtime operation. The canonical operational record remains a route only; none
of these unknowns can be filled by comments, trait names, manifests, or fakes.

[Ownership guide](../../../../.claude/skills/own-corelink-slack-real/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
