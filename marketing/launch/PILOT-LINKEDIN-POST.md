# Pilot LinkedIn Post — Copy-Paste-Ready (PILOT-COMMS-003)

> **Status:** READY FOR OWNER PUBLICATION. Trace: wave-28 step-7. Honest pre-GA pilot framing.
> Format: long-form LinkedIn post, ~250 words, technical narrative voice. Single post; no carousel.
> Companion to `marketing/launch/PILOT-ANNOUNCEMENT.md`.

---

## Post body — paste verbatim

> Today we are opening pilot enrolment for CoreLink.
>
> CoreLink is a shared, tenant-isolated, content-addressable cache. It is REAPI-compatible (Bazel, Buck2, Pants, Remote Build Execution) and exposes a generic content-addressable API for any workload that benefits from cryptographic addressing and dedup — Docker layer caches, Nix store mirrors, internal package registries (npm / PyPI / cargo / Maven), and ML model and dataset registries.
>
> Three properties are not optional in our design:
>
> 1) **Tenant isolation is a TLA+ invariant.** We maintain a formal specification of cross-tenant byte non-leakage and the CI fails if the safety property regresses. The difference between marketing-language multi-tenant and actually-multi-tenant is exactly the kind of gap formal verification was invented to close.
>
> 2) **Audit is cryptographic.** Every CAS read and write appends to a per-tenant append-only hash chain (BLAKE3-linked, Ed25519-signed, replayable). You can prove what happened, when, and that nothing was retro-edited.
>
> 3) **Replication is multi-region, residency-aware.** Three regions today — WNAM, ENAM, WEUR (US-West, US-East, EU-West) — with more on the roadmap. Read locally. Write once. Converge globally. Residency policies are enforced per tenant, not assumed.
>
> The pilot is **pre-GA** and we are honest about that. No BYOK yet. No SOC 2 Type II yet (gap analysis only). No production SLA contract — we publish target SLOs and run best-effort during the pilot. If you need any of those today, wait for GA. If you can evaluate ahead of GA on the merits of the technical core, we want to talk.
>
> Pilot offer: $0 for 30 days, 100 GB CAS, 10k audit events / month, direct Slack with engineering. Auto-convert to STANDARD at GA or walk away — no card on file.
>
> 10 slots. Token-gated signup. Apply at `corelink-docs.humangr.com/pilot/apply`.
>
> We need ≥3 active pilots before we cut GA. If your team runs Bazel / Buck2 / Pants / Nix remote caches, a Docker / OCI registry, an ML model registry, or an internal package mirror, this is the call.

---

## Word count

277 words (under 300-word LinkedIn engagement target; above 200-word minimum for long-form algorithm preference).

## Posting notes (Owner-only — do not paste)

- **Best slot:** Tue–Thu 08:00–10:00 local time for dev-tools audience.
- **Hashtags:** none in body; LinkedIn's algorithm in 2026 weighs hashtags neutrally and they read as corporate-noise to engineers. Optional trailing comment with `#DevTools #BuildSystems #ContentAddressableStorage` if Owner wants discovery; do **not** add to body.
- **Co-post:** ask first two pilot prospects to repost on day 2 (if accepted into pilot pre-launch).
- **Crosspost:** post X thread (`PILOT-TWEET-THREAD.md`) 60 minutes before this post; LinkedIn audience trails the X audience by ~1h.
- **Image:** optional — single 1200×627 banner with CoreLink wordmark + tagline. Not required; image-less long-form posts have higher dwell-time engagement in technical audiences.
- **First comment hook:** drop a single comment within the first 5 minutes from the Owner's personal account: *"Happy to answer technical questions in this thread — formal spec details, audit chain semantics, residency policies."* Drives reply velocity which the algorithm rewards.

---

*HuGR Labs · CoreLink pilot · 2026-05-16*
