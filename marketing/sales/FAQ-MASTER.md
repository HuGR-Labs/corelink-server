---
id: "SALES-FAQ-MASTER"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "R-PREP-SALES-ENABLEMENT"
tags: ["sales", "faq", "objection-handling", "r-prep", "ga", "customer-facing-source"]
---

# CoreLink Sales FAQ — Master (50 Questions)

> **Audience:** anyone running a CoreLink pre-purchase conversation — Founder, Customer Success, partner SE. Each answer is the **canonical phrasing**. Use it as-is in a doc; paraphrase in conversation.
> **Tone:** factual, falsifiable, no marketing varnish. Every numeric claim has a source pointer in `PROOF-POINTS.md`.
> **What this is not:** a contract. The DPA, MSA, and order form override anything here.
> **Companion docs:** `marketing/sales/OBJECTION-HANDLING.md` · `marketing/sales/COMPETITIVE-MATRIX.md` · `marketing/sales/PROOF-POINTS.md` · `apps/docs/docs/trust/` · `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` · `marketing/launch/LAUNCH-CHECKLIST-V2.md` · `marketing/sales/legal-questionnaires/` (SIG Lite + CAIQ v4 + vendor template + evidence index + response SLA).

---

## How to use this file

- The 50 questions are grouped into six topic blocks: **Pricing (8), Security (12), Compliance (8), Performance (6), Operations (8), Migration (8)**.
- Each entry has: **Q**, **A** (short canonical answer), **Sources** (doc / spec / commit pointers).
- If a prospect's question isn't here, route to `trust@humangr.com` rather than improvising — and file the new question against this doc.

---

## Pricing (8)

### P1 — How do the tiers compare (Free / Team / Lighthouse / Enterprise)?

**Q:** What do I actually get at each tier?

**A:** Four tiers; the differences that matter at sales time are storage cap, egress allowance, TPS ceiling, audit retention, BYOK availability, region count, and the SLA we sign.

| Limit | **Sandbox** | **Free** | **Team** | **Lighthouse** | **Enterprise** |
| --- | --- | --- | --- | --- | --- |
| Storage cap | 1 GiB | 10 GiB | 100 GiB | 1 TiB | unlimited |
| Egress / mo | 5 GiB | 50 GiB | 500 GiB | 5 TiB | negotiated |
| TPS (put/get) | 50 | 100 | 500 | 5000 | negotiated |
| Audit retention | 24h | 14d | 90d | 365d | 365d+ |
| BYOK | no | no | no | no | yes |
| Regions available | 1 (auto) | 1 | 2 | 4 | 4 + cross-region |
| SLA | none | none | 99.5% | 99.9% | 99.95% |

The egress numbers are *generous* by build-cache standards because the underlying R2 substrate has zero egress fees on cache reads (see `BLOG-POSTS/05-fast-cache-hit-economics.md`); the cap exists to prevent abuse, not to extract bandwidth rent.

**Sources:** `apps/docs/docs/tutorials/quickstart-faq.mdx#8`; `marketing/launch/BLOG-POSTS/01-introducing-corelink.md#pricing`.

### P2 — How does overage work? Do we get rate-limited or billed?

**Q:** If we blow through our tier's storage / TPS / egress cap, what happens?

**A:** Soft-cap then notify, then negotiate. Concretely: at 80% of any cap we emit a webhook + email; at 100% we serve a `429 / overage-pending` for **TPS** (rate-limited, not refused — your build still completes, just slower) and continue serving **storage / egress** with the overage line appearing on the next invoice. We *do not* hard-fail builds in production tenants when caps are exceeded; we'd rather invoice you than break your inner loop. For Enterprise the overage line is governed by your order form; for Team it's metered at our standard list rate. We are happy to convert a recurring overage into a contractual tier upgrade with no penalty (see P5 — migration discount).

**Sources:** SLO catalog (`docs.corelink.humangr.com/slo`); overage runbook (`specs/_runbooks/RB-BILLING-OVERAGE.md`); quickstart-faq Q8.

### P3 — Is BYOK a premium? How much more?

**Q:** What does BYOK actually cost on top of Enterprise base?

**A:** BYOK is an Enterprise-tier capability — it's not sold as an add-on to Team. The premium versus a hypothetical "Enterprise without BYOK" is governed by your order form (negotiated; Sales recreates from the live Finance model — `marketing/lighthouse-kit/07-pricing-comparison-internal.md` §5). The premium reflects four real costs: (1) per-tenant KMS call volume to your AWS / GCP / Azure / Vault provider, (2) dedicated incident channels for kill-switch events, (3) FIPS-endpoint pinning per provider, and (4) the weekly synthetic kill-switch chaos drill we run on your tenant. No, we don't run BYOK as a marketing checkbox; the kill switch is exercised on a schedule and recorded in the audit chain.

**Sources:** `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md`; `apps/docs/docs/trust/data-handling.mdx#encryption`.

### P4 — What enterprise discounts are available?

**Q:** What levers do you have on price?

**A:** Three structural levers, in order of usual impact: (1) **multi-year commit** — 1y / 2y / 3y discounts; the 3y is the largest single move and triggers Founder-level approval. (2) **annual prepay** vs. monthly. (3) **volume commit** — committed storage / egress / TPS bands negotiated above the next-tier list. We do *not* discount on SLO (the SLO catalog is canonical — adding or weakening SLOs requires a spec contract waiver, see `marketing/lighthouse-kit/07-pricing-comparison-internal.md` §4). We do *not* discount on BYOK kill-switch latency or audit-chain retention — both are structural invariants, not negotiable line items.

**Sources:** `marketing/lighthouse-kit/07-pricing-comparison-internal.md` §4 (decision rules for negotiating off standard offer).

### P5 — Migration discount: what is it and who qualifies?

**Q:** If we're switching from bazel-remote / BuildBuddy / a self-hosted cache, do we get a transition price?

**A:** Yes — the **migration credit**. Customers actively decommissioning a competing remote-cache deployment receive 3 months at 50% of list on Team tier (or Enterprise equivalent), conditional on (a) signing the standard MSA, (b) committing to a minimum 12-month term, and (c) participating in a non-binding 30-minute "what made you switch" interview at month 4 (used internally for product roadmap; not published without your sign-off). The credit does not stack with the lighthouse program (which is more generous but capacity-constrained — see P7).

**Sources:** `marketing/lighthouse-kit/07-pricing-comparison-internal.md`; sales decision tree (internal).

### P6 — Churn refund: if we leave, do we get money back?

**Q:** If we cancel mid-term, what's the refund posture?

**A:** Two cases. (1) **You cancel for convenience** (mid-annual-prepay): you receive a prorated refund of unused months minus a 30-day notice equivalent. We do not enforce minimum-term penalties beyond the unused-prepay clawback. (2) **You cancel for cause** (an SLA breach we've acknowledged via the public status page, or a material DPA breach): full refund of the current paid period plus an exit-assistance window. The audit-chain export is included free in either case — your data is yours, content-addressed, and portable by construction (you can leave any time; we are aware "BLAKE3 digests are content-addressed" is *itself* an escape hatch, see P34).

**Sources:** standard MSA §10 (cancellation); `legal/dpa/v1.0.0` §11 (termination).

**Self-service surface.** Team-tier and below cancel through the Stripe Customer Portal — see the customer guide at `apps/docs/docs/how-to/billing/manage-subscription.mdx` (published as `/how-to/billing/manage-subscription`). Enterprise cancellation routes through your sales contact for the paper amendment + DPA closure (see `specs/_audits/sealed/2026-05-15-stripe-customer-portal-spec.md` §2.2 — Enterprise downgrade rule).

### P7 — Can we sign a multi-year contract?

**Q:** What does 2y or 3y look like?

**A:** Both available. The trade is depth-of-discount-for-commit-length, with two protections written in:

- **Price-lock for the term.** List-price increases during the term do not apply to you.
- **Annual re-baseline option.** You can renegotiate downward (not upward) once per year if your actual usage falls below the committed band — we don't want to be the vendor your CFO has to justify a clawback against.
- **Exit ramp.** If at the 12-month or 24-month mark you'd rather not continue, you give us 90 days notice and a one-time exit fee that is *less* than the discount you've already received. We make the math defensible internally and verbal in the negotiation.

**Sources:** sales playbook (internal); `marketing/lighthouse-kit/07-pricing-comparison-internal.md` §4.

### P8 — Is pricing different per region?

**Q:** Do you charge more for the SAM (Brazil) region, or for cross-region replication?

**A:** Two parts. (1) **Single-region pricing is flat across the four GA regions** — WNAM, ENAM, WEUR, SAM — because the underlying R2 substrate's zero-egress economics travel with the region pin (see `BLOG-POSTS/05-fast-cache-hit-economics.md`). (2) **Cross-region replication** (Enterprise opt-in only) carries a per-replicated-blob storage line on the secondary region; it is metered, transparent, and shown on the invoice. We do not surcharge SAM despite its smaller infrastructure footprint; we do surcharge cross-region active-active because that's where the real cost lives.

**Sources:** `marketing/launch/BLOG-POSTS/04-multi-region-residency.md`; `apps/docs/docs/trust/data-handling.mdx#residency`.

---

## Security (12)

### S1 — What does "BYOK" actually mean at CoreLink? Are you holding our keys?

**Q:** Is your BYOK real, or is it "we'll let you bring an opaque token we still hold the keys to"?

**A:** Real. Concretely: your KMS holds the **Key Encryption Key (KEK)**. CoreLink generates per-blob **Data Encryption Keys (DEKs)**, wraps each DEK under your KEK, and stores the wrapped DEK alongside ciphertext. To read a blob we call your KMS to unwrap. We do **not** hold a copy of your KEK, encrypted or otherwise. There is **no break-glass path** that re-derives plaintext from CoreLink-side material alone. The DEK cache TTL is **hard-capped at 5 minutes** — a code path, not a config knob — which bounds your kill-switch window. When you disable your KEK, within 5 minutes every in-flight DEK expires and CoreLink simply cannot read your data. This is the `INV-BYOK-CRYPTO-SOVEREIGNTY` CRITICAL invariant.

**Sources:** `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` (full deep-dive); `ADR-S14-004 / S14-005 / S14-006`; `apps/docs/docs/trust/data-handling.mdx#encryption`.

### S2 — Show me an audit-chain inclusion proof I can verify offline.

**Q:** How do I, the auditor, verify that an event I received was actually in the chain at the time you say it was?

**A:** Four steps you (or your external auditor) run yourself, with no CoreLink-side trust required: (1) JCS-canonicalize the event payload per RFC 8785; (2) hash it with SHA-256 using the RFC 6962 leaf prefix; (3) request the inclusion proof for `chain_position` against any later published head; (4) re-derive the root from leaf + sibling hashes. The arithmetic is logarithmic; we publish a reference verifier in Rust + TypeScript that produces bitwise-identical canonical output. For a window of events the same flow gives you a **consistency proof** between two heads — structural append-only evidence, not a screenshot. This is `INV-AUDIT-APPEND-ONLY` + `INV-OBS-AUDIT-CHAIN-INTEGRITY`, both CRITICAL, both modeled in `audit_immutability.tla` and checked in CI.

**Sources:** `marketing/launch/BLOG-POSTS/03-audit-chain-merkle-proofs.md` (full walk-through); `/security/audit-chain` (docs).

### S3 — What are the tenant-isolation guarantees? Specifically, what's the cross-tenant blast radius?

**Q:** If tenant A is compromised, what's the worst case for tenant B?

**A:** Cross-tenant blast radius is engineered to **zero**. Tenant A cannot read, write, or enumerate tenant B's blobs. This is the `INV-TenantIsolation` invariant — TLA+ model-checked (`tenant_isolation.tla`) and gated in CI. Every CAS read, every AC write, every audit append carries a verified tenant binding; cross-tenant access is structurally impossible. AAD binding (`tenant_id || blob_hash || cache_id`) means even if an attacker could substitute ciphertext, the AEAD primitive (AES-GCM-256 / ChaCha20-Poly1305 per provider) would reject the decryption. We have run an external pentest with a tenant-isolation adversarial scenario; clean, post-remediation retest also clean.

**Sources:** `apps/docs/docs/trust/index.mdx#posture-at-a-glance`; `marketing/launch/BLOG-POSTS/01-introducing-corelink.md`; `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` (AAD binding); `specs/03_architecture/security_model.md`.

### S4 — What's your SOC 2 status?

**Q:** Do you have a SOC 2 report?

**A:** Not yet — and we're explicit about it. **Internal readiness:** 83.7% weighted as of 2026-05-15 (per the auditor-grade Drata rollup, not a marketing percentage). **Type I fieldwork:** target Q4 2026, report Q1 2027. **Type II fieldwork:** target Q3 2027, report Q4 2027. **Audit partner:** Schellman & Co. (engagement letter executed). **Continuous-evidence platform:** Drata. We crosswalk to **ISO 27001:2022 at 98.9% in-scope** today (Stage 1 stacked with SOC 2 Q4-2026, certificate Q1-2027). Today's evidence is available under NDA from `trust@humangr.com` (1 business day SLA on the routing).

**Sources:** `apps/docs/docs/trust/compliance.mdx#soc-2`; `apps/docs/docs/trust/iso27001.mdx`; `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`.

### S5 — Have you done an external pentest?

**Q:** When was the last pentest? Can we see the report?

**A:** Yes — Pentest-1 firm engaged pre-GA, full report on file, post-remediation retest clean (this is one of the seven engineering-gate prerequisites, per spec contract S-20 §6.1 — see `BLOG-POSTS/01` "What 'GA' means"). The executive summary is shareable under NDA via `trust@humangr.com`. Annual cadence post-GA; next engagement is Q4 2026 stacked with SOC 2 Type I fieldwork. We do not publish the unredacted report (industry-standard practice and a condition of the testing firm's engagement); we do share the executive summary, the methodology, and the remediation status for any finding.

**Sources:** `marketing/launch/BLOG-POSTS/01-introducing-corelink.md` (engineering gate); `apps/docs/docs/trust/index.mdx#whats-verifiable-vs-whats-attested`; spec contract S-20 §6.1.

### S6 — What encryption do you use, and where?

**Q:** At rest, in flight, in use — be specific.

**A:** **At rest:** AES-256-GCM. R2 blobs are Cloudflare-managed SSE by default; with BYOK enabled, additional customer-side envelope encryption per blob (per-tenant DEK wrapped by your KMS root). D1, KV, DO are Cloudflare-managed encryption at rest. Backups same envelope as source. **In flight:** TLS 1.3 mandatory; HSTS (`max-age=63072000; includeSubDomains; preload`); HTTP/2 + HTTP/3 available; cipher suites limited to AEAD (AES-256-GCM, ChaCha20-Poly1305); mTLS edge-to-origin per CTRL-NET-002. **In use:** Worker isolates provide per-request memory isolation (V8 isolate model); customer bytes are not held in long-lived memory across requests; with BYOK, plaintext DEKs never leave the request scope and are discarded before the isolate recycles.

**Sources:** `apps/docs/docs/trust/data-handling.mdx#encryption`; `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md`.

### S7 — How does key rotation work?

**Q:** When and how are KEKs / DEKs rotated?

**A:** **KEK rotation is a customer operation.** CoreLink does not initiate KEK rotation; we observe it. When you roll your CMK, we re-wrap existing DEKs against the new KEK in an online background job, with progress visible in your dashboard and recorded in the audit chain. You can run with multiple active CMK versions; we choose the right one per object based on wrapped-DEK metadata. **DEKs are per-blob** and never re-used across blobs, tenants, or regions — there is no DEK rotation cadence because every blob already has a fresh DEK. **PAT rotation:** revocation propagates globally in **< 60s** (P95 measured by the FM-061 rotation drill).

**Sources:** `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md#rotation-audit-operational-details`; `apps/docs/docs/tutorials/quickstart-faq.mdx#10`.

### S8 — What's your breach-notification commitment?

**Q:** If you confirm a personal-data breach, when do we hear about it?

**A:** **72 hours** to authorities (LGPD Art. 33 / GDPR Art. 33), **24 hours** to the affected enterprise tenant (per DPA §7), and **without undue delay** to affected high-risk data subjects (GDPR Art. 34). The 72-hour clock starts at **awareness** — the moment we have reasonable certainty a breach occurred. We have a documented internal escalation that puts the decision in front of the DPO and Security Lead within 4 hours of suspicion — well inside the regulatory window.

**Sources:** `apps/docs/docs/trust/incident-response.mdx#breach-notification`; `specs/_runbooks/RB-BREACH-NOTIF.md`.

### S9 — Where do my secrets / PATs live?

**Q:** How are personal access tokens stored, and how do I avoid leaking them?

**A:** Stored hashed in our Clerk-backed identity store (server side never sees plaintext after issuance). The CLI **does not accept** `--pat` as a flag (security control `CTRL-CRED-001` — flags leak into shell history and `ps aux`); reads from `CORELINK_PAT` env var or `~/.corelink/config.toml`. Token format `corelink_<env>_t_xxx.xxx.xxx` makes the environment explicit (`sandbox / dev / staging / prod`); mixing environments is rejected at the edge with `COR_AUTH_TENANT_MISMATCH`. Revocation propagates in < 60s globally.

**Sources:** `apps/docs/docs/tutorials/quickstart-faq.mdx#9-11`.

### S10 — Does the CLI verify bytes after download?

**Q:** Could I receive a corrupted or tampered blob?

**A:** No — client-side BLAKE3 re-hash is **default-on** (control `CTRL-CAS-002`). A mismatch raises `COR_CAS_DIGEST_MISMATCH` and the process exits non-zero **before the bytes touch your filesystem**. The single Rust truth `corelink-client-verify` is wrapped by all four official SDKs (Rust / Python / Go / JS); there is no per-language hash drift. You can opt out via SDK construction (`client_verify=False`) but we don't recommend it; the CLI does not offer an opt-out flag.

**Sources:** `apps/docs/docs/tutorials/quickstart-faq.mdx#12, #6`.

### S11 — How is the kill switch tested? What proves it actually works?

**Q:** "We support BYOK kill switch" is easy to claim. How do you exercise it?

**A:** **Weekly synthetic chaos drill** scheduled at a time you prefer, on your tenant, against your KMS. Drill measures kill-switch round-trip time (target ≤ 5 min) and is recorded in your audit chain. For lighthouse customers, the drill is part of the 30-day SLA observation window with the `byok-kill-switch-rtt` SLO sampled. Internally we additionally run cross-tenant adversarial scenarios in the staging environment; the kill switch under load is the part we're proudest of — one tenant being killed does not slow down other tenants' data paths.

**Sources:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#what-were-measuring-daily-automated`; `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md#the-kill-switch`.

### S12 — Has anyone ever cross-tenant-leaked? Has there been any customer-impacting incident?

**Q:** What's the incident history?

**A:** Today (2026-05-15): **No customer-impacting SEV1 incidents since the platform's first paid traffic.** This is published on the Trust Center incident-response page and will be populated as needed (with post-mortem links) when an incident occurs. We are aware "no incidents yet" is partly a function of how young the production fleet is; we don't claim it as evidence of perfection. Our 24/7 on-call across three regions and a weekly synthetic page (sustained for 30 days pre-GA) verify the rotation actually pages — `marketing/launch/STATUS-PAGE-SPEC.md`.

**Sources:** `apps/docs/docs/trust/incident-response.mdx#past-incidents`.

---

## Compliance (8)

### C1 — SOC 2 status?

**Q:** Type I when? Type II when?

**A:** Type I fieldwork Q4 2026, report Q1 2027. Type II fieldwork Q3 2027 (after the minimum 6-month observation), report Q4 2027. Auditor: Schellman & Co. Continuous-evidence platform: Drata. Readiness today: 83.7% weighted (113 of 135 weighted criterion-points green; 2 reds, both BYOK-FIPS attestation gaps with D+30 closure cap and a fallback ADR if attestation letters slip). All major gaps close before Type I fieldwork; all minor gaps close before the Type II observation window cuts. See S4.

**Sources:** `apps/docs/docs/trust/compliance.mdx#soc-2`.

### C2 — LGPD (Brazil)?

**Q:** Are you LGPD compliant?

**A:** Yes — compliant as a processor (and joint controller for limited service-telemetry purposes). DPO in place (`dpo@humangr.com`). Brazilian-tenant data is processed **in-region** (`sam` — São Paulo) with full Art. 33 §1º residency attestation; the attestation document (`specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`) is verified nightly by `scripts/verify-lgpd-residency.py`. Residency enforcement crate: `crates/corelink-privacy-residency-enforcement/`. DSR turnaround: 5-business-day acknowledgement, 15-business-day resolution (Art. 18). Cross-border transfers: SCCs in the DPA with supplementary measures per EDPB recommendation. Breach notification: 72h to ANPD.

**Sources:** `apps/docs/docs/trust/compliance.mdx#lgpd`; `apps/docs/docs/trust/data-handling.mdx#residency`; `/residency/lgpd-brazil`.

### C3 — GDPR?

**Q:** Are you GDPR compliant?

**A:** Yes — compliant as a processor (joint controller for limited service-telemetry purposes). DPA template at `legal/dpa/v1.0.0` — three locales reviewed by external counsel (English EU+UK, Portuguese Brazil, Spanish LATAM). Schrems II: SCC Modules 2/3 plus supplementary measures (BYOK envelope encryption, EU-region pin). Breach notification 72h to supervisory authority (Art. 33) and without-undue-delay to high-risk affected data subjects (Art. 34). DSR rights (Arts. 15–22) supported with verifiable erasure (`INV-DATA-ERASURE-COMPLETE`, `INV-ERASURE-ATTESTATION-SIGNED`). Sub-processor change notice 30 calendar days advance.

**Sources:** `apps/docs/docs/trust/compliance.mdx#gdpr`; `marketing/launch/BLOG-POSTS/04-multi-region-residency.md`.

### C4 — ISO 27001?

**Q:** Are you ISO 27001 certified?

**A:** Not yet — **certification target Q1-2027** with Schellman (Stage 1 audit Q4-2026 stacked with SOC 2 Type I fieldwork; Stage 2 + certificate issuance Q1-2027). Today's crosswalk: **98.9% in-scope Annex A coverage** (89 of 90 applicable controls Implemented or Partial). 91% overlap with SOC 2 Type I evidence collection in Drata. Single Gap row (A.5.10 Acceptable Use Policy formalization) closes in T+1m. First-year cert cost envelope $55–90k (Stage 1 + Stage 2 combined).

**Sources:** `apps/docs/docs/trust/iso27001.mdx`.

### C5 — HIPAA?

**Q:** Can we use CoreLink for PHI?

**A:** **Out of scope by design.** CoreLink is a build-artefact cache; it does not handle PHI. We will **not** sign Business Associate Agreements (BAAs) for production PHI workflows. The underlying infrastructure (Cloudflare / AWS / GCP / Azure) is HIPAA-aligned at the substrate level, but CoreLink's product surface is not engineered, scoped, or tested for PHI, and we do not commit to the Privacy / Security / Breach Notification rules. If your build artefacts contain PHI, that's likely an upstream tagging bug — raise a SEV-2 with your account team.

**Sources:** `apps/docs/docs/trust/compliance.mdx#hipaa`.

### C6 — PCI DSS?

**Q:** Can we use CoreLink in a PCI cardholder-data environment?

**A:** **SAQ-A compliant (self-attested 2026-05-15)** — meaning CoreLink itself is *not* in your PCI CDE, and we never see card data. All cardholder data is tokenized at the edge via Stripe Elements (PCI DSS Level 1 Service Provider); CoreLink only stores opaque Stripe IDs (`cus_…`, `sub_…`, `pm_…`, `in_…`). We do not store, transmit, or process card data — zero, never. Stripe AOC is inherited via the Drata vendor module. Next recertification: 2027-05-15 (annual cadence).

**Sources:** `apps/docs/docs/trust/pci-dss.mdx`; `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md`.

### C7 — FedRAMP?

**Q:** Do you have FedRAMP authorization?

**A:** **No, and not in the near-term roadmap.** FedRAMP authorization (Moderate or High) requires a 12–18 month engagement with a 3PAO, an authorizing-official sponsor in a federal agency, and operational evidence at a maturity level we have not yet committed to. We crosswalk to **NIST 800-53 Rev 5 Moderate at 87%** today (informational — `specs/_compliance/NIST-800-53-CROSSWALK.md`), and GAP-24 closes T+6m. If you have a federal-sponsorship requirement, talk to us at Founder level; we will be honest about the gap and whether we can credibly close it on your timeline. (Most federal-adjacent customers we've spoken to need their *vendor's* SOC 2 + ISO 27001, not FedRAMP — see C1 / C4.)

**Sources:** `apps/docs/docs/trust/compliance.mdx#quick-scope-map`; `specs/_compliance/NIST-800-53-CROSSWALK.md`.

### C8 — Data residency: where does my data physically live?

**Q:** Pick one — where does my tenant's data live?

**A:** Whichever of the four GA regions you select at provisioning: `wnam` (Western NA), `enam` (Eastern NA), `weur` (Western EU — Frankfurt / Dublin), `sam` (South America — São Paulo). Three additional regions are available on request: `oce` (Sydney), `apc` (Tokyo / Singapore), `mea` (Dubai). Region binding is structural — `INV-REGION-NO-CROSS-LEAK` is a CRITICAL invariant. Every R2 blob, every D1 database, every DO instance is pinned to your region's colos. A request that reaches a region different from the tenant binding is refused at the boundary — not load-balanced, not falling back. You can verify your own tenant's residency via `GET /v1/tenant/me/residency-proof` — signed attestation with Merkle inclusion proof against the audit chain.

**Sources:** `apps/docs/docs/trust/data-handling.mdx#residency`; `marketing/launch/BLOG-POSTS/04-multi-region-residency.md`.

> **Pre-sales legal questionnaire toolkit:** for SIG Lite / CAIQ v4 / custom vendor-form responses, see `marketing/sales/legal-questionnaires/` — pre-filled SIG Lite (114 question-rows mapped to canonical evidence), CAIQ v4 (197 questions across 17 CCM v4 domains), generic vendor response template, evidence pack index, and response SLA policy (5d SIG Lite / 10d SIG Full / 7d CAIQ).

---

## Performance (6)

### PF1 — What's the p99 cache-hit latency?

**Q:** When my CI runner asks for a blob and you have it, how long does it take?

**A:** **p99 CAS GET latency target ≤ 300 ms** in-region (SLO `SLO-LAT-CAS-GET`, customer-tenant scope). For lighthouse customers we measure daily; representative steady-state runs are well below target (typical observed p99: **180–220 ms** depending on region and blob-size mix; the calculator at `corelink.humangr.com/calculator` exposes your projected value). Audit-append p99 target ≤ 500 ms. The fast path does *not* call your KMS — DEK is unwrapped at first read into the bounded 5-min in-memory cache and re-used until expiry. Cache-miss reads pay one KMS unwrap RTT (provider-specific; AWS / GCP / Azure / Vault all sub-100 ms p99 in practice).

**Sources:** SLO catalog (`docs.corelink.humangr.com/slo`); `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#what-were-measuring-daily-automated`; `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md`.

### PF2 — What hit rates are realistic?

**Q:** What cache hit ratio should we expect?

**A:** Hit rate is partly *your* property (build-graph stability) and partly *vendor* policy (cache key stability, negative caching policy, eviction policy, dedup). CoreLink optimizes the vendor-side terms hard: BLAKE3-keyed byte-stable content addressing, REAPI-canonical action-cache keys, bounded explicit negative caching, content-aware tunable eviction, structural dedup via content addressing. We do **not** publish a headline hit-rate number because it depends on your build. We publish the *curve shape* (see `REMOTE-CACHE-PRODUCT-PROFILE`) and expose your actual measured hit rate in the Grafana dashboard from D+1. Cold-start ramp: expect the first week to ramp from low single-digits to your steady state; subsequent weeks plateau.

**Sources:** `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md#cache-hit-ratio-a-model`; `REMOTE-CACHE-PRODUCT-PROFILE`.

### PF3 — What's the throughput ceiling?

**Q:** How many puts/gets per second?

**A:** Per-tier TPS ceilings are in the comparison table (P1). The **Enterprise tier is negotiated** — we've stress-tested in staging to 5× the Lighthouse-tier 5000 TPS with no observed degradation, and the Cloudflare R2 + Workers substrate scales horizontally per region. If you need a specific committed TPS, name it in the order form; we'll either commit or come back with a fact-based pushback. We do not artificially throttle below the published cap; we *do* rate-limit gracefully above it (soft-cap then 429 / overage-pending — see P2).

**Sources:** `apps/docs/docs/tutorials/quickstart-faq.mdx#8`; SLO catalog.

### PF4 — Cache-miss latency? Latency in the worst case?

**Q:** When the cache *doesn't* have it, what's the cost?

**A:** Cache-miss = first PUT against an unseen digest. The cost is **the upload itself** (size-bound, on your CI runner's egress) plus a small CoreLink-side append (typically < 50 ms over the upload). There is no "miss penalty" beyond the actual transfer; we don't synthesize artificial backoff on misses. For BYOK with cold DEK cache the read miss adds one KMS unwrap RTT (provider-specific; usually < 100 ms p99).

**Sources:** `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md`; `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md`.

### PF5 — Multi-region performance: does the SAM region underperform?

**Q:** Is Brazil slower than the US regions?

**A:** No structural reason for it to be — `sam` runs on Cloudflare's São Paulo colos with the same R2 + D1 + Workers substrate. Observed p99 in São Paulo during the 30-day staging window: comparable to ENAM (within < 15% variance). The honest caveat is that CI runners *outside* SAM hitting a SAM-pinned tenant pay round-trip latency — keep your runners and tenant in the same region. We do not perform implicit cross-region failover (would violate the residency contract); customers willing to span ENAM + WNAM for higher availability can opt into multi-region active-active explicitly.

**Sources:** `marketing/launch/BLOG-POSTS/04-multi-region-residency.md#failover-within-a-region-set`; `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md#honest-caveats`.

### PF6 — Cold start: what's the ramp?

**Q:** First week — what do we measure?

**A:** Cold-start curve: first build of a fresh repo populates the cache (every blob is a miss, paying upload cost). Hit rate climbs over D+1 to D+7 as the working set warms. **Steady state is typically observable by D+7** for Bazel monorepos with typical build profiles; very large polyglot graphs may take longer to warm. The calculator at `corelink.humangr.com/calculator` projects steady-state economics from your inputs; the dashboard shows your *actual* warming curve from D+1. This is true of every remote cache — we name it explicitly because some vendors don't.

**Sources:** `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md#honest-caveats`; `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#phase-1-days-1-7-mirror-your-ci`.

---

## Operations (8)

### O1 — Where's the status page?

**Q:** Where do I subscribe to status?

**A:** [status.corelink.humangr.com](https://status.corelink.humangr.com) — operated by Atlassian Statuspage, **isolated from the CoreLink production fabric** (if CoreLink is down, status page is up). Eight components tracked: API ingress, CAS read path, CAS write path, action cache, audit chain, admin plane, identity (Clerk), BYOK envelope. Per-region status shown for each component. Subscribe via email, SMS (US / EU), RSS (`https://status.corelink.humangr.com/history.rss`), webhook (Slack / Teams / PagerDuty), or JSON API (`/api/v2/summary.json`). Enterprise can pre-register a dedicated incident-comms distribution address inside tenant settings.

**Sources:** `apps/docs/docs/trust/incident-response.mdx#status-page`; `marketing/launch/STATUS-PAGE-SPEC.md`.

### O2 — What's on-call coverage?

**Q:** Who's awake at 3am Brazil time when our build cache goes down?

**A:** **24/7 paging across three regions** per PagerDuty rotation. Weekly synthetic page (sent every Monday 14:00 UTC, sustained for 30 days pre-GA) verifies the rotation actually pages — not just that it's configured. PagerDuty page acknowledgement target ≤ 10 min, 24/7. Phone callback on a P0 within 1 hour. For Enterprise tenants, the dedicated Slack Connect channel SEV-1 acknowledgement is ≤ 1 hour business hours / ≤ 4 hours outside.

**Sources:** `apps/docs/docs/trust/incident-response.mdx#reporting-an-incident-to-us`; `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#support-slas`.

### O3 — What are the support SLAs?

**Q:** How fast do you respond?

**A:** Tier-dependent. **Team:** community Slack (`#help`) median < 1 hour weekday response; GitHub Issues / Discussions for design + bugs; email `support@humangr.com` for billing / account. **Enterprise:** dedicated Slack Connect channel; SEV-1 response SLA **4 hours**; weekly account review available. **Lighthouse:** the full playbook applies (`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#support-slas`) — Slack acknowledge ≤ 1h business / ≤ 4h outside, PagerDuty ≤ 10 min 24/7, P1 engineering response ≤ 24h, attestation draft delivery ≤ 24h after D+40.

**Sources:** `apps/docs/docs/tutorials/quickstart-faq.mdx#15`; `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#support-slas`.

### O4 — Incident response process?

**Q:** What happens during a SEV1?

**A:** Deliberate cadence. **≤ 5 min:** status page flips affected components; initial "Investigating" post. **≤ 30 min:** email to affected tenants (security@ + registered tech contact + incident-comms address); initial scope statement. **Every 30 min:** status update with the *change since last update* — even if "no change". **Until resolved:** continuous updates; on-call commander identified by name. **≤ 72h:** public post-mortem published on status page and linked from Trust Center. **T+14d:** internal post-mortem review with corrective-action register; summary delta added to public post-mortem if material. Templates in `marketing/launch/CRISIS-COMMS-TEMPLATES.md`.

**Sources:** `apps/docs/docs/trust/incident-response.mdx#communicating-during-a-sev1`.

### O5 — Status communication during partial impairment?

**Q:** What about a SEV2 — single region, degraded performance?

**A:** Status page within **15 minutes** if customer-facing; otherwise internal only. Per-component, per-region status shown. SEV2 does not trigger the SEV1 email cadence but does appear on the status page with running updates. Enterprise pre-registered incident-comms addresses still receive SEV2 notifications. Severity definitions on `apps/docs/docs/trust/incident-response.mdx#severity-definitions`.

**Sources:** `apps/docs/docs/trust/incident-response.mdx`.

### O6 — Planned maintenance: how much notice?

**Q:** Do you do maintenance windows? When and how do we hear about them?

**A:** **30 calendar days advance notice** for sub-processor changes (per DPA §6 / GDPR Art. 28 / LGPD Art. 27 §4º) — published as a *Maintenance / Informational* item on the status page; email digest via the `subprocessor-changes@` distribution address inside tenant settings; RSS feed. **For routine deploys** that touch a customer's hot path: 24h notice in the Slack Connect channel with rollback plan. Customers can request a freeze on their tenant during a critical period (e.g., your own product launch window) — tell us. Material spec or DPA changes follow `specs/_runbooks/RB-DPA-CHANGE.md`.

**Sources:** `apps/docs/docs/trust/subprocessors.mdx#notice-of-changes-30-day-grace`; `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#faq` Q12.

### O7 — Backups, RPO, RTO?

**Q:** Recovery point objective? Recovery time objective?

**A:** **Backup snapshots:** 35-day rolling retention, encrypted at rest with the same envelope as source. **Failover semantics:** within a residency-compatible set only — AZ failure inside a region fails over to another AZ inside the same residency boundary; whole-region failure falls back to read-only mode from the secondary footprint inside the same region. Customers can opt into multi-region active-active explicitly. **RPO / RTO** targets are tier-dependent and published in your scoping doc for lighthouse / Enterprise; ask Sales for the current targets if your procurement requires them in writing.

**Sources:** `apps/docs/docs/trust/data-handling.mdx#retention`; `marketing/launch/BLOG-POSTS/04-multi-region-residency.md#failover-within-a-region-set`; `specs/_runbooks/RB-DR-DRILL.md`.

### O8 — Audit chain retention?

**Q:** How long do you keep audit events?

**A:** **7 years.** Customer-controlled? No — compliance-driven minimum. Audit chain is append-only, Merkle-linked, with RFC 6962 inclusion proofs and JCS-canonicalized leaves. Backup snapshots of the chain follow the same 35-day rolling window with tombstones recorded on day 0 so a Type II auditor can trace the chain. Right-to-erasure under GDPR Art. 17 / LGPD Art. 18: tenant-initiated via `DELETE /v1/tenant/me` triggers verifiable erasure with cryptographic attestation; the audit record itself remains for integrity, but PII-bearing claims are made cryptographically unrecoverable via the salt-rotation pattern (`ADR-S11-003`).

**Sources:** `apps/docs/docs/trust/data-handling.mdx#retention`; `marketing/launch/BLOG-POSTS/03-audit-chain-merkle-proofs.md`.

---

## Migration (8)

### M1 — Migrating from bazel-remote: what's the path?

**Q:** We're running `bazel-remote` standalone today. How do we switch?

**A:** Easiest path is the **`corelink-bazel-remote-shim`** — a drop-in front that forwards to CoreLink with the existing bazel-remote API surface. Your `.bazelrc` doesn't change beyond the cache URL. Alternative: use Bazel's native `--remote_cache` against `https://cache.corelink.humangr.com/v1/{slot-id}` directly with a `--remote_header=Authorization=Bearer ${CORELINK_PAT}`. We recommend the **mirror-then-cutover** pattern (see M5) — write to both for 1–2 weeks, validate hit ratio, then flip primary. The full self-serve guide is at `apps/docs/docs/how-to/migrate/from-bazel-remote/`.

**Sources:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#phase-1-days-1-7-mirror-your-ci`; `apps/docs/docs/how-to/migrate/`.

### M2 — Migrating from S3-based self-hosted cache?

**Q:** We rolled our own on S3 + a custom HTTP shim. How does that move?

**A:** Three components to migrate: (1) **The cache surface** — point your build tool at CoreLink's HTTP/REAPI endpoint; we support any client that does PUT/GET against a content-addressable backend. (2) **Existing blobs** — optional; you can either let CoreLink warm naturally (recommended; cold-start is < 1 week to steady state — see PF6) or pre-warm via the bulk-upload CLI (`corelink import s3://your-bucket/...` — content-addressed, so dedup happens for free). (3) **Egress economics** — the migration *itself* costs you one final S3 egress charge if you pre-warm; ongoing reads then run on R2 with zero egress (see PF1 / `BLOG-POSTS/05`). Migration guide: `apps/docs/docs/how-to/migrate/from-s3/`.

**Sources:** `apps/docs/docs/how-to/migrate/from-s3/`; `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md`.

### M3 — Migrating from Docker registry / Harbor?

**Q:** We're using a Docker registry for layer caching today. Does CoreLink replace it?

**A:** **Partial overlap.** CoreLink stores any content-addressable blob — Docker layer caching (gzipped tar layers keyed by digest) is a valid CAS workload, and customers do use CoreLink as a layer-cache backend. **CoreLink is not a full Docker registry** (no manifest API surface, no OCI registry HTTP spec conformance at GA — that's roadmap). For *layer caching only*, the migration is the same shape as M2: point your build tool at CoreLink's CAS endpoint, optionally pre-warm. For *registry + layer caching combined*, keep Harbor / your registry for manifests and offload the heavy layer-cache traffic to CoreLink. We're explicit about this scope; Phase 2 — Remote Execution — and an OCI front are both post-GA roadmap items.

**Sources:** `marketing/launch/BLOG-POSTS/01-introducing-corelink.md#what-is-next`; `REMOTE-CACHE-PRODUCT-PROFILE`.

### M4 — Data egress: what does it cost to leave?

**Q:** If we leave you for another vendor, what do we pay to extract our data?

**A:** **Zero, structurally.** Two ways out: (1) Your data is content-addressed by BLAKE3 / SHA-256 — every blob can be downloaded by digest with `corelink cas get blake3:{digest}` against the standard tier-included egress allowance. (2) For a full bulk extraction, `corelink cas export --tenant me --output s3://your-bucket/` ships every blob, every audit event, every action-cache record to a destination you control, billed at the same R2-zero-egress economics that govern read traffic — no surcharge for departure. Audit-chain export is always free. We mean it: the content-addressed nature of CAS is *itself* the escape hatch (see P34 in objection handling).

**Sources:** `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md`; `marketing/sales/OBJECTION-HANDLING.md` Obj-22.

### M5 — Mirror / parallel-run period: how long, and what does it look like?

**Q:** How do we run CoreLink alongside our existing cache for a while?

**A:** Recommended pattern: **mirror, not cut-over.** Your existing cache (bazel-remote, BuildBuddy, your S3 thing) stays primary. CoreLink runs as secondary destination: writes go to both, reads come from existing cache. If CoreLink misbehaves, your builds don't break. Two practical patterns: (i) **Sidecar mirror** — `corelink ci mirror --from bazel-remote --to corelink` runs as a sidecar process on CI runners. (ii) **BES consumer** — point Bazel's `--bes_backend` at our endpoint; we ingest cache references asynchronously without touching the build's critical path. **Duration:** 1–2 weeks for typical customers; lighthouse customers do 14 days of mirror (Phase 1) before optional cutover.

**Sources:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#phase-1-days-1-7-mirror-your-ci`; `apps/docs/docs/how-to/migrate/`.

### M6 — Rollback: if we cut over and it doesn't work, can we revert?

**Q:** What's the rollback story?

**A:** Two layers. (1) **Build-tool rollback** — flip `--remote_cache` back to your previous cache URL. Trivial; minutes. We recommend keeping your previous cache running for at least 30 days after cutover for exactly this reason. (2) **Data rollback** — your data on CoreLink doesn't get destroyed when you flip; it remains under the retention policy you've configured (default 90-day LRU on Team). If you flip back and then forward later, the cache repopulates from your live build traffic. There is no "decision penalty" for trying us, finding it doesn't fit, and rolling back; that's the point of the mirror-then-cutover pattern (M5).

**Sources:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#phase-1-days-1-7-mirror-your-ci`; rollback runbook (`specs/_runbooks/RB-CUSTOMER-ROLLBACK.md`).

### M7 — Cutover support: do we get help during the switch?

**Q:** During the actual cutover, what support is available?

**A:** Tier-dependent. **Team:** community Slack, GitHub Issues, written migration guides; we'll review your `.bazelrc` / `buckconfig.local` on request via `support@humangr.com`. **Enterprise:** named Customer Success engineer for the cutover window, scheduled cutover-day Slack Connect call, post-cutover review at 24h / 7d / 30d. **Lighthouse:** the full Customer Playbook applies — dedicated engineer, weekly check-in, daily SLA samples, attestation at D+30 (`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`). We do **not** charge for cutover support on Enterprise — it's part of the contract.

**Sources:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`; `apps/docs/docs/tutorials/quickstart-faq.mdx#15`.

### M8 — Migration timeline: how long, realistically?

**Q:** End-to-end, when can we be off the old cache?

**A:** Typical timelines we've observed (with the obvious caveat that your build is your build):

- **Single-pipeline pilot:** D+0 sign → D+1 first cache write → D+7 measurable hit ratio → D+14 cutover-ready.
- **Full monorepo cutover:** D+0 → D+30 (the lighthouse playbook timeline; runs in parallel with attestation).
- **Multi-team enterprise migration:** D+0 → D+60–90 depending on team count and tooling heterogeneity (Bazel + Buck2 + Pants + Gradle in the same shop is the typical worst case).

The mirror-then-cutover pattern means you're not blocked on full cutover to get value — you start measuring hit ratio and latency from D+1.

**Sources:** `marketing/lighthouse-kit/03-integration-timeline.md`; `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`.

---

## Cross-references

- **Lighthouse playbook:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` — for the prospect who's becoming a customer.
- **Launch checklist V2:** `marketing/launch/LAUNCH-CHECKLIST-V2.md` — for the prospect asking about GA timing.
- **Trust Center:** `apps/docs/docs/trust/` — full compliance posture, sub-processors, incident response.
- **Objection handling:** `marketing/sales/OBJECTION-HANDLING.md` — top 30 objections with structured responses.
- **Competitive matrix:** `marketing/sales/COMPETITIVE-MATRIX.md` — honest comparison vs. bazel-remote / BuildBuddy / S3 / Harbor.
- **Proof points:** `marketing/sales/PROOF-POINTS.md` — every numeric claim with a doc / commit pointer.

---

**Fim SALES-FAQ-MASTER.**
