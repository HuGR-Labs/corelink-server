---
id: "SALES-OBJECTION-HANDLING"
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
tags: ["sales", "objection-handling", "r-prep", "ga", "playbook"]
---

# CoreLink Objection Handling — Top 30

> **Audience:** anyone running a pre-purchase or renewal conversation. **Not** a script — these are reference patterns.
> **Structure per objection:** Objection → Why it matters to the customer → Our position → Evidence → Counter-question.
> **Tone rule:** acknowledge the legitimacy of every objection before pushing back. Customers can tell when they're being deflected.
> **Companion docs:** `marketing/sales/FAQ-MASTER.md`, `marketing/sales/COMPETITIVE-MATRIX.md`, `marketing/sales/PROOF-POINTS.md`, `marketing/retention/RETENTION-PLAYBOOK.md` (renewal-save plays), `marketing/retention/CHURN-RISK-SIGNALS.md` (heuristic catalog backing the save motion), `marketing/retention/CANCELLATION-FLOW.md` (in-product cancel UX).

---

## How to use

- Find the objection that matches (or the closest analog) and start with the "Our position" line.
- "Counter-question" is the question to ask **back** — it shifts the burden of proof, surfaces the real concern, and gives you data for the next step.
- If an objection isn't here, file it against this doc and route to Founder review.

---

## Category A — Vendor risk (the "you're tiny" family)

### Obj-1 — "You're a 1-person company. What happens if you get hit by a bus?"

- **Why it matters:** real procurement concern. Vendor continuity is a contractual risk.
- **Our position:** acknowledge the asymmetry, point to the structural mitigations rather than counter-narrative. Three of them.
- **Evidence:**
  - **Advisor pool:** named technical + commercial advisors on file; available under NDA.
  - **OSS-foundation roadmap:** the verification toolkit for the audit chain (independent of the managed service) is planned for OSS release; the core REAPI surface conforms to a public spec — a successor maintainer is not starting from zero.
  - **Escape hatch is structural:** your data is content-addressed by BLAKE3 / SHA-256. `corelink cas export` to any S3-compatible destination is a single command; departure is bounded by your egress, not by our cooperation.
  - **Source-available trajectory:** while CoreLink ships as a managed service at GA, the source-available + on-prem path is on the post-GA roadmap (anti-scoped from GA explicitly — see `BLOG-POSTS/01` "What is next").
- **Counter-question:** "What's the procurement gate you need to clear here? Is it 'vendor must have ≥ N employees', or 'we must have an exit strategy that survives vendor failure'? Those map to different conversations."

### Obj-2 — "You haven't been around long enough."

- **Why it matters:** customers want a track record. Build-cache outages are quietly catastrophic.
- **Our position:** name the gap honestly. Counter with engineering-gate rigor, not bluster.
- **Evidence:** 21 sprints, external pentest with clean retest, 30 days sustained staging as a precondition of GA, three lighthouse customers attesting our SLA, zero cross-tenant leaks ever (adversarial E2E + audit chain), 24/7 on-call across three regions with weekly synthetic page exercises sustained for 30 days pre-GA.
- **Counter-question:** "Would a SOC 2 readiness rollup + audit-chain proofs + a lighthouse-customer reference call resolve this, or is the concern more about ARR-scale and runway? Those are different conversations and I want to make sure I'm answering the right one."

### Obj-3 — "What if you raise prices on us next year?"

- **Why it matters:** lock-in fear; CFO instinct.
- **Our position:** the contract is the answer. Multi-year contracts have price-lock built in.
- **Evidence:** multi-year contracts include explicit **price-lock for the term**; list-price increases during the term do not apply. Annual re-baseline option allows renegotiation **downward** (not upward) if your usage falls below committed bands. See FAQ P7.
- **Counter-question:** "Want to take that risk off the table contractually? We sign 2y and 3y with price-lock; the 3y is the largest discount and the tightest cap on future increases."

### Obj-4 — "We need 5 customer references to consider you."

- **Why it matters:** procurement gate. Some shops have hard rules.
- **Our position:** acknowledge we're at 3 lighthouse customers; offer alternative evidence forms.
- **Evidence:** 3 lighthouse customers will reference-call (with sign-off); Forge customer-zero attestation is public; SOC 2 readiness rollup, ISO 27001 crosswalk, pentest executive summary, vendor risk register — all NDA-deliverable. Reference calls scale to 2/quarter per lighthouse alumni for 12 months post-attestation.
- **Counter-question:** "If references are the only blocker — would 3 reference calls plus the SOC 2 readiness pack and a pentest summary get you across the line? If the answer is 'still need 5,' I'd rather know now."

### Obj-5 — "Your roadmap is too small for our needs."

- **Why it matters:** customers want to see commitment to their use case.
- **Our position:** Phase 2 — Remote Execution — opens next sprint cycle. SOC 2 Type I in 6 months. ISO 27001 cert Q1-2027. APAC region post-GA demand-driven. We anti-scoped aggressively for GA on purpose — we'd rather ship the cache cleanly before we ship the executor.
- **Evidence:** `marketing/launch/BLOG-POSTS/01-introducing-corelink.md#what-is-next`; spec contract S-20 §10 (anti-scope list); ROADMAP-TO-GA + roadmap-post-GA documents.
- **Counter-question:** "Which specific roadmap item would unlock the conversation? If you can give me one named feature, I can come back with a date or an honest 'not in scope' — and the latter is more useful than a hedge."

---

## Category B — "Just use the free / cheap thing" (commodity-substitute family)

### Obj-6 — "bazel-remote is free."

- **Why it matters:** legitimate substitute for many teams. We should not bash it.
- **Our position:** acknowledge true; explain the operational tax that doesn't show up on the AWS invoice; offer the TCO calculator.
- **Evidence:**
  - bazel-remote is excellent reference implementation; single-tenant by construction.
  - Self-hosting carries a real, persistent operational tax: eviction tuning, GC correctness, dashboards, 3am pages, blob sprawl across regions. Often a part-time job for an SRE, full-time during bad weeks.
  - On S3 with realistic CI traffic, **egress** is typically the single largest line item — `BLOG-POSTS/05-fast-cache-hit-economics.md`. CoreLink on R2 has zero egress.
  - No multi-tenant isolation, no audit chain with Merkle proofs, no BYOK, no residency invariants.
- **Counter-question:** "Would you run the calculator at `corelink.dev/calculator` against your actual CI numbers? If the answer comes back 'self-hosted is cheaper,' the answer is self-hosted. We're aware that's a possible outcome."

### Obj-7 — "Why not just S3 + CloudFront?"

- **Why it matters:** cloud-native engineers think in primitives first.
- **Our position:** name the four things you'd build yourself if you went that route, and the egress economics.
- **Evidence:**
  - S3 + CloudFront gives you object storage + edge cache. To use it as a build cache you'd build: (1) **multi-tenant isolation** — IAM is not by itself sufficient; you'd need cross-account isolation patterns + access logging + access-policy diffing in CI. (2) **Audit chain** — S3 access logs are not Merkle-proof material; you'd build a separate append-only structure with daily proofs. (3) **No operational burden? No — S3 lifecycle policies, CloudFront cache invalidation, content-addressed key schemes, GC for deleted content, dedup — all on you. (4) **BYOK with kill switch** — KMS yes, kill-switch propagation < 5 min with audit-chain attestation no.
  - On S3 you pay cloud egress on every cache hit. On R2 (CoreLink's substrate) you don't.
- **Counter-question:** "Are you actually evaluating us against a build-it-yourself S3 stack, or against a competitor that ships those four pieces? Those are different conversations."

### Obj-8 — "Cloudflare is just a CDN; can it really be a primary build cache?"

- **Why it matters:** mental model mismatch. CoreLink runs on the full Cloudflare developer platform, not just the CDN.
- **Our position:** clarify the substrate. Workers + R2 + D1 + DO + KV is a complete primitive set, not CDN-with-extras.
- **Evidence:** `apps/docs/docs/trust/data-handling.mdx#what-we-store` — R2 (blobs), D1 (action-cache metadata, per-tenant DB), KV (hot-path counters), DO (per-tenant coordination state). Worker isolates for per-request isolation. This is the same substrate Cloudflare's own internal infrastructure runs on at internet scale; it's not "Cloudflare's CDN doing a side job."
- **Counter-question:** "Want a Workers/R2 architecture briefing from our engineering lead? We can do it without sales attached if that's helpful."

### Obj-9 — "BuildBuddy already does this."

- **Why it matters:** real competitor; respect required.
- **Our position:** BuildBuddy is excellent and we owe them a debt for normalizing the REAPI surface. We differ on three structural axes.
- **Evidence:**
  - **Multi-region residency:** BuildBuddy's hosted plane runs in a single primary region per customer; CoreLink has 4 enumerated regions with `INV-REGION-NO-CROSS-LEAK` as a CRITICAL invariant.
  - **BYOK at the envelope-encryption layer:** customer-managed KMS, per-blob DEK, 5-min kill switch. Not part of BuildBuddy's current shipped scope.
  - **TLA+-modeled tenant isolation in CI:** the invariant is checked, not asserted.
  - Full competitive matrix at `marketing/sales/COMPETITIVE-MATRIX.md`. Note we also point out where BuildBuddy is better — Remote Build Execution is shipped today; ours is Phase 2 post-GA.
- **Counter-question:** "Is your use case dominated by RBE (compute) or by remote cache (cache)? If it's compute today, BuildBuddy might be the right answer today and we can revisit at Phase 2. If it's cache, the residency + BYOK axes are where we differ."

### Obj-10 — "NativeLink is open-source and fast."

- **Why it matters:** legitimate alternative for self-hosted-minded teams.
- **Our position:** acknowledge NativeLink's strong performance and license. We're a different shape.
- **Evidence:** NativeLink is self-hosted; the operator is responsible for tenancy, residency, and key management. CoreLink is managed multi-tenant with regulated-industry primitives. If you want to run cache infrastructure yourself, NativeLink is a serious option; if you want the residency invariants, audit chain with Merkle proofs, BYOK kill switch, and 24/7 on-call to come pre-wired, that's the difference.
- **Counter-question:** "Are you set up to run the operational tail? If you have an SRE rotation that wants to own a cache, NativeLink is a fine answer. If you don't, that operational tail becomes a line item we're priced against."

---

## Category C — Security and trust theatre

### Obj-11 — "Your BYOK is just BYOK-flavored; you still hold the keys."

- **Why it matters:** legitimately, most vendors' BYOK is weak. The objection is well-trained.
- **Our position:** the only way to refute this is to show the construction, not the marketing.
- **Evidence:**
  - We do **not** hold a copy of your KEK, encrypted or otherwise. There is no break-glass.
  - DEK cache TTL is **hard-capped at 5 minutes** — a code path, not a config. Inspect `crates/corelink-crypto-envelope/src/dek_cache.rs`.
  - AAD binding: every DEK wrap and every payload encryption binds `tenant_id || blob_hash || cache_id`. AEAD verification fails on tamper.
  - Kill switch: disable your KEK in your KMS → within 5 min every in-flight DEK expires → CoreLink simply cannot read your data. Exercised weekly in synthetic drill.
  - No operator-readable tier exists; no "support decrypt" path exists.
  - `INV-BYOK-CRYPTO-SOVEREIGNTY` is a CRITICAL invariant, tracked in the registry and gated in CI.
  - Full architecture: `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md`.
- **Counter-question:** "Want your security architect on a call with our crypto lead? We'll walk the construction at the code-path level — if it doesn't survive that scrutiny we don't deserve the contract."

### Obj-12 — "You don't have SOC 2."

- **Why it matters:** procurement gate at most regulated shops.
- **Our position:** acknowledge directly. We have a calendared path; offer Type I readiness pack now.
- **Evidence:** readiness 83.7% weighted; auditor Schellman; Type I fieldwork Q4 2026, report Q1 2027. ISO 27001 cert Q1-2027 (98.9% in-scope crosswalk today). Drata-continuous evidence available under NDA. PCI DSS SAQ-A compliant. LGPD + GDPR compliant as processor. Pentest with clean retest.
- **Counter-question:** "Is SOC 2 a hard gate, or is the gate 'evidence of a controlled environment'? If it's the latter, the SOC 2 readiness rollup + ISO 27001 crosswalk + pentest summary may resolve it under NDA. If it's the former and procurement won't bend, our Type I lands Q4 2026 — would a deferred-effective-date contract structure work?"

### Obj-13 — "Your audit chain is just logs in S3."

- **Why it matters:** customers have been burned by "compliant" log retention.
- **Our position:** the chain is RFC 6962 Merkle-linked with RFC 8785 JCS canonicalization. Verifiable offline by your auditor.
- **Evidence:** `marketing/launch/BLOG-POSTS/03-audit-chain-merkle-proofs.md`. Independently re-derivable chain heads. Consistency proofs across an arbitrary window. Reference verifier published in Rust + TypeScript producing bitwise-identical canonical output. `INV-AUDIT-APPEND-ONLY` + `audit_immutability.tla` model-checked in CI.
- **Counter-question:** "Want your auditor on a 30-minute call with us to walk the inclusion-proof flow? They can verify a synthetic event end-to-end live."

### Obj-14 — "Your tenant isolation is a marketing word."

- **Why it matters:** the word *is* over-claimed across the industry.
- **Our position:** for us, it's TLA+. The model checker either passes or fails CI.
- **Evidence:**
  - `tenant_isolation.tla` formal specification, model-checked in CI; CI fails if the safety property regresses.
  - AAD binding makes cross-tenant decryption cryptographically impossible — not policy-prevented.
  - External pentest tenant-isolation adversarial scenario: clean, post-remediation retest also clean.
  - **0 cross-tenant leaks ever** — adversarial E2E + audit chain.
  - `INV-TENANT-NO-CROSS-READ` CRITICAL invariant.
- **Counter-question:** "Would you want a read-only copy of the TLA+ spec + the CI run history? It's not theatre to share — auditors find it useful."

### Obj-15 — "What if you get a subpoena / court order for our data?"

- **Why it matters:** customers in regulated industries operate under regimes where "the vendor can be compelled" is not acceptable.
- **Our position:** with BYOK enabled, the compelled party cannot in fact comply unilaterally. We designed for this.
- **Evidence:** `BLOG-POSTS/02-byok-deep-dive.md#the-threat-model-we-are-willing-to-defend`. Threat model #2: a coerced CoreLink operator. The unwrap call is to your KMS, under your policy. We cannot decrypt. We have built the integration so that the compelled party (us) cannot in fact comply unilaterally. Full transparency report cadence post-GA per Trust Center.
- **Counter-question:** "Is the concern the US legal regime (FISA, CLOUD Act) specifically, or sovereign-immunity-class compulsion broadly? For the US case the EU-region pin + BYOK + Schrems II TIA is the answer; for broader, the BYOK kill switch is the structural answer."

### Obj-16 — "What if your engineers go rogue?"

- **Why it matters:** insider threat is real and CoreLink does have operators with production access.
- **Our position:** same answer as Obj-15 for the data path: with BYOK, a compromised operator cannot read plaintext at rest. For non-BYOK tenants: dual-approval gate (`PAT-DUAL-APPROVAL-001`) on destructive admin ops, signed-deploy pipeline, Rekor transparency-log entries for every release, audit-chain coverage of all state-changing operations.
- **Evidence:** `apps/docs/docs/trust/compliance.mdx#what-you-can-rely-on-today-pre-type-i`; `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` (operator threat model).
- **Counter-question:** "Want the operator-trust threat model walk-through? It's a 20-minute call covering: BYOK envelope, dual-approval gate, signed deploys, audit-chain reconstruction."

### Obj-17 — "We need an annual pentest report."

- **Why it matters:** procurement standard.
- **Our position:** Pentest-1 firm pre-GA, clean post-remediation retest, exec summary shareable under NDA. Annual cadence post-GA; next Q4 2026 stacked with SOC 2.
- **Evidence:** `marketing/launch/BLOG-POSTS/01-introducing-corelink.md` (engineering gate); `apps/docs/docs/trust/index.mdx#whats-verifiable-vs-whats-attested`.
- **Counter-question:** "Is your annual-pentest requirement met by exec summary + remediation status, or do you need full unredacted report access? The latter is rare; the former we can do under NDA within one business day."

---

## Category D — Compliance / data sovereignty

### Obj-18 — "Our data must stay in Brazil / EU / [region]."

- **Why it matters:** LGPD / GDPR / sector regulator gate.
- **Our position:** structural region pin, not configuration. `INV-REGION-NO-CROSS-LEAK` CRITICAL.
- **Evidence:** `apps/docs/docs/trust/data-handling.mdx#residency`; `BLOG-POSTS/04-multi-region-residency.md`. Four enumerated regions (`wnam`, `enam`, `weur`, `sam`) with 3 more on request (`oce`, `apc`, `mea`). Region binding is enforced at routing, storage, and audit layers. Nightly verification script (`scripts/verify-lgpd-residency.py`). Verifiable per tenant via `GET /v1/tenant/me/residency-proof`. Three-locale DPA reviewed by external counsel (EN / PT-BR / ES).
- **Counter-question:** "Which residency invariants need to be in writing in the DPA, and which can live in the SOC 2 / audit-chain layer? Some customers want both; some are satisfied with the cryptographic + structural enforcement."

### Obj-19 — "Schrems II — we can't transfer to a US vendor."

- **Why it matters:** post-Schrems II uncertainty + Schrems III pending.
- **Our position:** EU-region pin + SCC + supplementary measures + Transfer Impact Assessment.
- **Evidence:** `BLOG-POSTS/04-multi-region-residency.md#the-legal-landscape-briefly`. WEUR region pin; SCC Modules 2/3 in our DPA; supplementary measures (BYOK envelope, encrypted-at-rest, audit-chain); TIA on file shareable under NDA. We assume EU-origin personal data does not leave the EU unless the controller has a specific documented reason for the contrary.
- **Counter-question:** "Is your concern about the underlying Cloudflare entity (Schrems II-relevant), or about CoreLink as a US-incorporated processor? Both have answers, but they're different conversations and the second one is structurally harder."

### Obj-20 — "We need a BAA (HIPAA)."

- **Why it matters:** healthcare procurement gate.
- **Our position:** out of scope by design. We will not sign BAAs for PHI workflows.
- **Evidence:** `apps/docs/docs/trust/compliance.mdx#hipaa`. CoreLink is a build-artefact cache; PHI is upstream from our scope. The infrastructure providers (Cloudflare / AWS / GCP / Azure) are HIPAA-aligned at substrate level, but CoreLink's product surface is not engineered, scoped, or tested for PHI.
- **Counter-question:** "Are your build artefacts actually PHI, or is the BAA requirement a blanket procurement policy? If build artefacts are PHI, that's likely an upstream tagging bug worth raising on your side — we can talk through it. If it's blanket procurement, we won't get past it."

### Obj-21 — "We need FedRAMP."

- **Why it matters:** federal-adjacent customer.
- **Our position:** we don't have FedRAMP and we're honest about the gap. NIST 800-53 Rev 5 Moderate crosswalk 87% today.
- **Evidence:** `apps/docs/docs/trust/compliance.mdx#quick-scope-map`; `specs/_compliance/NIST-800-53-CROSSWALK.md`. Most federal-adjacent customers we've spoken to actually need their vendor's SOC 2 + ISO 27001 — not FedRAMP per se.
- **Counter-question:** "Is FedRAMP a hard requirement from your agency sponsor, or is it 'high-confidence security posture'? If the former we're not the vendor today; if the latter, the SOC 2 + ISO 27001 path may resolve it."

---

## Category E — Migration cost / lock-in fear

### Obj-22 — "We don't want lock-in."

- **Why it matters:** healthy CFO instinct.
- **Our position:** lock-in is structurally bounded for CoreLink. Content-addressed storage is the escape hatch.
- **Evidence:** Your data is BLAKE3 / SHA-256 keyed. `corelink cas export --tenant me --output s3://...` is one command. Audit-chain export is free. Migration credit (3 months 50% on Team) explicitly exists for *both directions* — we'd rather know early if you want to leave. The content-addressed nature of CAS *is* the escape hatch.
- **Counter-question:** "What does 'lock-in' actually look like for you in worst case? If it's '90 days to extract data,' we can show you the export tool right now. If it's 'rewrite our toolchain,' that's REAPI-level standardization, not CoreLink-specific."

### Obj-23 — "Migration is too disruptive."

- **Why it matters:** real cost; engineers value uninterrupted CI.
- **Our position:** mirror-then-cutover. Your existing cache stays primary; CoreLink runs as secondary; if we misbehave, your builds don't break.
- **Evidence:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md#phase-1-days-1-7-mirror-your-ci`. Two practical patterns: sidecar mirror (`corelink ci mirror`) or BES consumer (asynchronous ingest of cache references). Typical mirror period: 1–2 weeks. Cutover is `--remote_cache` URL flip.
- **Counter-question:** "Want to scope the smallest possible pilot? One pipeline, 14 days of mirror, no production cutover. That's a 30-min scoping call, not a deal."

### Obj-24 — "What if your migration tool breaks something?"

- **Why it matters:** trust in tooling.
- **Our position:** the tools are open / inspectable. The mirror pattern means you don't depend on them for cutover.
- **Evidence:** `corelink-bazel-remote-shim` is a drop-in front (you can read it before deploying it). Bulk import (`corelink cas import s3://...`) is idempotent and content-addressed, so re-running is safe (digests already-present are no-ops). Rollback is a build-tool config flip.
- **Counter-question:** "Want your build-tools engineer on a working session with our integrations engineer? We'll walk the shim source live."

### Obj-25 — "We've been burned by 'easy migration' promises."

- **Why it matters:** scar tissue. Real.
- **Our position:** acknowledge directly. We don't promise easy; we promise a structured path with rollback at every step.
- **Evidence:** Lighthouse playbook has explicit rollback path (`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`); 5-business-day withdrawal during observation window without penalty; mirror-then-cutover means cutover is reversible.
- **Counter-question:** "What burned you last time? Knowing the specific failure mode helps us avoid claiming we won't repeat it without earning it."

---

## Category F — Pricing / TCO objections

### Obj-26 — "You're too expensive."

- **Why it matters:** often a proxy for "I haven't seen the TCO math."
- **Our position:** run the calculator. If TCO genuinely doesn't pencil out, the answer is self-hosted.
- **Evidence:** `corelink.dev/calculator` runs against your actual inputs. The cache-tax model in `BLOG-POSTS/05-fast-cache-hit-economics.md` makes the components visible (storage / egress / operations). Self-hosted on S3: egress dominates. Self-hosted on bazel-remote-cache: operations dominate.
- **Counter-question:** "Have you measured your current cache TCO? Most teams haven't — egress + SRE-on-call is the underweighted part. If you have measured and we still cost more, I want to know which line drives it."

### Obj-27 — "Egress fees will kill us."

- **Why it matters:** S3-mental-model assumption.
- **Our position:** R2 has zero egress on cache reads. The fees you're worried about don't exist here.
- **Evidence:** `BLOG-POSTS/05-fast-cache-hit-economics.md#zero-egress-pricing-on-cloudflare-r2`. Customers pay for stored capacity and operations; not for bytes flowing out on cache hits.
- **Counter-question:** "Have you modeled what cache-hit egress looks like at 70% hit ratio on S3? That's the most common version of this conversation — once the customer sees the math, the objection changes shape."

### Obj-28 — "We need an ROI guarantee."

- **Why it matters:** procurement / finance trying to de-risk.
- **Our position:** we will not promise a specific percentage hit-rate improvement or specific dollar savings. We will promise the SLO catalog, sustained against 30 days of staging, and let your measurements do the rest.
- **Evidence:** `BLOG-POSTS/05-fast-cache-hit-economics.md#what-corelink-will-not-promise`. SLO catalog at `docs.corelink.dev/slo`. 14-day shadow period (no commitment) lets you measure.
- **Counter-question:** "Would a 14-day shadow period with no commitment, where you measure green-build wall-clock time against your existing cache, satisfy the ROI gate? If the measurement comes back unconvincing, you don't sign."

### Obj-29 — "We can build this in-house."

- **Why it matters:** legitimately true for some teams.
- **Our position:** acknowledge. Walk the engineering-year cost of doing it right.
- **Evidence:** A properly-engineered self-hosted cache is a part-time job for at least one engineer and a full-time job during bad weeks (eviction tuning, GC correctness, dedup, region replication, audit chain, BYOK with kill switch and rotation, residency invariants, 24/7 on-call). Multi-tenant + BYOK + audit chain + residency: that's a 6–12 month project minimum, plus ongoing carry. Compare against a year of CoreLink Enterprise.
- **Counter-question:** "If you do build in-house, what's the loaded engineer-year cost vs. our Enterprise list? And — orthogonally — is build-vs-buy aligned with your engineering org's stated priorities for the next 4 quarters? Most CTOs have an answer to that second question already."

### Obj-30 — "We need NET-90 / extended payment terms."

- **Why it matters:** AP cycle / cash management.
- **Our position:** standard is NET-30. We accommodate NET-60 / NET-90 in exchange for annual prepay or a smaller multi-year commit; the trade is symmetric.
- **Evidence:** standard MSA + order form structure. Annual prepay = 5% discount as a structural offset. Founder-level approval for NET-90+ as one-off.
- **Counter-question:** "Is the NET-90 ask absolute, or is it because the alternative is a procurement-cycle delay? If it's the latter, annual prepay + NET-30 can be net-neutral to your AP team and we usually land there."

---

## How to *use* this in conversation

1. **Acknowledge first.** Repeat the objection back in your own words. Verify you heard it right.
2. **Name the structural answer.** Don't lead with evidence; lead with the position.
3. **Surface one piece of evidence.** Not all of them; one. The rest are in this doc for follow-up.
4. **Ask the counter-question.** Make the prospect tell you what would actually unblock them.
5. **File new objections.** If you hear an objection that isn't here, drop it in `marketing/sales/OBJECTION-HANDLING.md` and tag the founder.

---

## Cross-references

- **FAQ Master:** `marketing/sales/FAQ-MASTER.md` (50 canonical Q/A).
- **Competitive matrix:** `marketing/sales/COMPETITIVE-MATRIX.md` (honest comparison).
- **Proof points:** `marketing/sales/PROOF-POINTS.md` (numeric claims with sources).
- **Trust Center:** `apps/docs/docs/trust/`.
- **Lighthouse playbook:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`.
- **Launch checklist:** `marketing/launch/LAUNCH-CHECKLIST-V2.md`.

---

**Fim SALES-OBJECTION-HANDLING.**
