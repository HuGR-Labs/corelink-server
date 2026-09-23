---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-enterprise-inquiry
manifest: crates/corelink-enterprise-inquiry/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: author_validated
evidence_set: w012-enterprise-inquiry-source-static-20260920
---

# corelink-enterprise-inquiry — blast radius

SOURCE-only atomic relation map. Arrows describe local declarations and control flow, not external operation or execution.

[Root](#b01) · [Ledger](#b02) · [Ports](#b03) · [HubSpot](#b04) · [Tests](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Crate root → module/export relation

**Arrow:** `src/lib.rs` module declarations → public module paths and its `pub use` list. **Activation:** an import chooses a named path. **Impact:** a removed declaration or re-export breaks that source path. **Evidence:** `src/lib.rs`. **Boundary:** this does not enumerate consumers or prove compatibility.

<a id="b02"></a>
## B02 — Form → ledger → outbox relation

**Arrow:** `EnterpriseInquiryForm` + identity values → `EnterpriseInquiryLedger::submit_inquiry` → local `InquiryRecord`/`OutboxRecord` maps. **Activation:** a direct method call. **Failure:** validation, seal, audit, Slack, CRM, mail, or internal errors use `EnterpriseInquiryError` arms. **Evidence:** `src/{form,ledger,outbox,error}.rs`. **Boundary:** maps and branches do not prove a durable or externally visible effect.

<a id="b03"></a>
## B03 — Ledger → injected-port relation

**Arrow:** ledger generic parameters → `InquiryAuditSink`, `SlackClient`, `CrmClient`, `AutoReplyMailer`, and `InquiryPayloadEncryptor`. **Activation:** named calls in `submit_inquiry`, escalation, and breach scan. **Impact:** a signature change affects the ledger generic contract. **Evidence:** `src/{ledger,audit,slack,crm,mailer,encryption}.rs`. **Boundary:** a trait/fake relation is not proof of a selected adapter or transport.

<a id="b04"></a>
## B04 — CRM trait → HubSpot implementation relation

**Arrow:** `CrmClient` → `impl CrmClient for HubSpotCrmClient<T, S>`, which uses `HubSpotHttp` and `HubSpotSleeper`. **Activation:** construction with types satisfying declared bounds and a call to the trait method. **Impact:** trait, retry, residency, or adapter-construction text changes alter this source relation. **Evidence:** `src/{crm,hubspot}.rs`. **Boundary:** no authorization, wire activity, or remote acceptance is established.

<a id="b05"></a>
## B05 — Manifest → test-source relation

**Arrow:** `Cargo.toml` `[[test]]` `prop_enterprise_inquiry` → `tests/prop_enterprise_inquiry.rs`; local `#[cfg(test)]` modules → their source assertions. **Activation:** target selection is required but unobserved. **Impact:** renamed name/path breaks the declared target. **Evidence:** manifest and named files. **Boundary:** assertions are not results.

<a id="b06"></a>
## B06 — Unresolved relation boundary

Known inverse source edges: `crates/corelink-ops/Cargo.toml` declares a direct dependency and `crates/corelink-ops/src/enterprise.rs` re-exports this crate’s public API. `crates/corelink-slack-real/Cargo.toml` declares a direct dependency, and `src/adapter.rs` imports inquiry types and implements the inquiry `SlackClient` trait for `InquirySlackAdapter`. These are static manifest/import/re-export/implementation relations only; they do not select or instantiate the adapter or prove a message was sent.

**Arrow:** remaining static package text → unknown domains: resolved graph/build, additional callers, credentials/transports, persistent/external effects, and deployment/independent review. **Mode:** UNKNOWN. **Evidence:** [R08](REFERENCE.md#r08). **Boundary:** B01–B05 and the known inverse source edges above cannot be expanded into any unknown domain without separate evidence.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-enterprise-inquiry/SKILL.md#s01)
