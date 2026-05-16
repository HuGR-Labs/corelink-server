# Pilot Tweet / X Thread — Copy-Paste-Ready (PILOT-COMMS-002)

> **Status:** READY FOR OWNER PUBLICATION. Trace: wave-28 step-7. Honest pre-GA pilot framing.
> Format: 8 tweets, hard cap 280 chars each. Numbered `1/8` … `8/8`. No emoji unless Owner overrides.
> Companion to `marketing/launch/PILOT-ANNOUNCEMENT.md`.

---

## Tweet 1/8 — Hook

> Tired of slow CI builds? Re-uploading the same 200 MB blob 50x across regions, every PR, every day?
>
> Your build cache is the most leveraged latency primitive in your inner loop. Most teams accept the tax.
>
> We didn't. 1/8

**Char count:** 264.

## Tweet 2/8 — Introduce CoreLink

> CoreLink is a shared, tenant-isolated, content-addressable cache for builds, Docker layers, Nix stores, package registries, and ML model registries.
>
> One backend. Cross-region dedup. Cryptographic audit chain.
>
> By @humangr_labs. 2/8

**Char count:** 263.

## Tweet 3/8 — Capability 1 — Tenant-isolated CAS

> Capability 1: tenant-isolated content-addressable storage.
>
> BLAKE3 + SHA-256 addressing. Per-tenant CAS namespace. Cross-tenant byte access modelled in TLA+ and checked in CI — the invariant fails the build if regressed. 3/8

**Char count:** 234.

## Tweet 4/8 — Capability 2 — Cryptographic audit chain

> Capability 2: cryptographic audit chain.
>
> Every CAS read/write appends to a per-tenant append-only Merkle log. Ed25519-signed. Replayable. You can prove what happened, when, and that nothing was retro-edited. 4/8

**Char count:** 218.

## Tweet 5/8 — Capability 3 — Multi-region replication

> Capability 3: multi-region replication.
>
> Active across US-East, EU-West, AP-Southeast, AU-East on Cloudflare R2 + Workers + D1. Read locally; write once; converge globally. Residency-aware routing per tenant policy. 5/8

**Char count:** 232.

## Tweet 6/8 — Pilot offer

> The pilot offer:
>
> - $0 for 30 days
> - 100 GB CAS + 10k audit events / month
> - Auto-convert to STANDARD at GA, or walk away
> - Direct Slack with the eng team
> - Pre-GA — honest about that
>
> We need ≥3 ACTIVE pilots to hit GA. 6/8

**Char count:** 240.

## Tweet 7/8 — Who we are looking for

> Looking for:
>
> - Build infra teams on Bazel / Buck2 / Pants / Nix
> - Docker / OCI registry operators
> - ML platform teams shipping content-addressed model registries
> - Internal package registry maintainers (npm / PyPI / cargo)
>
> 7/8

**Char count:** 256.

## Tweet 8/8 — CTA + apply link

> Apply: signup.corelink.humangr.com/pilot
>
> 10 slots. 2-business-day review. 5-business-day activation. Token-gated signup so we can actually onboard you well.
>
> Questions: pilot@humangr.com
>
> 8/8

**Char count:** 196.

---

## Posting notes (Owner-only — do not paste)

- **Cadence:** post all 8 in one thread within 90 seconds; X's algorithm penalises slow-drip threads.
- **Best slot:** Tue / Wed / Thu 09:00–11:00 PT for dev-tools audience.
- **Reply-amplify:** quote-reply tweet 1 from `@humangr_labs` account with a single screenshot of the pilot landing page.
- **Do NOT** add "🧵" or "thread" emoji — corporate launch threads with that gimmick read as marketing-noise to the build-infra audience we want.
- **Char counts assume** X premium accounts (Owner has confirmed access per wave-23 social comms inventory). All tweets fit free-tier 280 ceiling regardless.
- **If a tweet must be re-cut for free tier or character creep,** preserve the hook/capability/offer/CTA structure; do not collapse capability tweets (3/4/5) into one — separation drives QT engagement.

## A11y — alt-text briefs (optional images)

If Owner wants images, here are 1-sentence alt-text briefs (1 image / tweet max; not required):

1. Hook tweet: bar chart of CI build minutes wasted on duplicate blob re-uploads (synthetic).
2. Intro tweet: CoreLink wordmark + 5 workload icons (Bazel, Docker, Nix, npm, model).
3. Capability 1: redacted TLA+ `tenant_isolation` invariant snippet.
4. Capability 2: Merkle audit-chain diagram, 3 leaves + root.
5. Capability 3: 4-region world map with replication arrows.
6. Offer: pilot tier table — quota / window / conversion.
7. Looking-for: 4-quadrant grid of target personas.
8. CTA: signup page screenshot or QR code.

---

*HuGR Labs · CoreLink pilot · 2026-05-16*
