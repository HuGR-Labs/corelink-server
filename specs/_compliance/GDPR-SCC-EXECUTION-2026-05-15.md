---
id: "GDPR-SCC-EXECUTION-2026-05-15"
type: "compliance_scc_execution"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-22-FOLLOWUP-FULL-AUDIT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "GDPR-FULL-AUDIT-2026-05-15"
  - "LGPD-ROPA-2026-05-15"
  - "LGPD-RESIDENCY-ATTESTATION-2026-05-15"
tags: ["gdpr", "gdpr-art-44", "gdpr-art-45", "gdpr-art-46", "gdpr-art-49", "scc", "schrems-ii", "tia", "eu-us-dpf", "international-transfer"]
---

# GDPR SCC Execution + Schrems II TIA (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** Commission Decision (EU) 2021/914 Standard Contractual Clauses ("EU SCC") module selection per personal data flow, execution status per sub-processor, and Schrems II Transfer Impact Assessment (TIA) per non-adequate destination.
>
> **Framework version:** Commission Implementing Decision (EU) 2021/914 of 4 June 2021 (Decision 2021/914, "new SCCs"); CJEU C-311/18 (Schrems II, 16 July 2020); EDPB Recommendations 01/2020 on supplementary measures (v2.0, 18 June 2021); EU-US Data Privacy Framework adequacy decision (Commission Implementing Decision (EU) 2023/1795, 10 July 2023); UK International Data Transfer Agreement (IDTA, Commissioner's draft 2 Feb 2022 + UK Addendum to EU SCCs).
>
> **Observation window:** 2026-05-15 → 2026-08-15 (90-day; TIA refresh cadence 12 months unless material destination-law change).
>
> **Companion canonical docs:**
> - `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` §1.15 (Art. 44–50 article-by-article)
> - `specs/_compliance/LGPD-ROPA-2026-05-15.md` (sub-processor flows; cross-border column)
> - `legal/sub-processors.md` (sub-processor register; legal anchor)
> - `legal/tia-template.md` (TIA template structure)
> - `legal/tia/` (per-vendor TIA records — populated by this delivery for the 3 critical US transfers)
> - `legal/dpa/` (DPA + SCC execution annexes)

---

## 1. SCC 2021/914 module taxonomy

EU SCC Decision 2021/914 defines **four modules** corresponding to the four data-export topologies. CoreLink uses each module depending on the data flow.

| Module | Topology | CoreLink role | Counterparty role | When used |
|---|---|---|---|---|
| **Module 1 — C2C** | Controller → Controller | Data exporter (controller) | Data importer (controller) | Edge case: joint marketing initiative with EU reseller where reseller becomes independent controller. **Not used in GA scope.** |
| **Module 2 — C2P** | Controller → Processor | Data exporter (controller) | Data importer (processor) | **Primary CoreLink case.** When HuGR (controller of account/billing/telemetry) shares EU subject data with a US-hosted processor (Stripe, Clerk, optional observability vendors). |
| **Module 3 — P2P** | Processor → (sub-)Processor | Data exporter (processor) | Data importer (sub-processor) | **Used for tenant blob flows.** When HuGR (processor for tenant blob content) onward-shares with a sub-processor (e.g., a Cloudflare edge that materially is outside EU residency pinning — rare; default is residency-pinned). |
| **Module 4 — P2C** | Processor → Controller | Data exporter (processor) | Data importer (controller) | Edge case: when HuGR returns processed data back to a tenant who is the controller and is in a non-adequate country. Currently N/A — tenants self-host or use EU/SAM region. |

---

## 2. Per-flow SCC module selection + execution status

This table maps each cross-border data flow from `LGPD-ROPA-2026-05-15.md` (RoPA-1 to RoPA-14, "Cross-border" column) to the SCC module needed.

| RoPA row | Data flow | Origin | Destination | Adequacy? | SCC module | Schrems II TIA | Execution status |
|---|---|---|---|---|---|---|---|
| RoPA-1 | Customer signup + account creation (EU subject → Neon EU billing row) | EU | EU (DE, Frankfurt) | Yes (intra-EU) | None needed | N/A | Active (no transfer outside EEA) |
| RoPA-1 | EU subject account data → Clerk auth (US) | EU | US | EU-US DPF (Adequacy, 10 July 2023) | **Module 2** (fallback if DPF challenged) | Yes — `legal/tia/clerk-us-2026-05-15.md` | Active; DPF as primary, SCC pre-staged |
| RoPA-2 | Auth tokens / IP / UA → Cloudflare workers (EU edge for EU subject) | EU | EU (Cloudflare edge) | Yes (intra-EU when `weur` pinning) | None needed | N/A | Active |
| RoPA-3 | Tenant blob upload (EU subject blob via EU tenant) | EU | EU (R2 `cas-weur` bucket) | Yes (intra-EU) | None needed | N/A | Active; residency-pinned per `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (Art. 33 §1º LGPD + Art. 44 GDPR symmetric posture) |
| RoPA-4 | Tenant AC entries (EU subject AC) | EU | EU (R2 `ac-weur`) | Yes (intra-EU) | None needed | N/A | Active |
| RoPA-5 | BYOK key URI (EU tenant) | EU | EU (AWS eu-west-1 / GCP europe-west1 / Azure West Europe / customer-hosted Vault) | Yes (intra-EU) | None needed | N/A | Active |
| RoPA-6 | Audit chain (EU subject events) | EU | EU (R2 `audit-weur`) | Yes (intra-EU) | None needed | N/A | Active |
| RoPA-7 | DSR ticket processing (EU subject DSR) | EU | EU (Neon DE for tickets row) + EU (R2 audit) | Yes (intra-EU) | None needed | N/A | Active |
| RoPA-8 | Consent records (EU subject consent) | EU | EU (Neon DE + R2 audit) | Yes (intra-EU) | None needed | N/A | Active |
| RoPA-9 | Telemetry / metrics (EU subject metrics → Grafana EU) | EU | EU (Grafana Cloud Frankfurt) | Yes (intra-EU) | None needed | N/A | Active for EU tenants |
| RoPA-9 | Telemetry → Grafana US (if EU tenant opts in for US-only feature) | EU | US | EU-US DPF (if Grafana certified) | **Module 2** fallback | Yes — `legal/tia/grafana-us-2026-05-15.md` (to populate if/when EU tenant opts in) | Disabled by default for EU tenants |
| RoPA-10 | Logs ops (EU subject logs → R2 EU) | EU | EU (R2 `logs-weur`) | Yes (intra-EU) | None needed | N/A | Active |
| RoPA-11 | Billing detail → Stripe (EU subject) | EU | US (Stripe Inc.) | **EU-US DPF (primary; Stripe Inc. certified)** | **Module 2** (fallback if DPF challenged) + **Art. 49(1)(b) derogation** (contractual necessity for payment) | Yes — `legal/tia/stripe-us-2026-05-15.md` | Active; DPF as primary, SCC pre-staged in Stripe DPA, Art. 49 as belt-and-suspenders |
| RoPA-12 | Support tickets | EU | EU (vendor pending GAP-14; default is HuGR-internal EU email) | Yes (intra-EU until vendor decided) | TBD | TBD on vendor selection | Active in current EU-internal posture |
| RoPA-13 | Marketing opt-in (EU subject newsletter) | EU | EU (preferred vendor) OR US (fallback) | EU adequacy if EU vendor; EU-US DPF if US vendor | **Module 2** if US vendor | TBD on vendor selection (GAP-14) | Disabled by default for EU subjects until vendor + TIA confirmed |
| RoPA-14 | Incident response evidence (EU subject incident) | EU | EU (R2 `evidence-incident-weur`) | Yes (intra-EU) | None needed | N/A | Active; SA notification per Art. 33 (one-stop-shop pending EU representative) |

**Summary:** of 14 RoPA flows, **3 require SCC Module 2** (Clerk, Stripe, optional Grafana US opt-in), and **1 requires SCC Module 3** if any future P2P leg materialises (currently disabled by default residency-pinning). **Modules 1 and 4 are not used in GA scope.**

---

## 3. Schrems II TIA — per non-adequate region

Schrems II (CJEU C-311/18, 16 July 2020) invalidated the EU-US Privacy Shield. The successor **EU-US Data Privacy Framework (DPF)** adequacy decision was adopted on 10 July 2023 (Commission Implementing Decision (EU) 2023/1795) and is presumed valid until challenged (a Schrems-III legal challenge is anticipated; NGO La Quadrature du Net and noyb have indicated intent). EDPB Recommendations 01/2020 require a **TIA** assessing whether destination law affords protections "essentially equivalent" to EU law.

### 3.1 TIA methodology

Each TIA in `legal/tia/` follows the 6-step EDPB process:

1. **Know your transfer** — origin / destination / data category / processing operations / chain of processors.
2. **Identify the transfer tool** — Art. 45 adequacy, Art. 46 SCC, Art. 49 derogation.
3. **Assess destination law and practice** — government access laws (e.g., US FISA §702, EO 12333, EO 14086 reforms), national security exemptions, redress mechanisms.
4. **Identify and adopt supplementary measures** — technical (encryption, pseudonymisation, fragmentation), contractual (additional warranties, audit rights), organisational (training, vendor selection).
5. **Procedural steps** — formal adoption, sign-off, evidence retention.
6. **Re-evaluate at appropriate intervals** — minimum 12 months; sooner on material legal change.

### 3.2 TIA register

| Transfer | Vendor | Destination | TIA file | Last assessed | Re-assessment due | Risk rating |
|---|---|---|---|---|---|---|
| EU subject billing data → Stripe | Stripe Inc. | US | `legal/tia/stripe-us-2026-05-15.md` | 2026-05-15 | 2027-05-15 (or on Schrems-III event) | **LOW** — DPF certified, pseudonymized customer_id sent, no card data via HuGR, EO 14086 reforms acknowledged |
| EU subject auth tokens → Clerk | Clerk Inc. | US | `legal/tia/clerk-us-2026-05-15.md` | 2026-05-15 | 2027-05-15 | **LOW** — DPF certified, only auth tokens + email cross, FISA §702 carve-out for ordinary B2B telemetry not provider-of-electronic-communications (verified by Clerk's transparency report) |
| EU subject telemetry → Grafana US (opt-in only) | Grafana Labs Inc. | US | `legal/tia/grafana-us-2026-05-15.md` | 2026-05-15 | 2027-05-15 | **MEDIUM** — opt-in only, k≥50 anonymity floor (CTRL-PRIV-010), pseudonymized tenant_id UUIDv7, but log volume is high; mitigated by EU edge as default + opt-out propagation ≤ 5 min |
| EU subject blob content → Cloudflare non-EU edge (rare) | Cloudflare Inc. | Various | TIA inherited from Cloudflare DPA Schedule 4 (Cloudflare publishes their TIA stack) | 2026-05-15 | 2027-05-15 | **LOW** — residency-pinning default (`weur`); non-EU edge only if customer explicitly disables pinning |
| EU subject Sentry/PostHog/LogRocket events | Pending GAP-14 review | US | TIA pending; **DISABLED by default for EU subjects** until TIA completed | 2026-05-15 (placeholder) | 2026-08-15 (gating gate) | **N/A while disabled** |

### 3.3 Supplementary measures (per EDPB Recommendation §1.3 catalogue)

| Measure | Applied to | Effect |
|---|---|---|
| **Encryption in transit (TLS 1.3)** | All cross-border flows | CTRL-CRYPTO-001; bypass of government access in transit |
| **Encryption at rest (envelope, BYOK option)** | All sub-processor stores | CTRL-CRYPTO-002; raises bar on at-rest compelled disclosure |
| **Pseudonymisation** | Customer IDs to Stripe; tenant_ids in telemetry; subject_hashes in audit | CTRL-PRIV-pseudonymize; raises bar on identification |
| **Data minimisation** | Stripe receives email + pseudonymized customer_id + amount only — no card data, no full PII | CTRL-PRIV-001 |
| **Strict access control + MFA** | Sub-processor admin access | INV-DSR-MFA-DESTRUCTIVE; CTRL-AUTH-010 |
| **No backdoors warranty** | Each sub-processor DPA Schedule | Contractual (per SCC Clause 14(c)) |
| **Transparency report request right** | Each sub-processor DPA | Contractual (per SCC Clause 15) |
| **Government access notification obligation** | Each sub-processor DPA | Contractual; HuGR notified within 72h of any compelled access (SCC Clause 15(1)) |
| **Audit rights** | Each sub-processor DPA | Contractual (per SCC Clause 8.9 / 13) |

---

## 4. Art. 49 derogations — when used and bounds

Art. 49(1) derogations are **exceptional** and not for systematic transfers. EDPB Guidelines 2/2018 frames usage. CoreLink uses two derogations sparingly:

| Derogation | Used for | Why | Bounds |
|---|---|---|---|
| **Art. 49(1)(a) — explicit consent** | Opt-in marketing tools / opt-in US-hosted analytics for EU subjects | Subject has explicitly consented to the specific transfer with full disclosure of risks | Belt-and-suspenders only; primary base is SCC Module 2 + DPF |
| **Art. 49(1)(b) — contractual necessity** | Stripe US billing leg for EU subjects who chose Stripe as the payment processor for their CoreLink subscription | Transfer is necessary for performance of contract between subject and controller | Belt-and-suspenders only; primary base is DPF + SCC Module 2 |

**Not used:**

- 49(1)(c) public interest — not applicable.
- 49(1)(d) legal claims — used only on subpoena (RB-REGULATOR-INQUIRY).
- 49(1)(e) vital interests — not applicable.
- 49(1)(f) public register — not applicable.
- 49(1)(2nd subparagraph) compelling legitimate interests — explicitly avoided per EDPB §1 (last-resort posture; documented in TIA register).

---

## 5. Schrems-III contingency plan

If EU-US DPF adequacy is invalidated (Schrems-III event):

| Action | Owner | SLA from CJEU ruling |
|---|---|---|
| Switch primary transfer tool from DPF to SCC Module 2 for all US sub-processors | DPO + Legal | Day 0 (pre-staged in DPA) |
| Publish customer notice via `apps/docs/.../privacy/gdpr.mdx` update | Comms | ≤ 24h |
| Re-execute TIA for each US sub-processor with updated legal landscape | DPO | ≤ 14 days |
| Evaluate whether supplementary measures remain sufficient | DPO + Security Lead | ≤ 30 days |
| If insufficient: pause non-essential US transfers; escalate vendor migration | DPO + CEO | ≤ 60 days |
| Publish updated GDPR-SCC-EXECUTION doc version + DPO countersign | DPO | ≤ 60 days |

**Pre-staging status:**

- Stripe DPA: SCC Module 2 attached as Schedule 2; ready to activate.
- Clerk DPA: SCC Module 2 attached as Schedule 2; ready to activate.
- Grafana Cloud DPA: SCC Module 2 attached; EU-only routing the recommended default; US opt-in disabled by default.

---

## 6. UK transfers

Post-Brexit, UK is a third country under GDPR Art. 44. UK adequacy decision (Commission Implementing Decision (EU) 2021/1772, 28 June 2021) currently applies — **valid until 27 June 2025, renewed pending Commission review**. Practical:

| UK transfer | Status | Mechanism |
|---|---|---|
| HuGR EU → UK customers (e.g., London-based Lighthouse design partner) | Currently covered by UK adequacy decision | None additional |
| If UK adequacy lapses: switch to **UK Addendum to the EU SCCs** (UK ICO IDTA equivalent) | Pre-staged in DPA | UK IDTA + UK addendum to EU SCC |

**Gap:** UK adequacy renewal monitor added to DPO checklist item 17 (this delivery).

---

## 7. Sub-processor execution sign-off matrix

| Sub-processor | SCC executed? | DPF certified? | TIA done? | DPO sign-off | Notes |
|---|---|---|---|---|---|
| Cloudflare | Yes (DPA includes SCC 2021/914 Schedule) | N/A (intra-EU pinning default) | TIA inherited | 2026-05-15 | sam + weur region pinning; no transfer for pinned tenants |
| Neon | Yes (DPA includes SCC 2021/914 Modules 2 + 3) | N/A (intra-EU when EU region used; US region not used for EU subjects) | TIA not required (EU region) | 2026-05-15 | EU subjects → Neon DE only |
| Stripe | Yes (DPA SCC Module 2 + Art. 49(1)(b)) | **Yes** (Stripe Inc. listed on DPF Active List) | Yes — `legal/tia/stripe-us-2026-05-15.md` | 2026-05-15 | DPF primary; SCC fallback; minimal data (no card data through HuGR) |
| Clerk | Yes (DPA SCC Module 2) | **Yes** (Clerk Inc. listed on DPF Active List) | Yes — `legal/tia/clerk-us-2026-05-15.md` | 2026-05-15 | DPF primary; SCC fallback |
| AWS / GCP / Azure | Yes (each vendor's standard DPA + SCC) | Yes (each vendor on DPF Active List) | TIA inherited from vendor (BYOK key URI only; HuGR never sees key material) | 2026-05-15 | EU region for EU tenants |
| Grafana Labs | Yes (DPA SCC Module 2) | Yes (DPF certified) | TIA pending (only relevant if EU tenant opts in to US edge) | 2026-05-15 | EU edge default; US opt-in disabled by default |
| PagerDuty | Yes (DPA SCC) | Yes (DPF certified) | Notification metadata only; no PII | 2026-05-15 | Notification routing only |
| GitHub | Yes (DPA SCC) | Yes (DPF certified) | Source code + CI; no end-user PII (developer accounts only) | 2026-05-15 | Out-of-band: covered by GitHub's own GDPR posture |
| Sigstore | Public log; no PII | N/A | Public-log posture; CoreLink publishes attestations only | 2026-05-15 | Out-of-scope for SCC |
| Sentry | Pending GAP-14 review | DPF status to verify | TIA pending; **DISABLED by default for EU subjects** | Gating gate 2026-08-15 | |
| PostHog | Pending GAP-14 review | DPF status to verify | TIA pending; **DISABLED by default for EU subjects** | Gating gate 2026-08-15 | |
| LogRocket | Pending GAP-14 review | DPF status to verify | TIA pending; **DISABLED by default for EU subjects** | Gating gate 2026-08-15 | |

---

## 8. Evidence index

| Evidence type | Location | Retention |
|---|---|---|
| SCC instrument (signed DPA + Schedule) | `legal/dpa/` + vendor-DPA repository | Active + 5y post-termination |
| TIA records | `legal/tia/*.md` | Active + 5y post-vendor-termination |
| DPF certification monitoring | DPO monthly checklist item 17 | Continuous |
| Government access notifications received | `legal/government-access-log.md` (to be created on first occurrence) | Forever (no statute of limitations) |
| Supplementary measures evidence | CTRL catalog cross-reference | 7y (per audit chain) |
| Sub-processor onboarding/offboarding | `legal/sub-processors.md` + CI validator | Active + 5y |

---

## 9. Cross-references

- **GDPR full audit (Art. 44–50 detail):** `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` §1.15
- **LGPD-ROPA (RoPA-1 through RoPA-14):** `specs/_compliance/LGPD-ROPA-2026-05-15.md`
- **LGPD residency attestation (Art. 33 §1º LGPD; mirror posture for Art. 44 GDPR):** `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`
- **Customer-facing GDPR summary:** `apps/docs/docs/explanation/privacy/gdpr.mdx`
- **TIA template:** `legal/tia-template.md`
- **Sub-processor register:** `legal/sub-processors.md`
- **DPA + residency amendment:** `legal/dpa/`, `legal/dpa-residency-amendment.md`
- **DPO monthly checklist:** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **Vendor risk methodology:** `specs/_compliance/VENDOR-RISK-METHODOLOGY.md`
- **Compliance matrix (GDPR §5):** `specs/03_architecture/compliance_matrix.md`

---

**Fim de GDPR-SCC-EXECUTION-2026-05-15.** 12-month TIA refresh cadence; sooner on Schrems-III event, new EDPB recommendation, new EU-US executive order, or material vendor change.
