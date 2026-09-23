# #1676 / B-154 — owner/counsel decision packet

**Status:** `UNSIGNED — DECISION REQUIRED`  
**Version:** 1.0 · **Prepared:** 2026-09-22 · **Repository:** `corelink-server`  
**Scope:** owner and counsel decision support only. This packet does not amend an
executed instrument, send a notice, authorize spend, attest a capability, or close
[B-154](https://github.com/HuGR-dev/corelink-server/issues/1676).

## 1. Decision requested

For each row in §2, owner and counsel must select exactly one disposition:

- **AMEND:** prepare and negotiate a written addendum or superseding instrument
  with the affected counterparty; it becomes effective only when all required
  parties sign.
- **NOTIFY:** approve and send a formal correction or reservation to every
  recipient counsel identifies as having received or relied on the statement;
  retain delivery and acknowledgement evidence.
- **BUILD CAPABILITY:** approve the engineering, provider, credential, budget,
  and operational work needed to prove the statement in the relevant deployment.
  Building a capability does not by itself amend an executed instrument or cure a
  prior notice obligation.

Record one selection per row below. Leaving a row blank means the issue remains
blocked and open.

## 2. Claim and evidence register

| Decision row | Executed or circulated claim | Current evidence (versioned) | Required decision recipients |
|---|---|---|---|
| **DPA / Object Lock** | `legal/dpa/v1.0.0.en-US.md:110`: audit events retained in immutable R2 with Object Lock for 7 years. | `evidence/owner-actions/B-046/object-lock-probe.json` (`captured_at: 2026-09-09`, classification `INDETERMINATE`; credential rejected before bucket creation; object retention was `SKIPPED`). Canonical context: [`BACKLOG.md#B-046`](../../BACKLOG.md#B-046), [`docs/knowledge/ops/r2-object-lock-probe.md`](../knowledge/ops/r2-object-lock-probe.md). | Legal owner; counsel; DPA counterparty contact(s) identified by counsel; engineering/storage owner if **BUILD** is selected. |
| **Enterprise SLA / BYOK** | `legal/sla/v1.0.0.md:46`: Enterprise BYOK kill-switch p99 ≤ 5 minutes. | `evidence/owner-actions/B-083/byok-real-kms-lifecycle.json` (`captured_at: 2026-09-22T00:00:00Z`, `evidence_state: BLOCKED`, `tenant_redacted: NOT_PROVISIONED`; all five external prerequisites are `MISSING`; all ten lifecycle steps are `NOT_EXECUTED` with null receipt references). The repository checks are wiring/mutation checks and do not prove runtime operation or p99. Canonical context: [`BACKLOG.md#B-083`](../../BACKLOG.md#B-083). | Legal owner; counsel; SLA counterparty contact(s) identified by counsel; engineering/KMS owner if **BUILD** is selected. |
| **Enterprise case study / circulation** | `marketing/launch/CASE-STUDIES/enterprise-byok.md` is marked `DRAFT — NOT FOR PUBLICATION`, but prior circulation is unresolved. | Source remains draft. Issue #1676 records that removal from source does not prove that no earlier copy circulated. Preserve repository history and circulation/export evidence; do not infer customer attribution. | Legal owner; counsel; marketing/compliance records owner; the customer or recipient only if counsel identifies a notice or consent path. |

The receipts above are blockers and are not approvals, legal conclusions, runtime
proof, or evidence that an executed document changed. The historical B-046
`NotImplemented` report must not replace the current `INDETERMINATE` receipt.

## 3. Action worksheet and deadlines

Set **D0** to the date on which owner and counsel acknowledge receipt of this
packet. The deadlines below are operational targets, not contractual admissions.

| Row | Decision by | If AMEND | If NOTIFY | If BUILD | Decision / effective-date record |
|---|---|---|---|---|---|
| DPA / Object Lock | D0 + 5 business days | Counsel drafts addendum that states the actual retention mode and effective date; owner obtains counterparty signatures. | Counsel approves audience, factual correction, delivery channel and acknowledgement form; owner sends only after approval. | Owner approves a backend/provider design, budget and gate. Engineering reruns a provider/account-specific Object-Lock probe and records reproducible bucket and object results before any claim is restored. | Selection: ____ · Owner initials: ____ · Counsel initials: ____ · Date: ____ |
| Enterprise SLA / BYOK | D0 + 5 business days | Counsel drafts addendum with measurable scope, tier, tenant, measurement method and effective date; owner obtains signatures. | Counsel approves audience and correction/remedy language; owner sends only after approval and records delivery. | Owner approves the five protected prerequisites named in the B-083 receipt; engineering captures all ten lifecycle receipts (`customer_create_or_import`, `provider_access`, `wrap_unwrap`, `revoke_restore`, `rotate`, `deletion_schedule`, `audit_receipts`, `tenant_isolation`, `failure_retry`, `residency`) and a reproducible p99 measurement. | Selection: ____ · Owner initials: ____ · Counsel initials: ____ · Date: ____ |
| Case study / circulation | D0 + 5 business days | If a customer attribution or permission is required, counsel specifies the consent/addendum path before publication. | If counsel confirms circulation or reliance, owner sends the approved correction/withdrawal to the identified recipients; preserve the recipient list. | Keep source unpublished while owner investigates; any publication requires customer evidence, attribution approval and counsel clearance. | Selection: ____ · Owner initials: ____ · Counsel initials: ____ · Date: ____ |

**Execution target:** for any selected AMEND or NOTIFY row, counsel supplies the
approved instrument/notice by D0 + 10 business days, or records a revised date and
reason. For BUILD, the owner records a funded work order and acceptance gate by
D0 + 10 business days; no claim is considered proven until the required receipt is
versioned and reviewed.

## 4. Rollback and preservation controls

- **Before signature or send:** drafts are disposable working copies. Mark them
  `DRAFT — NOT EXECUTED`, keep the original instrument unchanged, and delete or
  quarantine superseded drafts according to counsel's record-retention direction.
- **After AMEND:** the latest fully signed instrument controls from its stated
  effective date. If negotiation fails, no unilateral edit is applied; retain the
  prior instrument and record `AMEND — NOT EXECUTED`.
- **After NOTIFY:** if counsel withdraws or corrects a notice, issue a dated,
  counsel-approved follow-up that identifies the original delivery and preserves
  both records. Do not silently delete the sent notice or recipient evidence.
- **During BUILD:** use an isolated test tenant/provider and least-privilege,
  revocable credentials. If a gate fails, stop rollout, disable the new path,
  revoke temporary credentials, preserve logs and receipts, and keep the executed
  claim unresolved. A successful probe or p99 run is scoped to its provider,
  account, image and measured deployment.
- **Case study:** freeze publication and preserve provenance while circulation is
  investigated. Removing the source file is not proof that no copy was delivered.

## 5. Closure receipt and signatures

[B-154] can be reconsidered only after each row has a recorded disposition and a
versioned evidence pointer: signed addendum reference, formal notice plus delivery
record, or capability receipt satisfying the applicable gate. Engineering and CI
status alone cannot close the legal decision.

**Owner decision-maker**  
Name: ______________________________  Role: ______________________________  
Disposition(s): _____________________  Date/time (UTC): __________________  
Signature: ______________________________________________________________

**Counsel**  
Name: ______________________________  Firm/role: _________________________  
Disposition(s): _____________________  Date/time (UTC): __________________  
Signature: ______________________________________________________________

**Engineering evidence owner (acknowledgement only; no legal approval)**  
Name: ______________________________  Scope accepted: ____________________  
Date/time (UTC): ____________________  Signature: _________________________

**Packet audit trail**  
Decision record / addendum / notice / capability receipt refs: ______________  
Reviewer: __________________________  Date: ______________________________
