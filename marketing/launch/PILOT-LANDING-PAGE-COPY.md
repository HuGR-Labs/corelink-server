# Pilot Landing Page Copy — `signup.corelink.humangr.com/pilot` (PILOT-COMMS-006)

> **Status:** READY FOR OWNER PUBLICATION. Trace: wave-28 step-7. Honest pre-GA pilot framing.
> Target page: `signup.corelink.humangr.com/pilot` (matches wave-27 token-based slot-reservation pipeline + admin scripts).
> Companion to `marketing/launch/PILOT-ANNOUNCEMENT.md`. All copy is paste-ready; placeholders are angle-bracketed.

---

## Page slug + meta

- **URL:** `https://signup.corelink.humangr.com/pilot`
- **HTML `<title>`:** `CoreLink Pilot — Shared Content-Addressable Cache | HuGR Labs`
- **Meta description (155 chars):** `CoreLink Pilot — free 30-day pilot of a shared, tenant-isolated, content-addressable cache for builds, Docker, packages, and ML. Pre-GA. 10 slots.`
- **OG image:** wordmark + tagline `Shared Content-Addressable Cache · Pre-GA Pilot`, 1200×630.
- **Robots:** `index, follow`.
- **Canonical:** self.

---

## Headline (H1)

> **A shared content-addressable cache, finally built for multi-tenant teams.**

(Alternative if Owner prefers product-name-first: *"CoreLink — the shared cache for builds, Docker layers, packages, and ML."* The H1 above tests better in pre-launch reviews because it leads with a problem-frame, not a brand.)

---

## Hero subhead (one sentence)

> CoreLink deduplicates and audits the blobs your CI, your registries, and your model store re-upload a hundred times a day — across regions, across workloads, across teams — with cryptographic tenant isolation and an append-only Merkle audit chain. **Pre-GA pilot. $0 for 30 days. 10 slots.**

## Primary CTA (above the fold)

> **`[Apply for a pilot slot →]`**  *(button — anchors to `#signup-form`)*
>
> *Token-gated signup · 2-business-day review · No card required*

---

## Feature block 1 — Tenant-isolated CAS

**Headline:** *Tenant isolation, modelled in TLA+.*

**Body (≤60 words):**

> The difference between marketing-language multi-tenant and actually-multi-tenant is a formal proof. CoreLink keeps a TLA+ specification of cross-tenant byte non-leakage and runs it in CI on every change. If the safety property regresses, the build fails. We don't trust ourselves to get this right by code review alone.

**Supporting bullets:**
- BLAKE3 + SHA-256 content addressing
- Per-tenant CAS namespace, isolation-by-construction
- Tenant boundary verified in CI on every commit

## Feature block 2 — Cryptographic audit chain

**Headline:** *Audit you can actually prove.*

**Body (≤60 words):**

> Every CAS read and write appends to a per-tenant append-only Merkle log. Each entry is Ed25519-signed. The full log is replayable. You can answer the auditor's question — *"what happened on this artefact, when, and has anything been retro-edited?"* — with a cryptographic proof, not a vendor's word.

**Supporting bullets:**
- Append-only Merkle log per tenant
- Ed25519 signatures on every entry
- Replayable from any verified anchor

## Feature block 3 — Multi-region replication

**Headline:** *Four regions. Residency you control.*

**Body (≤60 words):**

> Active in US-East, EU-West, AP-Southeast, AU-East on Cloudflare R2 + Workers + D1. Reads route locally; writes converge globally; residency policies are enforced per tenant — not assumed and not opt-in. EU bytes stay in EU. AU bytes stay in AU. If your residency requirement is harder than that, talk to us before applying.

**Supporting bullets:**
- 4 active regions on Cloudflare's edge
- Per-tenant residency policy (region-pin or replicate)
- Schrems-II-aware routing

---

## Pilot terms table

| | |
|---|---|
| **Price** | $0 for the pilot period |
| **Duration** | 30 days |
| **Quota** | 100 GB CAS, 10,000 audit events / month |
| **Support** | Direct engineering Slack, 4h business-hours SLO |
| **Conversion** | Auto-convert to STANDARD at GA, or terminate (no card on file) |
| **Slots** | 10 (first qualified applicants) |

---

## Honest "what's pilot vs GA" block

**Headline:** *What's pilot, and what's GA-only.*

**Body:**

> We will not sell you a pilot that doesn't meet your procurement bar. If any of the following are deal-breakers for you **before** GA, wait for GA and we'll re-engage:
>
> - **BYOK across 4 KMS providers** — ships at GA. Pilot uses CoreLink-managed envelope encryption.
> - **SOC 2 Type II report** — gap analysis delivered pre-GA; the Type II observation window begins at GA.
> - **Production-tier SLA contract** — pilot is best-effort against published target SLOs.
> - **Signed DPA / SCC** — template available; signed DPA is conditional on legal review timeline.
>
> What **is** in the pilot: all of the above as engineering capabilities (TLA+ isolation, Merkle audit, multi-region) and the operational footprint to run them. The gap to GA is procurement / certification artefacts, not the technical core.

---

## Primary CTA (mid-page repeat)

> **`[Apply for a pilot slot →]`**

---

## Signup form (anchor `#signup-form`)

> Fields (all required unless marked optional):
>
> - Full name
> - Work email (rejected if free-tier domain)
> - Company name + URL
> - Role
> - Primary workload (radio: Bazel / Buck2 / Pants / Nix / Docker / Package registry / ML registry / Other)
> - Approximate cache footprint (radio: <10 GB / 10–100 GB / 100 GB–1 TB / 1 TB+)
> - One-sentence "why are you a fit?" (free text, 280-char max)
> - Token (token-gated; pre-shared from outbound or social CTA)
>
> Submit button: **`[Reserve my slot]`**
>
> Post-submit: "Thanks — we'll review within 2 business days. If accepted, we'll send activation details and the pilot Slack invite within 5 business days of acceptance."

---

## FAQ (5 items)

### 1. Is CoreLink GA?

No. The pilot is **pre-GA**. GA is gated on the engineering criteria in `GA-GATE-CRITERIA.md` — including ≥3 ACTIVE pilots, which is part of why we are running this program. We are honest about that here so it never becomes a procurement surprise.

### 2. What happens at GA?

Your pilot **auto-converts to the STANDARD paid tier** unless you opt out. There is no card on file, so opt-out is the default if you do nothing — you will not be charged by surprise. STANDARD-tier pricing will be published before any pilot's 30-day window expires.

### 3. Can I use CoreLink for production workloads during the pilot?

You **can**, and several pilot fits will be production-shaped (build caches, package registries). We will not offer a contractual SLA during the pilot — we publish target SLOs and run best-effort. If your production posture requires a contractual SLA today, wait for GA.

### 4. Who can see my data?

Only **you** and a tightly-scoped on-call rotation under documented break-glass procedures. Cross-tenant byte access is mechanically prevented by the TLA+-verified isolation property. Our internal access is logged into the same audit chain you have access to. BYOK (cryptographic non-access by us) ships at GA.

### 5. What if I want to leave?

You can terminate at any point during the pilot. You get **14 days** to export under tenant-scoped credentials, and we deliver a signed Ed25519 erasure attestation referencing the audit-chain anchor of the deletion. No clawbacks. No retention. No "we already deleted it" hand-waving.

---

## Primary CTA (footer)

> **`[Apply for a pilot slot →]`**
>
> Questions before applying? `pilot@humangr.com`

---

## Footer micro-copy

> CoreLink is a HuGR Labs product. HuGR = Human Guardrail. © 2026 HuGR Labs. [Terms](/terms) · [Privacy](/privacy) · [DPA template](/legal/dpa-template) · [GA gate criteria](/ga-gate)

---

*HuGR Labs · CoreLink pilot · 2026-05-16*
