# Published-claims audit — 2026-08-24

What the public docs site asserts, checked against what this repository
implements. Every row below was verified by hand against the cited
`file:line` after a six-agent sweep produced it; agent-reported findings that
did not survive that check are not listed.

The audit was triggered by making the `broken-links` gate run unauthenticated
for the first time. Authenticated it reported 338 problems; unauthenticated it
reported 2,589 — the number a customer sees.

## Closed in this branch

| # | Page | Claim | Reality |
|---|---|---|---|
| 1 | `src/pages/trust/incident-history.tsx` | two drills with 7m12s / 42m18s / RPO 0, each linking to a `drill-evidence/` file | neither evidence file has ever existed; the only drill doc is an unexecuted dry-run template. Entries removed, honest empty state added |
| 2 | `docs/trust/compliance.mdx:41` | "Audit partner: Schellman & Co., LLC (engagement letter executed R5-1)" | every internal source lists Schellman as a *candidate* next to A-LIGN, all DRAFT; no executed letter exists |
| 3 | `docs/trust/compliance.mdx:42` | "Continuous-evidence platform: Drata" | `VENDOR-RISK-REGISTER.md:148` — `DRATA_API_KEY` on none of 10 prod Workers, nothing invokes the client |
| 4 | `explanation/privacy/gdpr.mdx:53`, `lgpd-full.mdx:56`, `src/pages/legal/privacy.tsx` | consent withdrawal "≤ 5 minutes" via `DELETE /v1/consent/<purpose>` | `corelink-privacy` is not a dependency of `corelink-container` or any Worker; no `/v1/consent` route is mounted |
| 5 | `docs/trust/fedramp-info.mdx:103` | "AWS KMS is FIPS 140-3 L3 attested today (per the matrix)" | the cited `BYOK-FIPS-ATTESTATION-MATRIX.md:68` says **Level 1** |
| 6 | `docs/trust/pci-dss.mdx:98,164` | verification steps citing `crates/corelink-logpush/` | that crate does not exist; the code is in `crates/corelink-telemetry/` |
| 7 | site-wide | "file an issue" / "GitHub Discussions" (29 links, 4 locales) | the repository is private; every link 404s. Now `support@humangr.com` |

## Open — needs a decision, not a rewrite

**A. Status badges that sell something production does not do.**

- `src/pages/trust/center.tsx:135` publishes **BYOK envelope encryption — LIVE**,
  "four KMS providers, CoreLink only holds wrapped DEKs". `Dockerfile:182` builds
  `cargo build --release --locked -p corelink-server --bin corelink-server` with no
  `--features byok-aws-real`, so `active_provider()` is `InMemoryFake` — an XOR
  against a fixed constant, doc-marked not for production.
- Same file: **audit chain, 7-year retention — LIVE**. `corelink-audit-chain/Cargo.toml:3`
  defers R2 Object Lock Governance 7y; the wired sink is `InMemoryR2AuditSink`.
- Same file: **customer audit-log export — LIVE**. `audit_export/state.rs:80` says the
  exporter is in-memory, lost on restart, not shared across containers.

Either downgrade the badges or wire the real providers. Wiring lives on the
quarantined `feat/remediation-gated-features` branch.

**B. Pricing that disagrees with itself and with the code.**

`explanation/pricing/comparison.mdx` prices Starter at **$29**; the canonical
`src/lib/pricing.ts:188` says **$35**. The same page invents a **"CoreLink Team"
tier at $199** that is not in the six-tier taxonomy. `pricing/index.mdx` advertises
a **$99/mo BYOK add-on** with no Stripe price and no entitlement, per-tier **seat
caps** that `handle_team_invite` never enforces, **audit-log retention** wrong in
three tiers against `corelink-audit/src/retention.rs`, and a **429 hard cap** that
`corelink-ratelimit/src/tier.rs:97` calls "a no-op today". The page is
`draft: true` and published anyway. Recommend unpublishing it until reworked,
because every number on it is either wrong or unenforced.

**C. The OpenAPI document ships endpoints that do not exist.**

`openapi/corelink-v1.yaml` declares five `privacy-consent` operations with
rate-limit classes and auth schemes. The API reference pages are generated from
it, so a customer generating an SDK gets methods that 404. Hand-editing the
generated pages reverts; the fix is in the YAML, and removing a published
operation is an API-surface decision.

**D. Two undeclared sub-processors (GDPR Art. 28).**

- **Sentry** — `sentry.server.config.ts` and `sentry.edge.config.ts` call
  `Sentry.init`; four secrets are registered. Absent from `legal/sub-processors.md`
  and from `VENDOR-RISK-REGISTER.md`.
- **Plausible** — `apps/docs/docusaurus.config.ts:232` injects the script
  unconditionally on `corelink-docs.humangr.com`. Absent from both registers.

Same defect class as the Resend omission.

**E. Erasure fan-out is documented against a design, not the build.**

`gdpr.mdx:148` and `lgpd-full.mdx:169` describe twelve erasure targets. Only four
adapters exist — `adapter_r2_cas.rs`, `adapter_r2_ac.rs`, `adapter_d1.rs`,
`adapter_stripe.rs`; the rest return `NotApplicable` because, per
`adapter_not_applicable.rs:1`, the backends "were never shipped".

**F. Residency description does not match the region map.**

`src/pages/legal/privacy.tsx:299` lists `Sam` among regions with enforced
isolation and places the EU region in Frankfurt. `region_map.rs` provisions
`wnam/enam/weur/apac` only, and routes `weur` to `lhr` (London). Mitigating:
signup rejects `sam` and `afr`, so no tenant can be pinned there — the page is
wrong, but no data is mis-landing.

## Staleness measurement

Of ~227 published pages, 92 were touched in the last three days. But 24 have not
been touched since mid-May, and they are precisely the commercial and legal
surface: `compliance/dpa`, `compliance/soc2-timeline`, `compliance/pentest-summary`,
`compliance/sub-processors`, `privacy/gdpr`, `privacy/lgpd-full`,
`compliance/sbom-access`, `security/incident-history`,
`security/responsible-disclosure`, `pricing/comparison`.

The part of the site that talks to engineers is alive. The part that talks to
buyers and to lawyers froze in May.
