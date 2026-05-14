---
id: "CASE-STUDY-TEMPLATE-ENTERPRISE-BYOK"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "s20", "case-study", "template", "enterprise", "byok", "schrems-ii", "dpa", "governance"]
---

# Case Study Template — Enterprise BYOK (ICP)

> **Slot:** `LH-ENT-BYOK-01` · **Tier:** Enterprise BYOK
> **Audience:** prospective enterprise buyers under NDA; sanitized version sharable in sales conversations.
> **Confidentiality:** NDA-protected; ALL identifying details (customer name, region, vertical specifics) require customer sign-off + Legal review before publication.

---

## 1. Problem statement (≤ 200 words, sanitized)

_Describe the customer's compliance posture + crypto-sovereignty requirements pre-CoreLink. Anchor to **anonymized** but specific facts: vertical (e.g. "EU financial services"), regulatory regime (e.g. "EBA + Schrems II"), KMS provider, scale (e.g. "X TB stored, Y operations/day")._

Suggested skeleton: "A {vertical} customer operating across {region} required a build/CAS infrastructure provider that could honor (1) crypto-sovereignty via BYOK, (2) Schrems II TIA-backed DPA, (3) sub-300ms p99 cache GET latency, and (4) verifiable erasure attestations. Their incumbent stack {generic description} could not satisfy {specific requirement}..."

DO NOT name the customer in the public-sanitized version. The NDA-internal version may name them with customer sign-off.

---

## 2. Before-state requirements

| Requirement | Pre-CoreLink state |
|---|---|
| **Crypto-sovereignty (BYOK)** | _customer-managed AWS KMS in customer account_ |
| **Data residency** | _EU-only (no US transit)_ |
| **DPA + Schrems II TIA** | _required; in negotiation with prior vendor_ |
| **Erasure attestations** | _Ed25519-signed required for audit_ |
| **Build / CAS scale** | _XX TB stored; YY ops/day; ZZ engineers_ |
| **SLA expected** | _99.9% availability + sub-300ms p99 GET_ |
| **Compliance regime** | _GDPR + LGPD + EBA + FedRAMP-Moderate ambitions_ |

---

## 3. Integration story (≤ 500 words, sanitized)

_Narrate the migration. Enterprise BYOK migrations are heavier — emphasize:_

- **DPA + Schrems II TIA cycle** — duration (target ≤ 6 weeks), Legal teams involved, sub-processor list reviewed (S-14 deliverable).
- **BYOK provider setup** — AWS KMS / GCP KMS / Azure KV / Vault; admin UI wizard (S-16); kill-switch test prior to migration.
- **Tier selection + Stripe enterprise contract** — custom pricing; net-30 invoice (NOT credit card).
- **Region selection** — single-region or multi-region; data-residency enforcement (S-14 INV-DATA-RESIDENCY).
- **Migration day** — typically split across multiple sessions (Day 1: signup + DPA; Day 2-3: BYOK + provisioning; Day 4: first PAT + first CAS PUT).
- **Engineering team training** — at least 2 sessions for customer engineering org.

This narrative is **the** primary asset for closing future enterprise deals. Every fact must be customer-approved before publication.

---

## 4. Results (30d observation)

| Metric | Pre-CoreLink (target) | Post-CoreLink (30d actual) |
|---|---|---|
| Cache GET p99 latency | < 300 ms | _XXX ms_ |
| Availability (CAS PUT + GET) | ≥ 99.9% | _99.9X%_ |
| Erasure SLO | ≤ 30d | _N/A (no DSR triggered) / X days_ |
| BYOK kill-switch chaos drill | ≤ 5 min p99 | _X min_ |
| BYOK key rotation tested | yes | _yes / no_ |

### 4.1 SLA evidence (BYOK-specific)

| SLO / Health metric | Target | Actual (30d) |
|---|---|---|
| SLO-AVAIL-CAS-PUT | ≥ 99.9% | _99.9X%_ |
| SLO-AVAIL-CAS-GET | ≥ 99.9% | _99.9X%_ |
| SLO-LAT-CAS-GET p99 | < 300 ms | _XXX ms_ |
| SLO-FRESH-BILLING | < 0.1% drift / 24h | _0.0X%_ |
| SLO-BYOK-KILL-SWITCH | ≤ 5 min p99 | _X min_ |
| BYOK matrix-test (4 providers) | 4/4 verde | _4/4_ |
| Erasure attestation Ed25519 verifiable | yes (if DSR) | _yes / N/A_ |

---

## 5. Customer quote (slot)

> _"[Quote from customer CTO / Head of Security / DPO — 2-4 sentences emphasizing crypto-sovereignty + DPA quality + measured SLA delivery.]"_
>
> — _Title, Vertical, Geography_ (e.g. "VP Engineering, EU financial services")

Quote MUST be approved by customer Legal + Communications teams. Sanitized version is the only one published; named version stored under NDA in `specs/_audits/` with restricted access.

---

## 6. Technical architecture diagram (slot, sanitized)

```
[ insert Mermaid / SVG diagram here ]

Customer engineers ──► CoreLink CAS API
                            │
                            ├─► R2 storage (envelope encryption)
                            │       │
                            │       └─► Customer-managed KMS
                            │              ({provider} in customer account)
                            │
                            ├─► Audit chain (signed, append-only)
                            │       └─► Erasure attestation (Ed25519)
                            │
                            └─► Region: {region} (data-residency enforced)
                                   └─► DPA amendment Schrems II TIA active
```

Enterprise diagram MUST show:
- BYOK provider as customer-controlled (NOT inside CoreLink trust boundary).
- Region enforcement boundary.
- DPA / sub-processor overlay (per S-14 deliverable).

Sanitize ALL specific KMS provider, region, and customer account references for the public version.

---

## 7. Future plans (≤ 150 words, sanitized)

_Customer's roadmap with CoreLink: multi-region expansion, additional BYOK provider adoption, integration with their incident response workflow, possible expansion to other CoreLink products._

---

## 8. Publication metadata

| Field | Value |
|---|---|
| Customer-approved (sanitized version) | _yes / no_ |
| Customer-approved (NDA-named version) | _yes / no_ |
| Legal review status (sanitized) | _approved / in review_ |
| Legal review status (NDA-named) | _approved (Legal Counsel signed)_ |
| Sanitization required | **YES** (default — see below) |
| Publication venue (sanitized) | _CoreLink website + sales materials_ |
| Publication venue (NDA-named) | _under NDA to specific prospects in sales pipeline_ |
| Embargo | _none after GA D+60_ |
| Co-authors | _CoreLink Marketing + Customer Communications + Customer Legal_ |

### 8.1 Mandatory sanitization checklist (sanitized version)

- [ ] Customer name removed; replaced with vertical + geography descriptor.
- [ ] Specific KMS provider replaced with generic ("a major cloud KMS").
- [ ] Specific region replaced with descriptor ("EU primary region").
- [ ] Specific contract size / pricing redacted.
- [ ] Customer engineering team identifiers redacted from quote attribution.
- [ ] DPA-specific language reviewed by Privacy Officer.

---

**Template for Enterprise BYOK case study; ALWAYS instantiate per real customer attestation under NDA. Mandatory Legal review per WI-S20-005 DPA + sub-processor agreement scope.**
