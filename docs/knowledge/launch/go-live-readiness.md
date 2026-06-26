---
type: "LaunchControl"
title: "Go-live readiness / launch due-diligence"
description: "The 2026-06-15 pre-launch due-diligence audit verdict (NO-GO), the launch paths it checked, and the blocking vs non-blocking findings."
source_files:
  - docs/security/2026-06-15-launch-due-diligence-audit.md
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["launch", "due-diligence", "go-live", "readiness", "security", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

The go-live readiness gate is the pre-launch due-diligence audit: an adversarial sweep (82 agents, 131 raw findings, 21 confirmed) over the load-bearing launch paths — money path, auth/PAT, DSR/GDPR erasure, secrets posture, and data residency — that produces a single ship/no-ship verdict plus the prioritized must-fix set. Its discipline is that a launch promise (residency, proof-of-erasure, BYOK, rate-card caps) cannot be sold if the running data plane cannot deliver it, and that a control must fail CLOSED, not silently degrade. This concept captures WHAT the audit checked, the verdict it returned, and HOW it separates launch-blockers from deferrable debt. Sibling concepts launch/money-path and launch/signup-onboarding cover the Stripe checkout and Clerk provisioning paths in depth.

# Role

The audit answers one question — is CoreLink safe to launch self-serve to SMBs today — by confirming each headline launch path is actually functional end-to-end, not merely green under an IAD-only smoke test. It exists because several launch promises (data residency, proof-of-erasure, BYOK/Schrems-II, the rate card) are sold in customer-facing/legal docs while the implementing code path is broken, dead, or unenforced — and a green deploy gate actively hides some of these. The verdict converts a pile of findings into a go/no-go decision keyed on `blocksLaunch:true`.

# How it works

- The audit ran 82 agents over target `integ/post-login-backlog` (main + #269), producing 131 raw findings of which 22 CRITICAL/HIGH were adversarially verified and 21 confirmed via 2-vote refutation `docs/security/2026-06-15-launch-due-diligence-audit.md:3` `docs/security/2026-06-15-launch-due-diligence-audit.md:13`.
- The verdict is NO-GO until the two CRITICALs and the top tier of HIGHs are fixed; it converts to GO once the prioritized `blocksLaunch:true` items are merged and verified `docs/security/2026-06-15-launch-due-diligence-audit.md:5-11`.
- CRITICAL #1 (region fan-out): regional workers re-fan-out and 503 every non-IAD residency request, so the data-residency product is 100% non-functional for the exact tenants it serves while passing IAD-only smoke `docs/security/2026-06-15-launch-due-diligence-audit.md:111-114`.
- CRITICAL #2 (signup): a transient PAT-mint failure leaves a permanent PAT-less orphan tenant because the idempotency check short-circuits on tenant existence alone and never re-issues the PAT — a launch-day money-path/onboarding break `docs/security/2026-06-15-launch-due-diligence-audit.md:117-120`.
- Money-path checks surfaced Stripe Checkout idempotency replay (redirect-URL omission) and two live webhook handlers sharing the secret with a divergent tier map `docs/security/2026-06-15-launch-due-diligence-audit.md:153-155`, plus monthly request quota (`checkRequestQuota`) being a hard-coded no-op so advertised rate-card caps are unenforceable `docs/security/2026-06-15-launch-due-diligence-audit.md:99-103`.
- Auth/PAT checks found no signing-key rotation overlap (rotating `PAT_SIGNING_KEY` instantly invalidates every live PAT — a self-inflicted fleet-wide outage) `docs/security/2026-06-15-launch-due-diligence-audit.md:15-19`.
- DSR/GDPR checks found the erasure attestation signature is never persisted and has no serving endpoint, so the Art.17 proof-of-erasure artifact is a non-functional stub (erasure itself still completes, fail-open) `docs/security/2026-06-15-launch-due-diligence-audit.md:63-66`.
- Secrets-posture checks caught two compliance-bearing name/var drifts hidden behind green gates: signup-worker `ENVIRONMENT` is never bound, defeating the F9 erasure-salt fail-closed guard `docs/security/2026-06-15-launch-due-diligence-audit.md:123-127`, and the OCI route reads `CORELINK_OCI_TOKEN_KEY` while prod binds `HUGR_OCI_TOKEN_KEY`, so the OCI surface is dead in prod `docs/security/2026-06-15-launch-due-diligence-audit.md:135-139`.
- Residency checks found the data plane is provisioned US-only ({wnam,enam}); the DPA-onboarding doc nonetheless sells WEUR/SAM and BYOK/Schrems-II guarantees the launched plane cannot deliver, a GDPR Art.28 / contractual exposure `docs/security/2026-06-15-launch-due-diligence-audit.md:69-79`.
- DoS-surface checks found the data plane (CAS/AC/Bazel/Turbo/OCI) has NO request-rate limiting `docs/security/2026-06-15-launch-due-diligence-audit.md:93-97` — its sole backstop is a repo-invisible CF zone WAF rule that already self-DoS'd the SPA `docs/security/2026-06-15-launch-due-diligence-audit.md:105-109`.
- Findings are clustered into root causes — fail-open-where-the-rest-fails-closed, deploy-var/secret-name drift behind green gates, headline-feature serving logic broken end-to-end, provisioning non-atomicity, no data-plane rate limiting, and compliance artifacts sold-but-non-functional `docs/security/2026-06-15-launch-due-diligence-audit.md:141-150`.

# Invariants

- The honest verdict TODAY is NO-GO because the must-fix set still contains two open CRITICALs plus a payment/auth/compliance break — never blind-ship on a green gate `docs/security/2026-06-15-launch-due-diligence-audit.md:5-11`.
- Either CRITICAL alone is disqualifying: each breaks a load-bearing launch path with no workaround `docs/security/2026-06-15-launch-due-diligence-audit.md:7`.
- The two CRITICALs must be re-verified with the exact cases existing tests miss — a non-IAD regional-worker test `docs/security/2026-06-15-launch-due-diligence-audit.md:115` and an auth-key-present transient-mint-failure test `docs/security/2026-06-15-launch-due-diligence-audit.md:120`.
- Every isolation/compliance boundary must fail CLOSED (refuse 503/409/401) rather than serve under a public/shared/predictable fallback when a tenant id, TDK, region, or auth dependency is missing `docs/security/2026-06-15-launch-due-diligence-audit.md:142`.
- The deploy gate must verify the SAME secret/var names the code actually reads against the live CF secret list, so name drift reds CI instead of hiding behind green `docs/security/2026-06-15-launch-due-diligence-audit.md:143`.
- Never sell a control the running data plane cannot deliver — ship the compliance artifact or descope it from launch docs (WEUR/SAM/BYOK marked Phase-2) `docs/security/2026-06-15-launch-due-diligence-audit.md:147`.
- The must-fix set is bounded surgical work (bind one var, reconcile one secret name, add one termination guard, fail-closed one branch, fix one idempotency predicate), not deep-architecture rewrites `docs/security/2026-06-15-launch-due-diligence-audit.md:11`.

# Gotchas

- "Green CI" is actively misleading here: the IAD-only smoke passes while residency dies for every real target, and the OCI deploy gate is green because it checks the wrong (`HUGR_`) secret name `docs/security/2026-06-15-launch-due-diligence-audit.md:114,138`.
- The orphan-tenant break self-conceals: every Svix redelivery short-circuits at the idempotency check and 200s, so it never self-heals and emits no error signal `docs/security/2026-06-15-launch-due-diligence-audit.md:120`.
- Several findings are "not exploitable today but live the instant Phase-2 lands" — e.g. OCI residency leak and the degraded-prefix shared keyspace are gated only by the US-only / canonical-UUID precondition `docs/security/2026-06-15-launch-due-diligence-audit.md:24,57-60`.
- The erasure-salt fail-closed guard is inert by polarity: `environment && throw` short-circuits to falsy when `ENVIRONMENT` is undefined, so the predictable SHA-256 fallback runs silently in prod — invert to fail-closed-by-default `docs/security/2026-06-15-launch-due-diligence-audit.md:129-133`.
- The 66-item MEDIUM/LOW tail is real debt (CSP nonce, TOCTOU AC writes, in-memory limiters reset on cold-start, no email-verification gate) but does not block the go/no-go decision `docs/security/2026-06-15-launch-due-diligence-audit.md:152,184,197`.

# Citations

- `docs/security/2026-06-15-launch-due-diligence-audit.md:5-11` — VERDICT: NO-GO, the two disqualifying CRITICALs, and the GO-with-fixes conversion condition.
- `docs/security/2026-06-15-launch-due-diligence-audit.md:111-120` — the two CRITICALs: region fan-out termination and the PAT-less orphan tenant.
- `docs/security/2026-06-15-launch-due-diligence-audit.md:141-150` — root-cause clusters that organize the must-fix posture.
