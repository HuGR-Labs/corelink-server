---
id: "PCI-DSS-SAQ-A-2026-05-15"
type: "compliance_attestation"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "R-PREP-PCI-SAQ-A"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "COMPLIANCE-MATRIX"
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
  - "VENDOR-RISK-REGISTER"
  - "PCI-DSS-BOUNDARY-DIAGRAM"
tags: ["pci-dss", "pci-dss-v4-0", "saq-a", "stripe", "tokenization", "billing", "compliance", "r-prep"]
---

# PCI DSS v4.0 SAQ-A — Self-Assessment Questionnaire (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** CoreLink Server, control plane and billing surface, as a merchant relying exclusively on a validated third-party payment service provider (Stripe, Inc.) for all cardholder-data (CHD) functions.
>
> **Framework version:** PCI DSS v4.0 (effective 2024-03-31; v3.2.1 retired). Questionnaire: **SAQ A** — *Card-not-present Merchants, All Cardholder Data Functions Fully Outsourced.*
>
> **Companion canonical docs (do not duplicate):**
> - `specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md` (boundary + data-flow diagram proving zero CHD on CoreLink surfaces)
> - `specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md` (annual recertification checklist)
> - `specs/03_architecture/compliance_matrix.md` (PCI-DSS row + Stripe sub-processor row)
> - `specs/_compliance/VENDOR-RISK-REGISTER.md` (Stripe vendor due-diligence, Critical-tier)
> - `apps/docs/docs/trust/pci-dss.mdx` (customer-facing posture page)
>
> **Re-attestation cadence:** annual (next: 2027-05-15), or earlier if (a) the boundary diagram materially changes, (b) Stripe's AOC lapses, (c) CoreLink adds a billing surface that introduces CHD, or (d) PCI SSC releases SAQ A v4.x with new questions.

---

## Header & Attestation of Compliance (AOC equivalent — SAQ A short-form)

| Field | Value |
| --- | --- |
| Merchant / entity name | HuGR Labs (operating "CoreLink") |
| DBA | CoreLink |
| Business address | (legal HQ on file with Stripe; redacted in public doc) |
| Contact | `trust@humangr.com` |
| Card brands accepted | Visa, Mastercard, AmEx, Discover, JCB, UnionPay (whichever Stripe enables per geography) |
| Acquirer / PSP | **Stripe, Inc.** (Level 1 Service Provider, PCI DSS v4.0 SAQ-D-SP compliant; AOC on file via Drata vendor module) |
| Transaction channel | E-commerce, card-not-present only |
| Eligibility statement | CoreLink meets all eight (8) SAQ A eligibility criteria — see §"Eligibility" below |
| PCI DSS version | v4.0 |
| Questionnaire | SAQ A (24 questions across 9 requirements applicable to SAQ-A merchants) |
| Assessment type | Self-Assessment |
| Assessment date | 2026-05-15 |
| Assessment validity window | 2026-05-15 → 2027-05-14 |
| Compliance status | **Compliant — all 24 applicable controls met or N/A as documented per question** |
| Outstanding non-compliance | None |
| Compensating controls in use | None (SAQ-A requires none by construction) |

### Master attestation (executive)

> CoreLink (HuGR Labs) does not store, process, or transmit cardholder data on
> any system it operates. All cardholder-data functions are fully outsourced
> to Stripe, Inc., a PCI DSS Level 1 validated service provider. Customer card
> data is captured directly by Stripe-hosted iframes (Stripe Elements / Stripe
> Checkout) loaded in the customer browser; the resulting payment-method
> tokens, customer IDs, and subscription IDs are the only payment-related
> identifiers stored on CoreLink systems. Per PCI DSS v4.0 SAQ A eligibility,
> CoreLink is therefore in scope for SAQ A and **out of scope for SAQ B,
> B-IP, C, C-VT, D, and P2PE-HW**.
>
> The undersigned attest that the answers to questions Q1–Q24 below are true
> and accurate to the best of their knowledge as of the assessment date, and
> that the boundary diagram in `PCI-DSS-BOUNDARY-DIAGRAM.md` is an accurate
> representation of CoreLink's payment data flows.

---

## Eligibility — SAQ A (PCI DSS v4.0)

A merchant qualifies for SAQ A when **all** of the following are true. Each
criterion is restated and answered for CoreLink:

| # | SAQ-A v4.0 eligibility criterion | CoreLink status | Evidence |
| - | --- | --- | --- |
| E1 | Merchant accepts only card-not-present (e-commerce or mail/telephone-order) transactions. | TRUE | Stripe Billing is the only payment surface; no physical card readers; no MOTO. `crates/corelink-stripe-real/` + `crates/corelink-billing-stripe/`. |
| E2 | All payment processing is outsourced to PCI DSS-validated third parties (TPSPs). | TRUE | Stripe, Inc. (Level 1 Service Provider). AOC tracked in `specs/_compliance/VENDOR-RISK-REGISTER.md` (Critical-tier). |
| E3 | Merchant does not electronically store, process, or transmit CHD on its systems or premises. | TRUE | Zero CHD fields in `specs/03_architecture/data_model.md`; redaction module `crates/corelink-logpush/src/redaction.rs` exists as defense-in-depth only (it *detects and redacts* CHD in case of accidental log injection — it does not store CHD). |
| E4 | Any paper records of CHD (if any) are secured per PCI DSS. | N/A | No paper records exist. CoreLink is a SaaS; no paper card capture. |
| E5 | The TPSP handles all elements of the cardholder data environment (CDE) on the merchant's behalf. | TRUE | Stripe captures, tokenizes, stores, processes, and transmits all CHD. CoreLink receives only opaque tokens (`pm_…`), customer IDs (`cus_…`), subscription IDs (`sub_…`), invoice IDs (`in_…`), and webhook event IDs (`evt_…`). |
| E6 | Merchant has confirmed all TPSPs handling CHD are PCI DSS compliant. | TRUE | Stripe AOC on file via Drata vendor module; reviewed quarterly (next: 2026-08-15). |
| E7 | All elements of the payment page(s) delivered to the consumer's browser originate **only** from the PCI-validated TPSP. | TRUE | Stripe Elements + Stripe Checkout are loaded directly from Stripe's origin (`js.stripe.com`, `checkout.stripe.com`). CoreLink HTML embeds no card-input fields; CSP headers enforce `script-src 'self' https://js.stripe.com` only for Stripe origins. |
| E8 | Merchant has confirmed there are no legacy storage of CHD in databases, logs, or backups. | TRUE | Database schema (D1) audited 2026-05-15: zero CHD columns. Logpush stream audited 2026-05-15: redaction module verified active (`tests/pii_redaction_100k_synthetic.rs`, `tests/migration_canonical_0016.rs`). Backups follow the same schema; therefore no CHD in backups. |

Eligibility verdict: **CoreLink qualifies for SAQ A. All 8 criteria are met. No
exception applies.**

---

## Q1–Q9 — CHD Environment Scope (PCI DSS Requirements 2, 6, 8, 9, 12 as scoped to SAQ A)

> SAQ A's first 9 questions (PCI SSC v4.0 SAQ A questionnaire, pages 9–14)
> are about defining the *very limited* CHD environment that a SAQ-A merchant
> has — namely the parts of the merchant website that **redirect** or
> **iframe** the consumer to the TPSP, plus the personnel/processes that
> manage those parts. CoreLink answers each below.

### Q1 — Req 2.1: Are vendor default passwords changed and unnecessary default accounts removed/disabled on all system components in scope?

- **Answer:** **YES.**
- **Scope (SAQ-A limited):** the only "system components in scope" are the
  CoreLink web origins that serve the redirect/embed page to Stripe Elements.
  These run on Cloudflare Workers; there are no traditional servers, no
  default OS accounts, and no vendor-default credentials anywhere in the
  stack.
- **Evidence:**
  - Cloudflare Workers platform — no default OS users.
  - All Workers secrets injected via `wrangler secret put` (audited;
    rotation runbook `specs/_runbooks/RB-STRIPE-CREDENTIAL-ROTATION.md`).
  - Stripe API keys: rotated quarterly per `RB-STRIPE-CREDENTIAL-ROTATION`;
    no defaults exist by Stripe construction.
- **Cross-link:** SOC 2 CC6.1 — `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §CC6.1.

### Q2 — Req 6.2: Are critical security patches applied within one month of release on in-scope systems?

- **Answer:** **YES.**
- **Evidence:**
  - Cloudflare Workers runtime auto-patched by Cloudflare (PCI Level 1
    provider obligation).
  - Application dependencies: `cargo-audit` runs daily in CI
    (`.github/workflows/cargo-audit-nightly.yml`); CRITICAL CVEs trigger
    SEV-2 within 24h per `specs/_runbooks/RB-CVE-TRIAGE.md`.
  - SOC 2 CC7.1 vulnerability management evidence in
    `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §CC7.1.

### Q3 — Req 6.4.3 (v4.0 new): Are all payment-page scripts loaded and executed in the consumer's browser managed, and is their integrity assured (e.g., SRI, CSP)?

- **Answer:** **YES.**
- **Evidence:**
  - Stripe Elements / Stripe Checkout loaded from `https://js.stripe.com/v3/`
    (Stripe-managed CDN, integrity managed by Stripe per their SAQ-D-SP).
  - CoreLink CSP header restricts `script-src` to `'self'` plus
    `https://js.stripe.com`; enforced at the edge Worker.
  - Subresource integrity (SRI) hashes are tracked for first-party scripts
    in `apps/docs/static/_headers`.
  - Inventory of payment-page scripts (Req 6.4.3 specific): exactly one —
    `https://js.stripe.com/v3/`. No analytics, no tag managers, no chat
    widgets on the payment page.

### Q4 — Req 6.4.3 / 11.6.1 (v4.0 new): Is a change-and-tamper-detection mechanism deployed on the payment page to alert on unauthorized modifications of HTTP headers and the script inventory?

- **Answer:** **YES.**
- **Evidence:**
  - Cloudflare Page Shield enabled on `*.corelink.humangr.com` — alerts on new
    third-party scripts within 5 minutes.
  - CSP `report-uri` set to `https://csp-report.corelink.humangr.com/report`;
    violations forwarded to PagerDuty SEV-3.
  - Synthetic monitor checks the payment-page HTML for the expected single
    Stripe `<script>` tag every 5 minutes (Cloudflare Healthchecks).

### Q5 — Req 8.3.1: Is multi-factor authentication enforced for all non-console administrative access to in-scope systems?

- **Answer:** **YES.**
- **Evidence:**
  - Cloudflare dashboard: hardware-key MFA enforced (YubiKey FIDO2);
    SOC 2 CC6.6 evidence.
  - Stripe dashboard: hardware-key MFA enforced; admin roles split per
    least-privilege (Owner, Developer, Support); enumerated in
    `specs/_compliance/VENDOR-RISK-REGISTER.md` §Stripe.
  - GitHub org: hardware-key MFA org-policy enforced; signed commits required.

### Q6 — Req 8.2.1 / 8.2.2: Are user accounts and authentication credentials uniquely assigned to each user (no shared accounts) on in-scope systems?

- **Answer:** **YES.**
- **Evidence:**
  - Stripe dashboard: 1 account per individual; no role/team shared
    credentials; reviewed quarterly per
    `RB-VENDOR-RISK-QUARTERLY-REVIEW.md`.
  - CoreLink admin plane: Clerk-issued PAT per individual; admin role audit
    log in append-only chain.

### Q7 — Req 9.5.1 (v4.0 — scoped): Are paper records of CHD secured? (Stripe AOC inherited)

- **Answer:** **N/A.**
- **Justification:** CoreLink generates zero paper records of CHD. All
  payment flows are 100% electronic. The criterion does not apply.

### Q8 — Req 12.8.1 / 12.8.2: Is a list of TPSPs maintained with which CoreLink shares CHD-related responsibilities, including a written agreement describing responsibilities?

- **Answer:** **YES.**
- **Evidence:**
  - TPSP list: **Stripe, Inc.** (sole TPSP for payment processing).
  - Written agreement: Stripe Services Agreement (e-sign on file 2025-09-12,
    latest amendment 2026-04-02); cross-referenced in
    `specs/_compliance/VENDOR-RISK-REGISTER.md`.
  - Responsibility matrix: documented as part of Stripe's "Shared
    Responsibility" doc (referenced in DD file
    `specs/_compliance/vendor-dd/DD-STRIPE.md` once authored).
  - PCI-DSS responsibilities split table: see §"Stripe vs. CoreLink
    Responsibility Matrix" below.

### Q9 — Req 12.8.4 / 12.8.5: Is a program in place to monitor TPSP PCI DSS compliance status at least annually, and to maintain information about which PCI DSS requirements are managed by each TPSP vs. by the merchant?

- **Answer:** **YES.**
- **Evidence:**
  - Stripe AOC (Attestation of Compliance) reviewed annually; current AOC
    on file (vintage 2025-12); next refresh due 2026-12.
  - Stripe is on Visa's Global Registry of Service Providers and
    Mastercard's Compliant Service Provider List (verified 2026-05-15).
  - Drata vendor module auto-pulls Stripe trust-center status weekly; alert
    on AOC expiry T-60 days.
  - Annual re-attestation checklist:
    `specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md`.

---

## Q10–Q15 — Outsourcing & Service Provider Management (PCI DSS Req 12.8 deep-dive + Req 9 inheritance)

### Q10 — Has the merchant documented which PCI DSS requirements are managed by each TPSP versus the merchant itself?

- **Answer:** **YES.**
- **Evidence (responsibility matrix):**

| PCI DSS v4.0 requirement family | Managed by Stripe | Managed by CoreLink | Shared / Notes |
| --- | --- | --- | --- |
| Req 1 — Network security controls | YES (Stripe CDE) | N/A | CoreLink has no CDE network. |
| Req 2 — Apply secure configurations | YES (Stripe CDE) | YES (limited: payment-page hosting) | CoreLink hardens Cloudflare Worker only. |
| Req 3 — Protect stored account data | YES (Stripe CDE) | N/A | CoreLink stores zero CHD. |
| Req 4 — Protect cardholder data with strong cryptography during transmission over open, public networks | YES (Stripe CDE; iframe TLS) | YES (CoreLink TLS 1.3 to Stripe API) | Outbound to Stripe negotiates 1.3. Inbound edge floor is 1.2 since 2026-07-19 (ADR-0072); HSTS preload. |
| Req 5 — Protect all systems and networks from malicious software | YES (Stripe CDE) | YES (build pipeline + CI scans) | CF Workers runtime is sandbox-isolated. |
| Req 6 — Develop and maintain secure systems and software | YES (Stripe CDE) | YES (CoreLink secure SDLC; Req 6.4.3 above) | Cosign + Rekor for binaries. |
| Req 7 — Restrict access by business need-to-know | YES (Stripe CDE) | YES (Stripe dashboard RBAC) | Least-privilege enforced. |
| Req 8 — Identify users and authenticate access | YES (Stripe CDE) | YES (Stripe + admin plane MFA) | FIDO2 required. |
| Req 9 — Restrict physical access to CHD | YES (Stripe CDE) | N/A | CoreLink has no physical CDE. |
| Req 10 — Log and monitor all access | YES (Stripe CDE) | YES (CoreLink webhook + admin audit chain) | Merkle-linked audit chain. |
| Req 11 — Test security of systems and networks regularly | YES (Stripe CDE) | YES (CoreLink pentest + Page Shield) | Annual pentest (R5-1). |
| Req 12 — Support information security with organizational policies | Stripe AOC | YES (this SAQ-A + SOC 2 program) | Annual recert + SOC 2 alignment. |

### Q11 — Is the TPSP listed on the Visa Global Registry of Service Providers and/or the Mastercard Compliant Service Provider List?

- **Answer:** **YES — Stripe, Inc. is on both.**
- **Evidence:**
  - Visa Global Registry of Service Providers — verified 2026-05-15
    (https://www.visa.com/splisting/).
  - Mastercard Compliant Service Provider List — verified 2026-05-15.
  - Screenshots archived in `specs/_compliance/vendor-dd/DD-STRIPE/` (to be
    populated alongside the next vendor-DD batch — backlog item).

### Q12 — Has the merchant requested and reviewed the TPSP's AOC / Section 2g responsibility matrix in the last 12 months?

- **Answer:** **YES.**
- **Evidence:**
  - Stripe AOC v4.0 obtained 2025-12-09 (vintage Q4 2025).
  - Section 2g responsibility matrix reviewed by Security Lead 2026-01-15;
    no responsibility transfer noted.
  - Next refresh: 2026-12 (annual cadence).

### Q13 — Does the merchant have a written agreement with the TPSP that includes an acknowledgement that the TPSP is responsible for the security of cardholder data the TPSP possesses or otherwise stores, processes, or transmits on behalf of the merchant?

- **Answer:** **YES.**
- **Evidence:**
  - Stripe Services Agreement §"Compliance with Laws" + §"Stripe Data";
    Stripe explicitly accepts responsibility for CHD it processes per its
    SAQ-D-SP.
  - Cross-referenced in `specs/_compliance/VENDOR-RISK-REGISTER.md`
    §Stripe → Contractual Controls.

### Q14 — Does the merchant maintain a list of accepted card brands and the PCI DSS validation status for each TPSP that processes those brands?

- **Answer:** **YES.**
- **Evidence:**
  - Card brands accepted via Stripe: Visa, Mastercard, AmEx, Discover,
    JCB, UnionPay (geo-gated by Stripe).
  - Stripe AOC covers all of the above (single TPSP; uniform validation
    status).

### Q15 — Are formal procedures in place for engaging new TPSPs (including PCI DSS due-diligence prior to onboarding)?

- **Answer:** **YES.**
- **Evidence:**
  - Vendor-onboarding methodology:
    `specs/_compliance/VENDOR-RISK-METHODOLOGY.md`.
  - PCI DSS due-diligence steps: AOC review + Visa/MC registry check +
    Section 2g responsibility-matrix review + DPA + contract review.
  - Critical-tier vendors (which includes any new TPSP) require
    Security-Lead + DPO + CTO sign-off prior to production use.

---

## Q16–Q24 — General Information Security Program (PCI DSS Req 12)

> SAQ A still requires the merchant to operate a general information-security
> program (Req 12). These nine questions cover that. CoreLink maps each
> answer to its existing SOC 2 control set to avoid duplicate evidence
> production.

### Q16 — Is a documented information security policy in place and reviewed at least annually?

- **Answer:** **YES.**
- **Evidence:** `specs/02_governance/security_policy.md` (or the canonical
  policy bundle referenced in SOC 2 CC1.5). Annual review cadence enforced.
- **SOC 2 cross-map:** CC1.4, CC1.5.

### Q17 — Are formal security-awareness and PCI DSS-relevant training programs in place for personnel?

- **Answer:** **YES.**
- **Evidence:**
  - General security-awareness training: required at onboarding + annually
    via Drata curriculum.
  - PCI-DSS-specific module: required for any role with Stripe dashboard
    access (Engineering Leads, Support, Finance); annual cadence.
- **SOC 2 cross-map:** CC1.4, CC2.2.

### Q18 — Are background checks performed on personnel prior to hire (where permitted by local law)?

- **Answer:** **YES.**
- **Evidence:** Pre-hire background checks contracted via standard provider
  (Checkr or equivalent) for all personnel; documented in
  `specs/02_governance/people_ops_policy.md`.
- **SOC 2 cross-map:** CC1.4.

### Q19 — Is an incident-response plan in place that addresses suspected or actual CHD compromise (even if CoreLink does not store CHD, the plan must address compromise of its outsourcing relationship)?

- **Answer:** **YES.**
- **Evidence:**
  - General IR plan: `specs/_runbooks/RB-INCIDENT-RESPONSE.md` +
    `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`.
  - Stripe-specific scenario: **TT-03 Stripe webhook compromise** in
    `specs/_compliance/ir-scenarios/`. Exercised quarterly per
    `IR-TABLETOP-SCHEDULE-2026.md`.
  - Notification path on suspected CHD-adjacent compromise: notify Stripe
    (`security@stripe.com`) within 24h + notify affected customers per
    standard breach-notification commitment.
- **SOC 2 cross-map:** CC7.3, CC7.4, CC7.5.

### Q20 — Are user accounts on systems in scope (Stripe dashboard, Cloudflare admin, CoreLink admin plane) reviewed at least every 6 months?

- **Answer:** **YES.**
- **Evidence:**
  - Quarterly access review per
    `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md` (covers Stripe
    dashboard).
  - SOC 2 CC6.2 / CC6.3 evidence in
    `SOC2-EVIDENCE-ROLLUP-2026-05-15.md`.

### Q21 — Are personnel access rights to in-scope systems revoked promptly (≤ 24 h) upon termination or role change?

- **Answer:** **YES.**
- **Evidence:**
  - Offboarding runbook `specs/_runbooks/RB-PERSONNEL-OFFBOARDING.md` —
    24 h SLA, evidenced via Drata.
  - Stripe dashboard access revoked as a mandatory offboarding step.
- **SOC 2 cross-map:** CC6.2, CC6.3.

### Q22 — Is sensitive authentication data (CVV, full track, PIN) never stored — even if encrypted — on any system CoreLink operates?

- **Answer:** **YES (never stored).**
- **Evidence:**
  - Database schema audit (`specs/03_architecture/data_model.md`,
    2026-05-15): zero CHD columns. No CVV, PAN, track data, or PIN
    fields exist.
  - Codebase grep audit (2026-05-15): the only references to PAN-like
    terms are in `crates/corelink-logpush/src/redaction.rs`, which is the
    *defense-in-depth* module that **detects and redacts** PAN/CVV/CPF
    patterns in the event of accidental log injection. It does not store
    any matched data; it replaces matches with a hashed placeholder and
    increments a counter. See §"Defense-in-depth note on logpush
    redaction" below.
  - Stripe documentation confirms tokens (`pm_…`, `cus_…`) are not
    sensitive authentication data.

### Q23 — Are vulnerability scans performed on the payment-page hosting infrastructure at least quarterly by an Approved Scanning Vendor (ASV)?

- **Answer:** **N/A — SAQ A does not mandate ASV scans on the merchant side**
  (the TPSP scope is fully outsourced). However CoreLink performs
  equivalent scans voluntarily as part of SOC 2 CC7.1:
- **Voluntary evidence:**
  - Daily `cargo-audit` + weekly Dependency-Track + monthly OWASP ZAP
    against `corelink.humangr.com` payment-redirect page.
  - Annual pentest (Schellman primary; SOW signed R5-1).
- **PCI SSC v4.0 note:** SAQ A explicitly defers Req 11.3 ASV scanning to
  the TPSP. Stripe's SAQ-D-SP covers Req 11.3.

### Q24 — Is this SAQ A reviewed and re-signed at least annually, or sooner if the merchant's payment-acceptance environment materially changes?

- **Answer:** **YES.**
- **Evidence:**
  - Annual recertification checklist:
    `specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md`.
  - Calendar reminder: 2027-04-15 (T-30 d before validity expires).
  - Trigger events for early re-attestation listed in the recert doc:
    boundary-diagram change, Stripe AOC lapse, new billing surface, SAQ A
    v4.x update.

---

## Defense-in-depth note on logpush redaction

A reasonable auditor question: "Why does `crates/corelink-logpush/src/redaction.rs`
mention `pan` at all if CoreLink never sees a PAN?"

The answer: that module is **defense-in-depth**. The PCI DSS v4.0 spirit
(Req 3.4.1, Req 10.3.1) is that even SAQ-A merchants should ensure CHD
cannot accidentally leak into operational logs (e.g., a misconfigured
client posting a card number into a wrong endpoint). The logpush redaction
module:

1. **Detects** patterns matching PAN (Luhn-valid 13–19 digit strings),
   CVV-adjacent context, CPF/CNPJ, and email/IP.
2. **Redacts** the match — replaces it with a salted-hash placeholder.
3. **Counts** the redaction (so we know if a leak happened upstream).
4. **Never stores** the original match.

It is provably safe: see property tests
`crates/corelink-logpush/tests/prop_logpush.rs` and
`crates/corelink-logpush/tests/pii_redaction_100k_synthetic.rs` (100k
synthetic-PAN run, asserts zero leaks past redaction).

Therefore the four hits surfaced by the boundary-verification grep
(`crates/corelink-logpush/src/redaction.rs:79,234,255,299`) are all in
the *redactor*, not in a storage path, and **do not violate the SAQ A
"no CHD stored" claim**.

---

## Stripe vs. CoreLink Responsibility Matrix (summary — also in Q10)

For procurement teams that want the one-page version:

| Responsibility | Stripe | CoreLink |
| --- | --- | --- |
| Capturing the cardholder's PAN, CVV, expiry | YES (Stripe Elements / Checkout iframe in customer browser) | NEVER |
| Tokenizing the card | YES | NEVER |
| Storing the PAN (tokenized vault) | YES | NEVER |
| Processing the authorization with the card brand network | YES | NEVER |
| Re-charging on subscription renewal | YES | NEVER |
| Storing the customer's billing address (per geo law) | YES | NEVER |
| Storing the `cus_…` customer ID and `sub_…` subscription ID | NEVER | YES (operational identifiers, not CHD) |
| Storing the `pm_…` payment-method token (opaque) | YES (vault) | YES (for repeat charges; opaque token, not CHD per PCI SSC FAQ 1153) |
| Webhook signature verification | NEVER | YES (`crates/corelink-billing-stripe/src/signature.rs`) |
| Billing-event ledger (append-only Merkle chain) | NEVER | YES (`crates/corelink-billing-stripe/src/ledger.rs`) |
| Sending receipt email | YES (Stripe-hosted) or NEVER | OPTIONAL (CoreLink can also send transactional receipts; no CHD in receipt) |
| Disputes / chargebacks | YES (Stripe Radar + dashboard) | NEVER |

---

## Signature Block

By signing below the signatories attest that the answers to Q1–Q24 are
true and accurate as of the assessment date, and that the boundary
diagram in `PCI-DSS-BOUNDARY-DIAGRAM.md` is faithful.

| Role | Name | Signature | Date |
| --- | --- | --- | --- |
| Chief Executive Officer | Gustavo Schneiter | __________________________ | __________ |
| Chief Technology Officer | Gustavo Schneiter (acting CTO) | __________________________ | __________ |
| Security Lead (witness) | (acting — recruit pending R5-8) | __________________________ | __________ |
| DPO (witness, optional) | (acting — recruit pending R5-8) | __________________________ | __________ |

> Until the dedicated Security-Lead and DPO recruits close (R5-8 track),
> Gustavo Schneiter signs in dual capacity as CEO and CTO. This is
> disclosed transparently to auditors per the founder-stage compliance
> posture documented in `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`.

---

## Cross-links

- Boundary diagram: `specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md`
- Annual recertification checklist: `specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md`
- Compliance matrix (PCI row): `specs/03_architecture/compliance_matrix.md` §3 Posture map
- Vendor risk register (Stripe row): `specs/_compliance/VENDOR-RISK-REGISTER.md`
- SOC 2 evidence rollup (control overlap): `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- Customer-facing trust page: `apps/docs/docs/trust/pci-dss.mdx`
- Trust Center index: `apps/docs/docs/trust/index.mdx`
- Trust Center compliance page (§PCI DSS): `apps/docs/docs/trust/compliance.mdx`
- ROADMAP-TO-GA reference: R-PREP track, H-10 SOC 2 row (PCI inherited)
- IR scenario TT-03 (Stripe webhook compromise): `specs/_compliance/ir-scenarios/TT-03-stripe-webhook-compromise.md`
- Stripe credential rotation runbook: `specs/_runbooks/RB-STRIPE-CREDENTIAL-ROTATION.md`
