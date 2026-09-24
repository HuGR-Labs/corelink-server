---
schema: corelink-ownership/1.1
document: reference
package: corelink-enterprise-inquiry
manifest: crates/corelink-enterprise-inquiry/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: author_validated
evidence_set: w012-enterprise-inquiry-source-static-20260920
---

# corelink-enterprise-inquiry — reference

SOURCE-only ownership record for the package manifest and local Rust text. No assertion here establishes an external operation, stored row, provider result, deployment, or execution.

[Identity](#r01) · [Surface](#r02) · [Ports](#r03) · [Representations](#r04) · [Axioms](#r05) · [Tests](#r06) · [Boundary](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Identity and scope

The manifest names `corelink-enterprise-inquiry`, declares `thiserror`, `blake3`, `base64`, `hmac`, and `sha2`, and declares one integration-test target, `prop_enterprise_inquiry`. `src/lib.rs` forbids unsafe code, denies missing documentation/debug implementations, declares ten public modules, and re-exports their named surfaces. Changing any stated manifest entry, module declaration, lint, or re-export falsifies this inventory.

<a id="r02"></a>
## R02 — Form and lifecycle surface

`form.rs` defines `InquiryId`, `IdempotencyKey`, `EnterpriseInquiryForm`, `InquiryReceipt`, and four non-exhaustive enums: `Role` (five variants), `BYOKRequirementsKind` (five), `ResidencyKind` (six), and `InquiryStatus` (four). `EnterpriseInquiryForm::validate` checks trimmed company presence/200-char maximum, light email shape, optional notes/2,000-char maximum, use-case/500-char maximum, and nonempty trimmed locale; `lead_score` adds 40 for non-None BYOK, 30 for EU residency, and 30 for monthly GB at least 1,000. Removing or changing a named type, variant, branch, bound, or score arm falsifies this record.

<a id="r03"></a>
## R03 — Port and adapter declarations

`InquiryAuditSink`, `SlackClient`, `CrmClient`, `AutoReplyMailer`, `InquiryPayloadEncryptor`, `HubSpotHttp`, and `HubSpotSleeper` are synchronous `Debug + Send + Sync` trait declarations. Their in-package in-memory and failing implementations are source-local fixtures. `HubSpotCrmClient<T, S>` implements `CrmClient` when its two generic ports meet their declared bounds; `HubSpotToken`, `HubSpotRegion`, envelope types, and retry declarations are local Rust representations. This is not evidence of a selected implementation, credential, transport, or provider effect.

<a id="r04"></a>
## R04 — Ledger, outbox, and encryption representations

`EnterpriseInquiryLedger<A, S, C, M, E>` exposes `submit_inquiry`, `drain_partial_escalations`, `sla_breaches`, `record_spam_blocked`, and read snapshots. `OutboxStatus` has `Pending`, `Committed`, `RolledBack`, and `PartialEscalated`; `OutboxRecord` has `pending`, `commit`, `rollback`, and `escalate_partial` helpers. `encryption.rs` supplies AAD, sealed payload, sanitized metadata, unsealed PII, encryptor error/trait, and helper functions; `enterprise_inquiry_schema_version` is a `const fn` returning `1`, alongside `SLA_RESPONSE_MS`, `AUTO_REPLY_BUDGET_MS`, `ADDITIONAL_NOTES_MAX`, and `COMPANY_MAX`. Altering a named API, enum arm, helper, or constant falsifies this source map.

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| ID | Atomic SOURCE axiom | Static falsifier / limit |
|---|---|---|
| AX-EI-01 | `submit_inquiry` validates, performs idempotency lookup, seals, then emits `Received` before inserting its local inquiry/outbox maps. | Reordering/removing those calls in `ledger.rs`; no persistence assertion follows. |
| AX-EI-02 | A Slack error reaches rollback without the CRM call; a CRM error attempts a rollback Slack post before returning `Crm` or `Compensation`. | Changing the two match branches in `submit_inquiry`; no external delivery claim follows. |
| AX-EI-03 | `sla_breaches` selects unreplied records excluding `RolledBack` and `PartialEscalated` when elapsed time is at least `SLA_RESPONSE_MS`. | Changing its predicate in `ledger.rs`; no clock or alert observation follows. |
| AX-EI-04 | `InquiryPayloadEncryptor` has synchronous `seal` and `unseal` methods, and the in-memory implementation checks AAD on unseal. | Changing the trait signature or AAD comparison in `encryption.rs`; no cryptographic-provider assurance follows. |
| AX-EI-05 | `classify_retry` returns success for 2xx, auth hard-fail for 401/403, and retries retryable responses only below `MAX_RETRIES`. | Changing `HubSpotResponse` predicates or `classify_retry`; no remote response behavior follows. |

<a id="r06"></a>
## R06 — Test-source inventory

The manifest names `tests/prop_enterprise_inquiry.rs`; local unit-test modules also occur in the package source, including `hubspot/tests.rs`. Their assertions and fixtures are SOURCE text only. They do not establish compilation, test selection, result, concurrency behavior, or compatibility.

<a id="r07"></a>
## R07 — Ownership boundaries

| Boundary | Source-defined fact | Not established |
|---|---|---|
| Form/ledger | Rust validation, maps, status helpers, and control flow | durable storage or handler binding |
| Ports/fakes | traits and local implementations | provider use or message/mail effect |
| Encryption | local types, AAD construction, and fake algorithm | key custody, cryptographic assurance, or secret handling |
| HubSpot | adapter-construction and retry text | authorization, transmission, remote acceptance, or receipt |
| Tests | declared target and assertion source | compilation, execution, or result |

<a id="r08"></a>
## R08 — Explicit unknowns and OKF route

The canonical verified OKF route is [OKF profile](../../../internal/okf-wiki/01-okf-corelink-profile.contract.md); it is linked for routing only and is neither copied nor revalidated here.

1. Feature/target resolution, compilation, linking, and test outcome are UNKNOWN.
2. Known inverse source edges are `corelink-ops` (manifest dependency and
   `src/enterprise.rs` public re-export) and `corelink-slack-real` (manifest
   dependency and `src/adapter.rs` imports plus `InquirySlackAdapter`'s
   `SlackClient` implementation). These declarations do not prove adapter
   construction, selection, invocation, or reachability. Additional reverse
   consumers, caller compatibility, and runtime reachability are UNKNOWN.
3. Credential handling, transport, provider behavior, and any remote acceptance are UNKNOWN.
4. Persistence, timing, concurrency, retries, and observability effects are UNKNOWN.
5. Configuration, deployment, production data, monitoring, and independent review are UNKNOWN.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-enterprise-inquiry/SKILL.md#s01)
