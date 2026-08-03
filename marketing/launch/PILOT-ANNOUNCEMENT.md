# CoreLink Pilot Announcement — Executive Summary (PILOT-COMMS-001)

> **Status:** READY FOR OWNER PUBLICATION. Pilot program is **pre-GA**; CoreLink reaches General Availability per the engineering gate in `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md` + `specs/_compliance/GA-GATE-CRITERIA.md`. All claims below are verified against the spec corpus or labelled as pre-GA pilot scope.
> **Trace:** wave-28 step-7 pilot-comms-package · DEBT-027 (pilot-signup pipeline; ≥3 ACTIVE pilots required to GA) · wave-23 customer-success-playbook · wave-27 signup pipeline + admin scripts.
> **Freeze posture:** §3.b P1-GA-blocker prep (DEBT-027 ≥3 ACTIVE pilots) + §3.d cosmetic-doc; no engineering surface modified.

---

## Headline

**CoreLink Pilot — shared content-addressable cache for builds, packages, Docker, and ML.**

Run by HuGR Labs. Free during the pilot period. 30-day evaluation. Pre-GA.

---

## Offer at a glance

| Item | Pilot tier |
|---|---|
| **Price** | $0 for the full pilot period |
| **Storage quota** | **100 GB** content-addressable storage (CAS) |
| **Audit events** | **10,000 audit events / month** |
| **Evaluation window** | **30 days** from pilot activation |
| **Conversion** | Auto-convert to **STANDARD** tier at GA, **or** terminate without obligation |
| **Support** | Direct Slack channel with the engineering team; 4-hour business-hours response SLO |
| **Onboarding** | Hands-on; we wire your first cache backend with you |

> **No credit card. No auto-charge.** Conversion to a paid tier at GA requires an explicit opt-in by the pilot owner. If you take no action, the pilot terminates and we delete tenant data per `specs/_runbooks/RB-TENANT-OFFBOARD.md`.

---

## What CoreLink is (one paragraph)

CoreLink is a **shared, tenant-isolated, content-addressable cache** for any workload that benefits from blob deduplication and cryptographic addressing. It is REAPI-compatible (Bazel / Buck2 / Pants / Remote Build Execution) and exposes a generic S3-style content-addressable API for non-build workloads — Docker layer caches, Nix store mirrors, package registries (npm, PyPI, cargo), and ML model / dataset registries. Built on Cloudflare R2 + Workers + D1. Multi-region replication across four regions (US-East, EU-West, AP-Southeast, AU-East). Per-tenant cryptographic audit chain with append-only Merkle proofs.

## Who we are looking for

You are a fit for the pilot if **at least one** is true:

- Your team runs **Bazel / Buck2 / Pants / Nix** remote build caches and the current cache is single-tenant, self-hosted, or operationally expensive.
- You run a **Docker layer cache** or **OCI registry** and want cross-region dedup + audit-chain provenance.
- You operate an **ML model / dataset registry** and need content-addressed, cryptographically verifiable artefact storage.
- You run an **internal package registry** (npm / PyPI / cargo / Maven mirror) and want shared, deduplicated, audit-logged storage.

You are **not** a fit if you need any of the following before GA:
- Production SLA contract (we publish target SLOs; pilot is best-effort).
- BYOK / customer-managed encryption (BYOK ships at GA per `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`).
- SOC 2 Type II report (gap analysis delivered pre-GA; Type II observation window starts at GA).
- Schrems-II DPA (template available; signed DPA conditional on legal review timeline).

## What you commit to

1. **One 30-minute kickoff** with our engineering team.
2. **One 30-minute mid-pilot check-in** (day 14).
3. **One 30-minute exit interview** (day 30).
4. **Feedback in writing** — at minimum a 5-question survey at the end of the pilot.

## What we commit to

1. **Direct engineering Slack channel** for the duration of the pilot.
2. **4-hour business-hours response SLO** for any pilot-tagged ticket.
3. **No surprise data deletion.** If you opt to terminate, you get 14 days to export under tenant-scoped credentials, and we perform a verifiable crypto-erasure (a customer-served signed erasure attestation is on the near-term roadmap).
4. **No use of your data for marketing without written consent.** Logos and quotes only with your prior approval.

## Apply

Reserve a pilot slot via the token-gated signup flow (wave-27 pilot-signup pipeline):

**`https://corelink-docs.humangr.com/pilot/apply`**

Slots are limited to the **first 10 qualified applicants** (DEBT-027 minimum to GA is ≥3 ACTIVE pilots; we are sizing the program to leave headroom for two cohorts). Application review SLO: 2 business days. Activation SLO: 5 business days post-acceptance.

## What's pilot vs GA — honest accounting

| Capability | Pilot | GA |
|---|---|---|
| Multi-tenant CAS (BLAKE3 / SHA-256) | yes | yes |
| Tenant isolation modelled in TLA+ | yes | yes |
| Audit chain (append-only Merkle) | yes | yes |
| Multi-region replication (4 regions) | yes | yes |
| REAPI compatibility (Bazel / Buck2) | yes | yes |
| Generic S3-style API (Docker / Nix / ML) | yes | yes |
| **BYOK (AWS KMS at GA; GCP/Azure/Vault roadmap)** | **no** | yes (AWS KMS) |
| **SOC 2 Type II report** | **no — gap analysis only** | yes (observation window begins at GA) |
| **Production-tier SLA contract** | **no — best-effort target SLOs** | yes |
| **Signed DPA / SCC** | **template only** | yes |
| **Customer-managed kill switch** | **no** | yes |

If you need any of the GA-only items before signing, **wait for GA** and we'll re-engage. We will not sell you a pilot that can't meet your procurement bar.

## Contact

- Pilot signup: `https://corelink-docs.humangr.com/pilot/apply`
- Direct outreach: `pilot@humangr.com`
- Engineering escalation (post-acceptance only): pilot Slack channel

---

*HuGR Labs — Human Guardrail. CoreLink is a HuGR product. Last updated 2026-05-16.*
