---
id: 2026-05-27-secrets-matrix-drift-triage-seal
type: audit
date: 2026-05-27
status: SEALED
author: agent (Claude Sonnet 4.6)
scope: docs/internal/secrets-checklist.md + scripts/validate_secrets_matrix.py
---

# Secrets Matrix Drift Triage — SEAL

## Summary

`scripts/validate_secrets_matrix.py` reported **24 code_only ERRORs** —
env vars consumed by code with no corresponding matrix row. This audit
closes all 24 by either adding real matrix rows or adding non-secret
allowlist entries with rationale.

**Outcome:** `code_only: 24 → 0`. `validate_specs.py` unchanged (460 specs, 0 regressions).

---

## Triage Table

| # | Env Var | Decision | Rationale | Matrix Row / Allowlist Entry |
|---|---------|----------|-----------|------------------------------|
| 1 | `RESEND_API_KEY` | matrix-row | Real API credential; used by `apps/admin-ui` newsletter subscribe route AND `apps/analytics-worker` weekly digest cron. Revocation required on compromise. | Row 132 |
| 2 | `RESEND_NEWSLETTER_AUDIENCE_ID` | matrix-row | Resend audience ID; server-only by defense-in-depth (leaking without API key is harmless but kept server-side per code comments). Lifecycle tracking needed for SOC 2. | Row 133 |
| 3 | `CORELINK_TEST_TOKEN_CI` | matrix-row | GHA secret `secrets.CORELINK_TEST_TOKEN_CI` used by `.github/workflows/smoke-install.yml`; real auth credential for CI smoke test. Rotation cadence 90d per PAT policy. | Row 134 |
| 4 | `SENTRY_DSN` | matrix-row | Server-side Sentry DSN in `apps/admin-ui/sentry.server.config.ts` and `sentry.edge.config.ts`. Contains project ingest key; rotate-on-compromise per Sentry credential lifecycle policy. | Row 135 |
| 5 | `NEXT_PUBLIC_SENTRY_DSN` | matrix-row | Browser-side Sentry DSN in `apps/admin-ui/sentry.client.config.ts`. Intentionally public (browser-embedded) but tracked for credential lifecycle audit (SOC 2 CC6.1 coverage). | Row 136 |
| 6 | `SENTRY_DSN_DOCS` | matrix-row | Docs-site Sentry DSN injected at Cloudflare Pages build time in `apps/docs/docusaurus.config.ts`. Rotate-on-compromise; triggers Pages redeploy. | Row 137 |
| 7 | `INGEST_KEY` | matrix-row | HMAC secret key for trusted server-side analytics ingest (`X-Corelink-Ingest-Key` header) in `apps/analytics-worker/src/ingest.ts`. Optional in dev/test but required in prod; 90d rotation cadence. | Row 138 |
| 8 | `CF_PAGES_COMMIT_SHA` | allowlist | Cloudflare Pages build metadata — git SHA injected by Pages CI into the build environment. Consumed by `apps/docs/docusaurus.config.ts` as `SENTRY_DOCS_RELEASE` label. No credential material. | ALLOWLIST |
| 9 | `GIT_SHA` | allowlist | Fallback git SHA when `CF_PAGES_COMMIT_SHA` is absent (non-Pages CI). Consumed by `apps/docs/docusaurus.config.ts`. No credential material. | ALLOWLIST |
| 10 | `ALLOWED_ORIGINS` | allowlist | Wrangler [vars] public config — comma-separated CORS allow-list committed in plaintext in `apps/analytics-worker/wrangler.toml`. Consumed by `apps/analytics-worker/src/ingest.ts`. No credential material. | ALLOWLIST |
| 11 | `PLAUSIBLE_DOMAIN` | allowlist | Wrangler [vars] public config — Plausible domain label committed in plaintext in `apps/analytics-worker/wrangler.toml`. Consumed by `apps/analytics-worker/src/types.ts` Env binding. No credential material. | ALLOWLIST |
| 12 | `DIGEST_FROM` | allowlist | Wrangler [vars] public config — email From address (`noreply@humangr.com`) committed in plaintext in `apps/analytics-worker/wrangler.toml`. Consumed by `apps/analytics-worker/src/cron/weekly-email.ts`. No credential material. | ALLOWLIST |
| 13 | `DIGEST_RECIPIENT` | allowlist | Wrangler [vars] public config — weekly digest recipient email committed in plaintext in `apps/analytics-worker/wrangler.toml`. Consumed by `apps/analytics-worker/src/cron/weekly-email.ts`. No credential material (ops address, not auth). | ALLOWLIST |
| 14 | `RELEASE_ORIGIN` | allowlist | Wrangler [vars] public config — GitHub Releases download origin URL committed in plaintext in `apps/get-corelink-worker/wrangler.toml`. Consumed by `apps/get-corelink-worker/src/install.ts`. No credential material. | ALLOWLIST |
| 15 | `DEFAULT_API_ENDPOINT` | allowlist | Wrangler [vars] public config — default CoreLink API endpoint URL committed in plaintext in `apps/get-corelink-worker/wrangler.toml`. Consumed by `apps/get-corelink-worker/src/index.ts`. No credential material. | ALLOWLIST |
| 16 | `E2E_AUTH_STORAGE_STATE` | allowlist | E2E test-only config — file path for Playwright auth storage state. Consumed by `apps/admin-ui/e2e/signup-welcome.spec.ts`. Test harness path, no credential material. | ALLOWLIST |
| 17 | `E2E_DOCS_URL` | allowlist | E2E test-only config — docs site URL override for e2e tests. Consumed by `apps/admin-ui/e2e/signup-welcome.spec.ts`. Public URL, no credential material. | ALLOWLIST |
| 18 | `E2E_INSTALL_URL` | allowlist | E2E test-only config — install worker URL for e2e tests. Consumed by `apps/admin-ui/e2e/get-install-worker.spec.ts`. Public URL, no credential material. | ALLOWLIST |
| 19 | `NEXT_PUBLIC_SENTRY_ENVIRONMENT` | allowlist | Sentry build metadata — environment label injected at build/deploy time. Consumed by `apps/admin-ui/sentry.client.config.ts`. No credential material. | ALLOWLIST |
| 20 | `NEXT_PUBLIC_SENTRY_RELEASE` | allowlist | Sentry build metadata — release tag injected at build/deploy time. Consumed by `apps/admin-ui/sentry.client.config.ts` and server/edge configs. No credential material. | ALLOWLIST |
| 21 | `SENTRY_ENVIRONMENT` | allowlist | Sentry build metadata — server-side environment label. Consumed by `apps/admin-ui/sentry.server.config.ts` and `sentry.edge.config.ts`. No credential material. | ALLOWLIST |
| 22 | `SENTRY_RELEASE` | allowlist | Sentry build metadata — server-side release tag. Consumed by `apps/admin-ui/sentry.server.config.ts` and `sentry.edge.config.ts`. No credential material. | ALLOWLIST |
| 23 | `SENTRY_ORG` | allowlist | Sentry org identifier — non-secret config for `withSentryConfig`. Consumed by `apps/admin-ui/next.config.ts`. No credential material. | ALLOWLIST |
| 24 | `SENTRY_PROJECT` | allowlist | Sentry project name — non-secret config for `withSentryConfig`. Consumed by `apps/admin-ui/next.config.ts`. No credential material. | ALLOWLIST |

**BONUS (detected during triage):**

| # | Env Var | Decision | Rationale |
|---|---------|----------|-----------|
| 25 | `NEXT_PUBLIC_ANALYTICS_ENDPOINT` | allowlist | Public URL override for analytics ingest endpoint in `apps/admin-ui/src/lib/analytics.ts`. Has hardcoded default (`https://corelink-analytics.humangr.com/v1/event`); no credential material. | ALLOWLIST |

---

## Matrix Rows Added (7)

| Row # | Env Var | Storage | Rotation Cadence | Owner |
|-------|---------|---------|-----------------|-------|
| 132 | `RESEND_API_KEY` | vercel-env + cf-wrangler | 180d | DevOps |
| 133 | `RESEND_NEWSLETTER_AUDIENCE_ID` | vercel-env | rotate-on-compromise | DevOps |
| 134 | `CORELINK_TEST_TOKEN_CI` | gha-secret | 90d | DevOps |
| 135 | `SENTRY_DSN` | vercel-env | rotate-on-compromise | DevOps |
| 136 | `NEXT_PUBLIC_SENTRY_DSN` | vercel-env | rotate-on-compromise | DevOps |
| 137 | `SENTRY_DSN_DOCS` | vercel-env (CF Pages env) | rotate-on-compromise | DevOps |
| 138 | `INGEST_KEY` | cf-wrangler | 90d | DevOps |

## Allowlist Entries Added (17 + 1 bonus)

Added to `ALLOWLIST_REGEX` in `scripts/validate_secrets_matrix.py`:

```
CF_PAGES_COMMIT_SHA, GIT_SHA,
ALLOWED_ORIGINS, PLAUSIBLE_DOMAIN, DIGEST_FROM, DIGEST_RECIPIENT,
RELEASE_ORIGIN, DEFAULT_API_ENDPOINT,
E2E_AUTH_STORAGE_STATE, E2E_DOCS_URL, E2E_INSTALL_URL,
NEXT_PUBLIC_SENTRY_ENVIRONMENT, NEXT_PUBLIC_SENTRY_RELEASE,
SENTRY_ENVIRONMENT, SENTRY_RELEASE, SENTRY_ORG, SENTRY_PROJECT,
NEXT_PUBLIC_ANALYTICS_ENDPOINT
```

---

## Acceptance Gate Results

```
validate_secrets_matrix: matrix=139 code=117 in_both=117 matrix_only=22 code_only=0
validate_specs.py: ✅ Todos validados: 449 com schema completo, 11 com YAML only (460 total).
```

- code_only drift: **24 → 0** ✓
- matrix rows: 131 → 138 (+7) ✓
- allowlist entries: +18 (17 wave-33 drift + 1 bonus NEXT_PUBLIC_ANALYTICS_ENDPOINT) ✓
- validate_specs.py: 0 regressions ✓
- matrix_only soft-warns (22): all intentional (forward-looking or scanner-gap entries per existing Forward-looking secrets table) ✓

---

## Notes on INGEST_KEY

`INGEST_KEY` is declared as optional (`INGEST_KEY?: string`) in
`apps/analytics-worker/src/types.ts`. The wrangler scanner only picks up
required bindings from `[vars]` sections; optional secrets set via
`wrangler secret put` are invisible to the static scanner. The new matrix
row 138 documents it correctly; the `matrix_only` soft-warn for `INGEST_KEY`
is expected and intentional (scanner-gap documented in Forward-looking secrets
table pattern).
