---
id: AUDIT-SECRETS-COVERAGE-BASELINE-2026-05-15
type: audit
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-05-15
reviewers: [Claude Opus 4.7 — secrets drift validator agent]
scope: [secrets-checklist, env-var-references, soc2-cc6.1, supply-chain]
branch: wt/r-prep-secrets-drift
head: pending
tags: [audit, secrets, drift, soc2, cc6.1, baseline, r-prep]
---

# Secrets Coverage Baseline — 2026-05-15

> First-run snapshot of `scripts/validate_secrets_matrix.py` against `main`
> at branch creation of `wt/r-prep-secrets-drift`. Establishes the
> reference point for the daily drift gate
> (`.github/workflows/secrets-drift.yml`) and the SOC 2 CC6.1 evidence
> trail.

---

## 1. Summary

| Dimension | Count | Source |
|---|---:|---|
| Logical rows in matrix | **89** | `docs/internal/secrets-checklist.md` footer |
| Distinct env-var names in matrix (4th column) | **90** | Row 26 lists two aliases (`AWS_KMS_TEST_KEY_ARN` / `AWS_TEST_KEY_ARN`) |
| Distinct env-var names referenced in code (post-allowlist) | **89** | Rust + TS/JS + GHA + wrangler.toml |
| **In both** (clean intersection) | **70** | Documented + consumed |
| **Matrix-only** (forward-looking or stale) | **20** | Soft-warn; quarterly reconciliation |
| **Code-only** (real drift) | **19** | **Must close before GA** |
| Allowlist-skipped env vars (not credentials) | **21** | OS / proptest / build-metadata / test knobs |

**Health:** baseline is **NOT clean** — 19 code-side env vars are referenced
without a matrix row. This is expected at R-PREP stage (matrix was sealed
2026-05-14, drift accumulated in the same day across in-flight wt branches).
None of the code-only entries are net-new vendor credentials; the bulk are
load-test scaffolding (`K6_*`), staging-only Vault auth modes
(`VAULT_AWS_ROLE`, `VAULT_K8S_ROLE`, `VAULT_TOKEN`), and one production
secret already shipped via Drata onboarding (`DRATA_API_KEY`,
`DRATA_API_BASE_URL`).

---

## 2. Code-only drift (real — must close before GA)

| # | Env var | First-hit consumer | Category | Disposition |
|---|---|---|---|---|
| 1 | `AZURE_FEDERATED_TOKEN_FILE` | `corelink-byok-azure` (OIDC federation) | BYOK auth | Add row (Azure vendor; pairs with #30..#32) |
| 2 | `AZURE_TEST_KEY_RESOURCE` | `corelink-byok-azure` (e2e tests) | BYOK test config | Add row (test-only — pairs with #26 pattern) |
| 3 | `AZURE_TEST_REGION` | `corelink-byok-azure` (e2e tests) | BYOK test config | Add row (test-only) |
| 4 | `DRATA_API_BASE_URL` | `corelink-drata-sync` | Compliance vendor | Add row (Drata onboarded 2026-05) |
| 5 | `DRATA_API_KEY` | `corelink-drata-sync` | Compliance vendor | Add row (Drata onboarded 2026-05) |
| 6 | `HTTP_PORT` | `apps/server` (HTTP listener) | App config | Add to allowlist (port literal, paired with `PORT` #76) |
| 7 | `K6_PROMETHEUS_RW_SERVER_URL` | k6 load-test scripts | Load-test infra | Add to allowlist (test infra; pattern matches `K6_*`) |
| 8 | `K6_STAGING_BYOK_CMK_ID` | k6 load-test | Load-test infra | Add to allowlist |
| 9 | `K6_STAGING_MFA_STUB` | k6 load-test | Load-test infra | Add to allowlist |
| 10 | `K6_STAGING_PAT` | k6 load-test | Load-test infra | Add row (real PAT — pairs with #65) |
| 11 | `K6_STAGING_STRIPE_WHSEC` | k6 load-test | Load-test infra | Add row (real Stripe webhook secret — pairs with #3) |
| 12 | `K6_TARGET_HOST` | k6 load-test | Load-test infra | Add to allowlist (URL, not secret) |
| 13 | `PAGERDUTY_COMPLIANCE_KEY` | `corelink-compliance-pager` | Vendor key | Add row (third PD integration; pairs with #11, #12) |
| 14 | `STRIPE_PRICE_ID_STARTER` | `corelink-stripe-real` (price catalog) | Stripe config | Add to allowlist (price ID is public-ish; not a secret) |
| 15 | `VAULT_AWS_ROLE` | `corelink-byok-vault` (AWS auth mode) | Vault auth | Add row (pairs with #33..#35) |
| 16 | `VAULT_K8S_ROLE` | `corelink-byok-vault` (K8s auth mode) | Vault auth | Add row |
| 17 | `VAULT_K8S_SERVICE_ACCOUNT_TOKEN` | `corelink-byok-vault` | Vault auth | Add row (customer-side tier) |
| 18 | `VAULT_SKIP_VERIFY` | `corelink-byok-vault` (dev-only flag) | Test flag | Add row (mirrors `CORELINK_BYOK_VAULT_MOCK` #36 pattern) |
| 19 | `VAULT_TOKEN` | `corelink-byok-vault` (direct-token auth) | Vault auth | Add row (customer-side tier; pairs with #33..#35) |

**Disposition split:** 13 add-row, 6 add-to-allowlist.

---

## 3. Matrix-only (soft-warn — forward-looking or unreached consumers)

| # | Env var | Likely status |
|---|---|---|
| 1 | `AWS_ACCESS_KEY_ID` | Consumed only via GHA `${{ secrets.X }}`; matrix row #23 documents this — drift is in the regex (consumer is workflow YAML literal, not `process.env`/`env::var`) |
| 2 | `AWS_SECRET_ACCESS_KEY` | Same as #1 |
| 3 | `AWS_USE_FIPS_ENDPOINT` | Forward-looking (FIPS toggle pending GAP-02 close) |
| 4 | `CLERK_AUDIENCE` | Forward-looking (multi-tenant JWT validation pending) |
| 5 | `CLERK_JWKS_URL` | Optional override; default derived from publishable |
| 6 | `CLERK_JWT_ISSUER` | Optional override |
| 7 | `CLERK_PUBLISHABLE_KEY` | Server-side variant; admin-ui uses `NEXT_PUBLIC_*` |
| 8 | `COOKIEBOT_DOMAIN_GROUP_ID` | Consumed in admin-ui bundle (build-time inlined; not visible to grep) |
| 9 | `DT_MOCK_INJECTION_ENABLED` | Test flag (`#[cfg(test)]` only) |
| 10 | `HUBSPOT_PRIVATE_APP_TOKEN` | Enterprise-inquiry path; crate not yet wired |
| 11 | `PAGERDUTY_SYNTHETIC_ROUTING_KEY` | Synthetic-drill crate pending merge |
| 12..17 | `SLACK_WEBHOOK_URL_*` (6 variants) | Slack-real crate uses a single `SLACK_WEBHOOK_URL` env at runtime and routes via channel-id; per-channel webhooks are GHA-secrets-only |
| 18 | `STATUSPAGE_API_KEY` | Status-sync path pending |
| 19 | `TWILIO_ACCOUNT_SID` | SMS notification path pending |
| 20 | `TWILIO_AUTH_TOKEN` | Same as #19 |

**Action:** none required at GA gate (all rows are forward-looking with
documented owners). Quarterly access review (next: 2026-08-15) will
reconcile any rows still without a consumer.

---

## 4. Allowlist coverage

The Python validator's `ALLOWLIST_REGEX` (mirrored from
`secrets-checklist-verify.sh`) filtered **21** env vars from the
code-side scan that are not credentials:

```
CARGO_FEATURE_HOST_SERVER, CI, DOCS_BASE_URL, DT_API_KEY_TEST_03,
DT_MOCK_INJECTION_ENABLED, E2E_BASE_URL, ENVIRONMENT, GCP_TEST_KEY_RESOURCE,
GCP_TEST_REGION, GITHUB_TOKEN, HOME, LH_BASE_URL, LH_START_COMMAND,
NEXT_PUBLIC_E2E_TEST_MODE, NODE_ENV, PROJECTS, PROPTEST_ARGON_NIGHTLY,
PROPTEST_CASES, SKIP_WEBSERVER, SOURCE_DATE_EPOCH, USERPROFILE
```

These are OS-provided, CI-injected, proptest knobs, or local-dev test
flags. They are NOT credentials and intentionally do not have matrix
rows.

---

## 5. Cross-check with bash deploy-gate verifier

Running `bash scripts/secrets-checklist-verify.sh` against the same
worktree state produces the same `code_only` set **plus** two false
positives the Python validator correctly rejects:

| Bash false positive | Why bash flagged | Why Python doesn't |
|---|---|---|
| `X` | Bash regex `secrets\.X` matched a commented-out workflow line `# VAULT_TOKEN: ${{ secrets.STAGING_VAULT_TOKEN }}` interpreted by the line-by-line grep as `secrets.X` from `STAGING_VAULT_TOKEN` |  Python uses a stricter anchored regex requiring `${{ secrets.NAME }}` exact match |
| (same) | — | — |

The Python validator is therefore the **more authoritative** source for
the daily drift metric; the bash verifier remains the deploy-gate (it
errs on the side of fail-closed, which is correct for production cuts).

---

## 6. SLA + ownership

| Action | Owner | Deadline |
|---|---|---|
| Close all 19 `code_only` drift entries (matrix rows or allowlist) | SRE Lead + Security Lead | Before GA-gate sign-off (T-7 of R-8 launch) |
| Quarterly reconciliation of 20 `matrix_only` rows | SRE Lead | 2026-08-15 |
| Allowlist drift parity (Python + bash) | DevOps | Continuous (CI-enforced) |

---

## 7. Cross-references

- `docs/internal/secrets-checklist.md` — canonical 89-row matrix
- `scripts/validate_secrets_matrix.py` — Python validator
- `scripts/secrets-checklist-verify.sh` — bash deploy-gate verifier
- `.github/workflows/secrets-drift.yml` — daily cron + PR gate
- `.github/workflows/cf-deploy-prod.yml` — production deploy gate
- `specs/_runbooks/RB-SECRETS-DRIFT.md` — triage runbook
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §CC6.1 — control mapping
- `ROADMAP-TO-GA.md` §9 — Human Track credentials acquisition
