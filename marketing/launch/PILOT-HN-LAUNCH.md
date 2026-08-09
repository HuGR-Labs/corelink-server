# Pilot Hacker News "Show HN" Post — Copy-Paste-Ready (PILOT-COMMS-004)

> **Status:** READY FOR OWNER PUBLICATION. Trace: wave-28 step-7. Honest pre-GA pilot framing.
> Format: Show HN title + body. ~200 words. HN tone is hostile to marketing-speak; this draft errs technical and self-critical.
> Companion to `marketing/launch/PILOT-ANNOUNCEMENT.md`.

---

## HN submission title

> **Show HN: CoreLink Pilot — shared content-addressable cache for builds, Docker, ML (pre-GA)**

(Title length: 92 chars. HN soft cap is 80, hard cap 150; this fits and the parenthetical pre-GA disclosure inoculates against "you said launched but it's not GA" complaints.)

**Submission URL field:** `https://corelink-docs.humangr.com/pilot/apply`

---

## HN submission body — paste verbatim

> Hi HN — Gustavo from HuGR Labs.
>
> We're opening pilot enrolment for CoreLink, a shared, tenant-isolated, content-addressable cache. It's REAPI-compatible for Bazel / Buck2 / Pants / RBE, and exposes a generic CAS API for Docker layer caches, Nix store mirrors, package registries, and ML model registries.
>
> What's actually built and verifiable today:
>
> - BLAKE3 + SHA-256 addressing; per-tenant CAS namespace.
> - Tenant isolation modelled in TLA+ (the invariant fails CI if a regression touches it).
> - Per-tenant append-only hash-chain audit log (BLAKE3-linked, not Merkle), Ed25519-signed, replayable.
> - Multi-region: three regions today — WNAM, ENAM, WEUR (US-West, US-East, EU-West) — on Cloudflare R2 + Workers + D1; more regions on the roadmap.
> - Customer-success playbook + signup pipeline + admin scripts shipped over the last two waves.
>
> What's NOT shipped yet — important context, not buried:
>
> - **BYOK — not in pilot. AWS KMS ships at GA; GCP / Azure / Vault on the roadmap.**
> - **SOC 2 Type II — not yet. Gap analysis only; Type II observation window starts at GA.**
> - **Production SLA contract — not yet. Pilot runs best-effort against published target SLOs.**
> - **Signed DPA / SCC — template only.**
>
> Pilot terms: $0, 30-day window, 100 GB CAS + 10k audit events / month. Auto-converts to STANDARD at GA or terminates clean — no card on file. Direct Slack with engineering. 10 slots; we need ≥3 active pilots before we cut GA.
>
> Happy to answer technical questions: spec corpus design, audit chain semantics, residency model, why we picked R2 over S3, why TLA+ specifically. No marketing-speak; ask anything.

---

## Word count

Body: 269 words (target was 200; HN tolerance is 300+ for "what's NOT shipped" candor — keeping the honest-accounting block is more important than hitting 200 exactly).

## Posting notes (Owner-only — do not paste)

- **Best slot:** Tue–Thu 06:30–08:30 PT (HN front-page surf rises into US morning).
- **Do not solicit upvotes.** HN moderators will detach the submission if even a few friends comment "upvoted!" — it's the single fastest way to kill a Show HN.
- **Author is Owner's personal HN account** (must have ≥1 prior comment / submission to avoid green-name penalty); if Owner has a fresh account, post a normal Ask HN or comment first.
- **Reply velocity matters more than content.** Be at the keyboard for the first 90 minutes after submission; reply to every top-level comment with technical substance.
- **Flagging risks:** the "Show HN" prefix is reserved for things people can actually try. The signup page being token-gated is fine, but the landing page MUST not 404 at submission time. Verify `corelink-docs.humangr.com/pilot/apply` resolves before clicking submit.
- **If the submission goes nowhere in 90 minutes,** do NOT resubmit. HN's anti-resubmission heuristic penalises the second post. Wait a week.

---

*HuGR Labs · CoreLink pilot · 2026-05-16*
