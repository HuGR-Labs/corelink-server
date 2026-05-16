# Pilot Target List — Direct Outreach (INTERNAL) (PILOT-COMMS-007)

> **Status:** INTERNAL. Do not share externally. Trace: wave-28 step-7 pilot-comms-package · DEBT-027 (≥3 ACTIVE pilots to GA).
> **Source basis:** public knowledge of the developer-tools ecosystem in 2025–2026 — public engineering blog posts, public hiring pages, public conference talks, public GitHub repos. **No private outreach intel.** Personal contact details are intentionally omitted; the outbound flow is "find a contact via the company's public channels, then send the email template from `PILOT-EMAIL-BLAST.md`."
> **Cohort sizing:** 10 pilot slots (DEBT-027 minimum to GA is ≥3 ACTIVE). This list is **30 candidates** — 3× the slot count for funnel headroom. Tier-1 (highest fit) listed first within each segment.

---

## Segment A — Build-cache power users (Bazel / Buck2 / Pants / Nix) (12 candidates)

Teams known to operate large remote build caches and have publicly described pain.

| # | Company / team | Why a fit (public signals) | Cohort tier |
|---|---|---|---|
| A1 | **Pinterest — Build Infrastructure** | Public Bazel migration blog posts; large multi-repo footprint; public job listings citing remote-cache scaling. | Tier-1 |
| A2 | **Lyft — Mobile Build Platform** | Public Buck2 adoption talks; published cache-hit-rate metrics; multi-region engineering presence. | Tier-1 |
| A3 | **Reddit — Build Platform** | Bazel-based monorepo; public talks on remote execution; mid-sized footprint. | Tier-1 |
| A4 | **Snowflake — Developer Infra** | Bazel monorepo; public hiring for build-infra engineers in 2025. | Tier-1 |
| A5 | **Coinbase — Developer Productivity** | Public Buck → Buck2 migration commentary; large Android/iOS build footprint. | Tier-1 |
| A6 | **Shopify — Build Infrastructure** | Public Bazel adoption posts; large monorepo; remote-cache cost optimisation publicly discussed. | Tier-1 |
| A7 | **Stripe — Build & CI Platform** | Public Bazel commentary; large monorepo + heavy CI footprint. | Tier-1 |
| A8 | **Datadog — Build Platform** | Public Go-monorepo + Bazel posts; well-known build-pipeline blog cadence. | Tier-2 |
| A9 | **HashiCorp — Engineering Productivity** | Mix of Go monorepo + multi-product CI; published cache-strategy thinking. | Tier-2 |
| A10 | **GitHub — Internal Platform** | Public commentary on build-cache design; engineering productivity org. | Tier-2 |
| A11 | **Replit — Nix Platform** | Public Nix-store usage at scale; could test the generic-CAS surface as a Nix substituter. | Tier-2 |
| A12 | **Determinate Systems** | Nix-store cache product; potential partner-pilot (cross-team, not customer-pilot). | Tier-3 (partner) |

## Segment B — Docker / OCI registry operators (6 candidates)

Teams running internal OCI registries who would benefit from cross-region dedup + audit-chain provenance.

| # | Company / team | Why a fit (public signals) | Cohort tier |
|---|---|---|---|
| B1 | **GitLab — Container Registry** | Public registry-architecture posts; multi-tenant SaaS operator. | Tier-1 |
| B2 | **Sigstore Project — Rekor** | Public interest in cryptographic transparency for OCI; potential ecosystem-pilot. | Tier-1 (eco) |
| B3 | **Chainguard — Image Distribution** | Public OCI-distribution engineering blog; founder ecosystem overlap with HuGR. | Tier-1 |
| B4 | **Docker Inc. — Hub Infra** | Operates the largest public OCI registry; cohort-fit if a non-public regional mirror is in scope. | Tier-2 |
| B5 | **JFrog — Artifactory Engineering** | Adjacent product; could be partner-pilot rather than customer-pilot. | Tier-3 (partner) |
| B6 | **Harbor Project (CNCF)** | Open-source OCI registry; potential ecosystem-pilot, not a paying customer. | Tier-3 (eco) |

## Segment C — ML platform teams (model / dataset registries) (6 candidates)

Teams shipping ML platforms with content-addressed model or dataset stores.

| # | Company / team | Why a fit (public signals) | Cohort tier |
|---|---|---|---|
| C1 | **Hugging Face — Hub Infrastructure** | Public infra blog posts on LFS / dedup; obvious workload fit; founder ecosystem overlap. | Tier-1 |
| C2 | **Weights & Biases — Artifacts** | Content-addressed artefact store; could test the generic-CAS surface for model/dataset blobs. | Tier-1 |
| C3 | **Replicate — Model Registry** | Public content-addressed model registry; small engineering org; fast eval cycle likely. | Tier-1 |
| C4 | **Modal Labs — Function Store** | Public function/image caching infra; founder-led; fast pilot motion possible. | Tier-1 |
| C5 | **Anyscale — Ray Platform** | Public dataset+model pipeline footprint; cross-region replication is a known pain. | Tier-2 |
| C6 | **OctoML / OctoAI** | Model inference infra; OCI-image-heavy footprint. | Tier-2 |

## Segment D — Internal package registry maintainers (6 candidates)

Teams operating internal npm / PyPI / cargo / Maven mirrors.

| # | Company / team | Why a fit (public signals) | Cohort tier |
|---|---|---|---|
| D1 | **Cloudflare — Workers Build Infra** | Existing CF partnership posture (CoreLink runs on CF R2/Workers); could be partner-pilot or dogfood-pilot. | Tier-1 (partner) |
| D2 | **Bytecode Alliance — Wasm registry** | Public interest in content-addressed component registry; ecosystem alignment. | Tier-1 (eco) |
| D3 | **Sentry — SDK Build Pipeline** | Public multi-language SDK release pipeline; mirror-and-cache motion well-documented. | Tier-2 |
| D4 | **Vercel — Build Cache Platform** | Public build-cache engineering content; potential partner-pilot. | Tier-2 (partner) |
| D5 | **Netlify — Build Cache** | Adjacent product; partner-pilot candidate. | Tier-3 (partner) |
| D6 | **Cloudsmith — Universal Registry** | Adjacent product; partner-pilot candidate; potential cross-sell rather than direct compete. | Tier-3 (partner) |

---

## Outreach playbook — short version

1. **Find contact via public channels** — engineering blog author bylines, public Slack/Discord, conference-speaker profiles, GitHub org `engineering@` aliases. Never scrape private contact data.
2. **Send `PILOT-EMAIL-BLAST.md` template,** subject-line variant A. Personalise the "looks like a clean fit" sentence with one specific public signal (a blog post, a talk, a GitHub repo).
3. **CRM-tag every send** with `cohort:pilot-outbound-wave28`, `segment:A|B|C|D`, `tier:1|2|3`, `signal:<one-liner>`.
4. **Sequence:** initial + day-4 follow-up + day-10 break-up. Three touches max.
5. **Stop list:** if a contact replies "no" or "not now", flag the **company** (not just the contact) as `do-not-reach until 2026-Q4`. Respect it.

## Expected funnel (rough order-of-magnitude — for sizing, not forecasting)

- 30 sends → expect 8–12 reply-positive → expect 4–6 to qualify → expect 3–5 to activate.
- That funnel exactly hits DEBT-027's ≥3 ACTIVE pilots with two-pilot margin.

## Cohort assignment guardrails

- **Do not run two direct-competitor pilots in the same cohort window.** If A1 (Pinterest) and A2 (Lyft) both accept, stagger their activation by 2 weeks so feedback signals don't blur.
- **At least one Tier-1 from each of segments A / B / C must be in cohort 1** to validate cross-workload claims (build / Docker / ML) — those are the three feature blocks on the landing page and on the X thread.
- **Partner-pilots and ecosystem-pilots do not count** toward DEBT-027's ≥3 ACTIVE pilot threshold. Customer-pilots only.

## Privacy / opsec notes

- This list is **internal** and not for external sharing or attribution. Move-the-needle data only.
- Do not paste this list into any LLM-backed tool that does not have a no-train clause.
- Public-signal references are not redistributable as "X is interested in CoreLink." A public engineering blog post is a signal, not a quote. Quote attribution requires explicit written consent.

---

*HuGR Labs · CoreLink pilot · internal · 2026-05-16*
