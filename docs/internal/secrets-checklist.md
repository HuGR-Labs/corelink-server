# Production Secrets Checklist (WI-R2-14)

> **Last sealed:** 2026-05-14
> **Owner of this document:** SRE Lead (co-owned with Security Lead)
> **Validation (deploy gate, fail-closed):** `scripts/secrets-checklist-verify.sh`
> via `.github/workflows/cf-deploy-prod.yml`. Drift between this matrix and
> the codebase fails the deploy gate.
> **Validation (daily cron + PR gate, structured JSON):**
> `scripts/validate_secrets_matrix.py` via
> `.github/workflows/secrets-drift.yml` (04:00 UTC daily + PRs touching
> `Cargo.toml`/`wrangler.toml`/`.github/workflows/`). Produces a
> `secrets-drift-report.json` artifact (90d retention) for SOC 2 CC6.1
> evidence sampling.
> **Triage on drift:** `specs/_runbooks/RB-SECRETS-DRIFT.md`.
> **Baseline snapshot:** `specs/_audits/2026-05-15-secrets-coverage-baseline.md`.

This is the **single source of truth** for every external secret that CoreLink
production requires. Every row maps a logical secret → the canonical env var
consumed by the runtime → the vendor → acquisition path → rotation cadence →
rotation owner → compromise response → storage location.

**Hard rules:**

- **No real secret values in this file.** Placeholders only. Real values live
  exclusively in Cloudflare Workers (`wrangler secret put`), GitHub Actions
  repository/environment secrets, or per-tenant customer-side stores.
- **Every row must have a non-empty `Rotation cadence` cell.** This is a SOC 2 /
  ISO 27001 audit hook.
- **Every `env::var("X")` call in code must have a row here.** The verifier
  script enforces this.
- **Every row here must have at least one consumer in code or workflows.** The
  verifier script catches stale rows.

## Storage tiers

| Tier | Location | Used for |
|---|---|---|
| `cf-wrangler` | Cloudflare Workers secrets (`wrangler secret put X --env prod`) | All runtime secrets consumed by `apps/server` or the Worker control plane |
| `gha-secret` | GitHub Actions repository / environment secret | Build-time signing keys, release pipeline credentials |
| `vercel-env` | Vercel (admin-ui) environment variables | Browser-exposed `NEXT_PUBLIC_*` keys + admin-ui server-side keys |
| `customer-side` | Per-tenant configuration (customer Vault, BYOK customer console) | Customer-managed keys (Vault AppRole, BYOK KMS ARNs) |
| `aws-sm-mirror` | AWS Secrets Manager (mirror copy) | Defense-in-depth mirror of AWS BYOK credentials for break-glass |

## Rotation owners

| Owner | Scope |
|---|---|
| `SRE Lead` | Runtime infrastructure secrets (Stripe, PagerDuty, Slack alerts) |
| `Security Lead` | All KMS/BYOK credentials (AWS, GCP, Azure, Vault) |
| `DevOps` | Auth provider (Clerk), notification transports (SendGrid, Twilio), Statuspage, Cookiebot |
| `Sales Ops` | HubSpot private app token (enterprise inquiry intake) |
| `Release Manager` | All code-signing keys (GPG, Apple, Windows EV cert) |
| `Customer` | Per-tenant Vault AppRole credentials (we never see these) |

## Secrets matrix

| # | Secret | Env var name | Consumer crate(s) / workflow | Vendor | Vendor URL | Acquisition steps | Rotation cadence | Rotation owner | Compromise procedure | Stored at |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Stripe live secret key | `STRIPE_SECRET_KEY` | corelink-stripe-real, apps/server | Stripe | https://dashboard.stripe.com/apikeys | Atlas application → Live mode toggle → "Reveal live key" | 90d | SRE Lead | Revoke at dashboard; rotate immediately; replay last 24h via corelink-billing-replay | cf-wrangler |
| 2 | Stripe test secret key | `STRIPE_SECRET_KEY_TEST` | corelink-stripe-real (test path) | Stripe | https://dashboard.stripe.com/test/apikeys | Test mode toggle → "Reveal test key" | 180d | SRE Lead | Revoke + recreate | cf-wrangler (staging only) |
| 3 | Stripe webhook signing secret | `STRIPE_WEBHOOK_SECRET` | apps/server (webhook handler) | Stripe | https://dashboard.stripe.com/webhooks | Create webhook endpoint at dashboard → copy signing secret | 90d | SRE Lead | Recreate endpoint; re-verify last-24h webhooks | cf-wrangler |
| 4 | Clerk publishable key | `CLERK_PUBLISHABLE_KEY` | corelink-clerk | Clerk | https://dashboard.clerk.com | Create application → API keys → publishable | 365d | DevOps | Rotate application keys (forces re-login) | cf-wrangler |
| 5 | Clerk publishable key (browser) | `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | apps/admin-ui | Clerk | https://dashboard.clerk.com | Same as #4; mirrored for the Next.js client bundle | 365d | DevOps | Same as #4 | vercel-env |
| 6 | Clerk secret key | `CLERK_SECRET_KEY` | corelink-clerk, apps/admin-ui (server) | Clerk | https://dashboard.clerk.com | Create application → API keys → secret | 90d | DevOps | Rotate keys; invalidate active sessions | cf-wrangler + vercel-env |
| 7 | Clerk JWKS URL override | `CLERK_JWKS_URL` | corelink-clerk | Clerk | https://dashboard.clerk.com | Optional override; default derived from publishable | rotate-on-compromise | DevOps | N/A (URL, not a secret) | cf-wrangler (optional) |
| 8 | Clerk JWT issuer | `CLERK_JWT_ISSUER` | corelink-clerk | Clerk | https://dashboard.clerk.com | Read from JWT template config | rotate-on-compromise | DevOps | N/A (config string) | cf-wrangler (optional) |
| 9 | Clerk audience | `CLERK_AUDIENCE` | corelink-clerk | Clerk | https://dashboard.clerk.com | JWT template `aud` claim | rotate-on-compromise | DevOps | N/A (config string) | cf-wrangler |
| 10 | Clerk JWT template name (PAT) | `CLERK_JWT_TEMPLATE_PAT` | apps/admin-ui | Clerk | https://dashboard.clerk.com | Create JWT template named `pat`; record name | rotate-on-compromise | DevOps | N/A (config string) | vercel-env |
| 11 | PagerDuty routing key (prod alerts) | `PAGERDUTY_ROUTING_KEY` | corelink-oncall | PagerDuty | https://pagerduty.com | Service → Integrations → Events API v2 → copy routing key | 365d | SRE Lead | Recreate integration; redeploy | cf-wrangler |
| 12 | PagerDuty routing key (synthetic drill) | `PAGERDUTY_SYNTHETIC_ROUTING_KEY` | corelink-synthetic-pager | PagerDuty | https://pagerduty.com | Separate `synthetic-drill` service → Events API v2 | 365d | SRE Lead | Recreate integration | cf-wrangler |
| 13 | Slack webhook — alerts SEV-1 | `SLACK_WEBHOOK_URL_ALERTS_SEV1` | corelink-slack-real | Slack | https://api.slack.com/messaging/webhooks | Create Slack app → Incoming Webhooks → enable → add to workspace → pick #alerts-sev1 | 365d | SRE Lead | Revoke at api.slack.com/apps; recreate webhook | cf-wrangler |
| 14 | Slack webhook — alerts SEV-2 | `SLACK_WEBHOOK_URL_ALERTS_SEV2` | corelink-slack-real | Slack | https://api.slack.com/messaging/webhooks | Same flow → channel `#alerts-sev2` | 365d | SRE Lead | Same as #13 | cf-wrangler |
| 15 | Slack webhook — enterprise inquiries | `SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES` | corelink-slack-real | Slack | https://api.slack.com/messaging/webhooks | Same flow → channel `#enterprise-inquiries` | 365d | SRE Lead | Same as #13 | cf-wrangler |
| 16 | Slack webhook — breach notifications | `SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS` | corelink-slack-real | Slack | https://api.slack.com/messaging/webhooks | Same flow → channel `#breach-notifications` (restricted membership) | 365d | SRE Lead | Same as #13; notify security on rotation | cf-wrangler |
| 17 | Slack webhook — oncall handoff | `SLACK_WEBHOOK_URL_ONCALL_HANDOFF` | corelink-slack-real | Slack | https://api.slack.com/messaging/webhooks | Same flow → channel `#oncall-handoff` | 365d | SRE Lead | Same as #13 | cf-wrangler |
| 18 | Slack webhook — lighthouse customers | `SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS` | corelink-slack-real | Slack | https://api.slack.com/messaging/webhooks | Same flow → channel `#lighthouse-customers` | 365d | SRE Lead | Same as #13 | cf-wrangler |
| 19 | Slack security webhook (GHA) | `SLACK_SECURITY_WEBHOOK` | .github/workflows/pentest-findings-sync.yml | Slack | https://api.slack.com/messaging/webhooks | Same flow → channel `#security-findings` | 365d | SRE Lead | Same as #13 | gha-secret |
| 20 | Slack bot token | `SLACK_BOT_TOKEN` | (Slack interactive surfaces, GHA) | Slack | https://api.slack.com/apps | App → OAuth & Permissions → "Install to Workspace" → copy `xoxb-...` | 365d | SRE Lead | Revoke at app config; reinstall app | gha-secret |
| 21 | Slack channel ID (alerts) | `SLACK_CHANNEL_ID` | (Slack notify helpers) | Slack | (Slack client UI) | Right-click channel → "Copy link" → extract trailing ID | rotate-on-compromise | SRE Lead | N/A (ID, not a secret; rotate only on channel rename) | gha-secret |
| 22 | HubSpot private app token | `HUBSPOT_PRIVATE_APP_TOKEN` | corelink-enterprise-inquiry | HubSpot | https://app.hubspot.com/private-apps | Settings → Integrations → Private Apps → create app with `crm.objects.contacts.write` + `crm.objects.deals.write` | 180d | Sales Ops | Revoke at HubSpot; recreate; replay last 24h inquiries | cf-wrangler |
| 23 | AWS BYOK staging access key ID | `AWS_ACCESS_KEY_ID` | corelink-byok-aws (e2e tests), .github/workflows/byok_matrix_weekly.yml | AWS | https://console.aws.amazon.com/iam | IAM → user `corelink-byok-staging` → create access key | 90d | Security Lead | IAM → deactivate + delete key; create new; redeploy | gha-secret + aws-sm-mirror |
| 24 | AWS BYOK staging secret access key | `AWS_SECRET_ACCESS_KEY` | corelink-byok-aws (e2e tests) | AWS | https://console.aws.amazon.com/iam | Returned only at creation time of #23 | 90d | Security Lead | Same as #23 | gha-secret + aws-sm-mirror |
| 25 | AWS region | `AWS_REGION` | corelink-byok-aws | AWS | (deploy config) | Region literal e.g. `us-east-1` | rotate-on-compromise | Security Lead | N/A (config, not secret) | cf-wrangler |
| 26 | AWS BYOK test key ARN | `AWS_KMS_TEST_KEY_ARN` / `AWS_TEST_KEY_ARN` | corelink-byok-aws (e2e), corelink-byok-matrix-test | AWS | https://console.aws.amazon.com/kms | KMS → create symmetric CMK in staging account; copy ARN | rotate-on-compromise | Security Lead | Schedule deletion + create replacement | gha-secret |
| 27 | AWS FIPS endpoint toggle | `AWS_USE_FIPS_ENDPOINT` | corelink-byok-aws | AWS | (config) | Set `true` to force FIPS 140-3 endpoint | rotate-on-compromise | Security Lead | N/A (config flag) | cf-wrangler |
| 28 | GCP service account credentials (file path) | `GOOGLE_APPLICATION_CREDENTIALS` | corelink-byok-gcp | GCP | https://console.cloud.google.com/iam-admin/serviceaccounts | Create SA `corelink-byok-staging@PROJECT.iam.gserviceaccount.com` → create JSON key → write to ephemeral file path | 90d | Security Lead | Disable key in IAM; issue new key; delete old after grace | cf-wrangler (path) + gha-secret (raw JSON) |
| 29 | GCP KMS staging credentials JSON | `GCP_KMS_STAGING_CREDENTIALS` | .github/workflows/byok_matrix_weekly.yml | GCP | https://console.cloud.google.com/iam-admin/serviceaccounts | Raw JSON contents of #28 (base64-encoded for GHA secret) | 90d | Security Lead | Same as #28 | gha-secret |
| 30 | Azure tenant ID | `AZURE_TENANT_ID` | corelink-byok-azure, .github/workflows/byok_matrix_weekly.yml | Azure | https://portal.azure.com/AppRegistrations | App registration → Overview → Directory (tenant) ID | rotate-on-compromise | Security Lead | N/A (tenant ID is not a secret but pair leakage is high-impact) | cf-wrangler + gha-secret |
| 31 | Azure client ID | `AZURE_CLIENT_ID` | corelink-byok-azure | Azure | https://portal.azure.com/AppRegistrations | App registration → Overview → Application (client) ID | rotate-on-compromise | Security Lead | Delete + re-register application | cf-wrangler + gha-secret |
| 32 | Azure client secret | `AZURE_CLIENT_SECRET` | corelink-byok-azure | Azure | https://portal.azure.com/AppRegistrations | App registration → Certificates & secrets → New client secret | 90d | Security Lead | Delete secret at Azure; new client secret; redeploy | cf-wrangler + gha-secret |
| 33 | Vault AppRole role ID | `VAULT_APPROLE_ROLE_ID` | corelink-byok-vault | (customer Vault) | (customer-side) | Customer provisions an AppRole and shares role-id | per customer | Customer | Customer rotates role-id via `vault write auth/approle/role/...` | customer-side |
| 34 | Vault AppRole secret ID | `VAULT_APPROLE_SECRET_ID` | corelink-byok-vault | (customer Vault) | (customer-side) | Customer issues a secret-id with bounded TTL | per customer (≤ 30d default) | Customer | Customer revokes secret-id; we re-bind | customer-side |
| 35 | Vault staging address | `VAULT_ADDR` | .github/workflows/byok_matrix_weekly.yml | (customer Vault) | (config) | Vault cluster URL | rotate-on-compromise | Security Lead | N/A (URL) | gha-secret |
| 36 | Vault mock flag (test only) | `CORELINK_BYOK_VAULT_MOCK` | corelink-byok-vault | (internal) | n/a | Set `1` in dev/test only | rotate-on-compromise | Security Lead | N/A (test flag; production HARD-rejects this) | dev only |
| 37 | GCP mock flag (test only) | `CORELINK_BYOK_GCP_MOCK` | corelink-byok-gcp | (internal) | n/a | Set `1` in dev/test only | rotate-on-compromise | Security Lead | N/A | dev only |
| 38 | Azure mock flag (test only) | `CORELINK_BYOK_AZURE_MOCK` | corelink-byok-azure | (internal) | n/a | Set `1` in dev/test only | rotate-on-compromise | Security Lead | N/A | dev only |
| 39 | SendGrid API key | `SENDGRID_API_KEY` | .github/workflows/pentest-findings-sync.yml (notification path) | SendGrid (Twilio) | https://app.sendgrid.com | Settings → API Keys → Create API Key with `mail.send` scope | 180d | DevOps | Revoke at SendGrid; replace; replay queued mail | cf-wrangler + gha-secret |
| 40 | Twilio account SID | `TWILIO_ACCOUNT_SID` | (SMS notification path) | Twilio | https://console.twilio.com | Console homepage → Account Info → Account SID | rotate-on-compromise | DevOps | N/A directly (paired with auth token) | cf-wrangler |
| 41 | Twilio auth token | `TWILIO_AUTH_TOKEN` | (SMS notification path) | Twilio | https://console.twilio.com | Console homepage → Account Info → Auth Token → "View" | 180d | DevOps | Rotate at console; redeploy | cf-wrangler |
| 42 | Statuspage API key | `STATUSPAGE_API_KEY` | (status-sync path) | Atlassian Statuspage | https://manage.statuspage.io | Account → API Keys → Create API Key | 365d | DevOps | Revoke + recreate | cf-wrangler |
| 43 | Cookiebot domain group ID | `COOKIEBOT_DOMAIN_GROUP_ID` | apps/admin-ui | Cookiebot | https://manage.cookiebot.com | Create domain group → copy ID | 365d | DevOps | N/A (ID, not a secret; rotate only on domain group recreation) | vercel-env |
| 44 | Dependency-Track API URL | `DT_API_URL` | corelink-dt-reconcile, corelink-dt-cli | Dependency-Track (self-hosted) | (internal) | Self-hosted DT instance URL | rotate-on-compromise | DevOps | N/A (URL) | cf-wrangler |
| 45 | Dependency-Track API key | `DT_API_KEY` | corelink-dt-reconcile, .github/workflows/sbom.yml | Dependency-Track (self-hosted) | (DT admin UI) | Administration → Access Management → Teams → API keys | 180d | DevOps | Regenerate at DT admin UI | gha-secret + cf-wrangler |
| 46 | Dependency-Track webhook secret | `DT_WEBHOOK_SECRET` | corelink-dt-reconcile, corelink-dt-cli | Dependency-Track (self-hosted) | (DT admin UI) | Configure webhook signing secret | 180d | DevOps | Update at DT + redeploy consumers | cf-wrangler |
| 47 | Dependency-Track project UUID | `DT_PROJECT_UUID` | corelink-dt-cli | Dependency-Track (self-hosted) | (DT admin UI) | Project → copy UUID | rotate-on-compromise | DevOps | N/A (UUID) | gha-secret |
| 48 | Dependency-Track mock injection flag | `DT_MOCK_INJECTION_ENABLED` | corelink-dt-cli | (internal) | n/a | Test-only flag | rotate-on-compromise | DevOps | N/A | dev only |
| 49 | Cloudflare account ID | `CF_ACCOUNT_ID` | .github/workflows/ac-bucket-acl-cron.yml | Cloudflare | https://dash.cloudflare.com | Right sidebar → Account ID | rotate-on-compromise | SRE Lead | N/A (account ID, not secret) | gha-secret |
| 50 | Cloudflare API token (deploy) | `CF_API_TOKEN` | .github/workflows/ac-bucket-acl-cron.yml, cf-deploy-prod.yml | Cloudflare | https://dash.cloudflare.com/profile/api-tokens | Create token with `Workers Scripts:Edit` + `Workers R2 Storage:Edit` + `Workers Secrets:Edit` | 90d | SRE Lead | Revoke at CF dashboard; create new; redeploy | gha-secret |
| 51 | Cloudflare Access client ID | `CF_CLIENT_ID` | .github/workflows/terraform-drift.yml | Cloudflare | https://dash.cloudflare.com/access | Access → Service Tokens → create | 365d | SRE Lead | Revoke + recreate | gha-secret |
| 52 | Cloudflare Access client secret | `CF_CLIENT_SECRET` | .github/workflows/terraform-drift.yml | Cloudflare | https://dash.cloudflare.com/access | Returned only at creation time of #51 | 365d | SRE Lead | Same as #51 | gha-secret |
| 53 | CF deploy verifier URL | `CF_DEPLOY_VERIFIER_URL` | .github/workflows/cosign-sign.yml | (internal) | (deploy config) | Set to corelink-deploy-verifier endpoint | rotate-on-compromise | SRE Lead | N/A (URL) | gha-secret |
| 54 | Deploy webhook signing secret | `DEPLOY_WEBHOOK_SECRET` | .github/workflows/cosign-sign.yml, corelink-deploy-verifier | (internal) | n/a | Generated via `openssl rand -hex 32`; mirrored to verifier service | 90d | SRE Lead | Generate new; update verifier + GHA in lockstep | gha-secret + cf-wrangler |
| 55 | GPG private key (release signing) | `GPG_PRIVATE_KEY` | .github/workflows/sign-linux.yml | (self-managed) | n/a | `gpg --full-generate-key` (RSA 4096); ASCII-armored export | 730d | Release Manager | Generate new keypair; publish new pubkey at `corelink.dev/.well-known/gpg-pubkey.asc`; revoke old via revocation cert | gha-secret |
| 56 | GPG private key passphrase | `GPG_PRIVATE_KEY_PASS` | .github/workflows/sign-linux.yml | (self-managed) | n/a | Strong passphrase set at key generation | 730d (with #55) | Release Manager | Same as #55 | gha-secret |
| 57 | GPG long key ID | `GPG_KEY_ID` | .github/workflows/sign-linux.yml | (self-managed) | n/a | `gpg --list-secret-keys --keyid-format=long` | 730d (with #55) | Release Manager | Same as #55 | gha-secret |
| 58 | Apple Developer ID cert (.p12 base64) | `APPLE_DEVELOPER_ID` | .github/workflows/notarize-macos.yml | Apple | https://developer.apple.com/account/resources/certificates | Enroll Apple Developer Program → request Developer ID Application cert → export `.p12` → `base64` | 365d (cert validity) | Release Manager | Revoke at developer portal; request new cert; re-sign next release | gha-secret |
| 59 | Apple Developer ID cert password | `APPLE_DEVELOPER_ID_PASSWORD` | .github/workflows/notarize-macos.yml | Apple | https://developer.apple.com | Password set when exporting `.p12` | 365d (with #58) | Release Manager | Same as #58 | gha-secret |
| 60 | Apple notarization username (Apple ID) | `APPLE_NOTARIZATION_USERNAME` | .github/workflows/notarize-macos.yml | Apple | https://appleid.apple.com | Account associated with developer program | 365d | Release Manager | Rotate Apple ID password + app-specific password | gha-secret |
| 61 | Apple notarization password (app-specific) | `APPLE_NOTARIZATION_PASSWORD` | .github/workflows/notarize-macos.yml | Apple | https://appleid.apple.com | Manage → App-Specific Passwords → generate | 365d | Release Manager | Revoke at appleid.apple.com; generate new | gha-secret |
| 62 | Apple Team ID | `APPLE_TEAM_ID` | .github/workflows/notarize-macos.yml | Apple | https://developer.apple.com/account | Membership → Team ID | rotate-on-compromise | Release Manager | N/A (Team ID, not secret) | gha-secret |
| 63 | Windows EV code-signing cert (.pfx base64) | `WINDOWS_CODE_SIGNING_CERT` | .github/workflows/sign-windows.yml | DigiCert | https://www.digicert.com | Order EV code-signing certificate; export `.pfx` from hardware token; base64 | 365d (cert validity) | Release Manager | Revoke at DigiCert; re-order EV cert (HSM-backed); re-sign | gha-secret |
| 64 | Windows EV cert password | `WINDOWS_CODE_SIGNING_PASSWORD` | .github/workflows/sign-windows.yml | DigiCert | n/a | Password set when exporting `.pfx` | 365d (with #63) | Release Manager | Same as #63 | gha-secret |
| 65 | CoreLink PAT (CI client tests) | `CORELINK_PAT` | corelink-cli, .github/workflows/bazel-starter-ci.yml, buck2-starter-ci.yml | (internal) | (admin-ui) | Mint PAT via admin-ui → My PATs → Create | 90d | DevOps | Revoke via admin-ui; reissue | gha-secret |
| 66 | CoreLink base URL (CLI override) | `CORELINK_BASE_URL` | corelink-cli | (internal) | (deploy config) | Override default `https://api.corelink.dev` | rotate-on-compromise | DevOps | N/A (URL) | dev/CI only |
| 67 | CoreLink API URL (browser) | `NEXT_PUBLIC_CORELINK_API_URL` | apps/admin-ui | (internal) | (deploy config) | Public base URL of the CoreLink API the UI talks to | rotate-on-compromise | DevOps | N/A (URL) | vercel-env |
| 68 | CSP report endpoint | `CSP_REPORT_ENDPOINT` | apps/admin-ui (server-side `/api/csp-report`) | (internal) | (deploy config) | Defaults to `${NEXT_PUBLIC_CORELINK_API_URL}/v1/csp-violations` | rotate-on-compromise | DevOps | N/A (URL) | vercel-env |
| 69 | Metrics gateway URL (reproducible-build) | `METRICS_GATEWAY_URL` | .github/workflows/reproducible-build.yml | (internal Prometheus pushgateway) | (deploy config) | Set to internal pushgateway URL | rotate-on-compromise | SRE Lead | N/A (URL) | gha-secret |
| 70 | Metrics gateway token | `METRICS_TOKEN` | .github/workflows/reproducible-build.yml | (internal Prometheus pushgateway) | (deploy config) | Generate at pushgateway admin | 180d | SRE Lead | Rotate at pushgateway; redeploy workflow | gha-secret |
| 71 | Terraform backend bucket | `TF_BACKEND_BUCKET` | .github/workflows/terraform-drift.yml | R2 (internal) | (deploy config) | R2 bucket name for TF state | rotate-on-compromise | SRE Lead | N/A (name) | gha-secret |
| 72 | Terraform backend endpoint | `TF_BACKEND_ENDPOINT` | .github/workflows/terraform-drift.yml | R2 (internal) | (deploy config) | R2 S3-compatible endpoint URL | rotate-on-compromise | SRE Lead | N/A (URL) | gha-secret |
| 73 | Security lead email (notify) | `SECURITY_LEAD_EMAIL` | .github/workflows/pentest-findings-sync.yml | (internal) | n/a | Distribution list address | rotate-on-compromise | Security Lead | N/A (email) | gha-secret |
| 74 | App version (build metadata) | `NEXT_PUBLIC_APP_VERSION` | apps/admin-ui | (internal) | (CI inject) | Injected by CI from `Cargo.toml` / `package.json` | per-deploy | (auto) | N/A (build metadata) | vercel-env |
| 75 | App commit SHA (build metadata) | `NEXT_PUBLIC_APP_COMMIT` | apps/admin-ui | (internal) | (CI inject) | Injected by CI from `${{ github.sha }}` | per-deploy | (auto) | N/A (build metadata) | vercel-env |
| 76 | Server listen port | `PORT` | apps/server | (internal) | (deploy config) | Container runtime injects (default `8080`) | rotate-on-compromise | SRE Lead | N/A (config) | cf-wrangler |
| 77 | Algolia DocSearch app ID | `ALGOLIA_APP_ID` | apps/docs (docusaurus.config.ts) | Algolia | https://www.algolia.com/apps | DocSearch onboarding → application ID | 365d | DevOps | Rotate at Algolia dashboard | vercel-env |
| 78 | Algolia search-only API key | `ALGOLIA_SEARCH_API_KEY` | apps/docs (docusaurus.config.ts) | Algolia | https://www.algolia.com/apps | DocSearch onboarding → search-only key (NOT admin) | 365d | DevOps | Rotate at Algolia dashboard | vercel-env |
| 79 | Algolia DocSearch index name | `ALGOLIA_INDEX_NAME` | apps/docs (docusaurus.config.ts) | Algolia | https://www.algolia.com/apps | DocSearch onboarding → index name | rotate-on-compromise | DevOps | N/A (name) | vercel-env |
| 80 | CSP enforcement mode | `CSP_ENFORCEMENT` | apps/admin-ui (middleware.ts) | (internal) | (deploy config) | Set `enforce` (prod) or `report-only` (staging) | rotate-on-compromise | DevOps | N/A (config flag) | vercel-env |
| 81 | CoreLink API base (admin-ui server) | `CORELINK_API_BASE` | apps/admin-ui (server fetch helpers) | (internal) | (deploy config) | Server-side base URL for CoreLink API | rotate-on-compromise | DevOps | N/A (URL) | vercel-env |
| 82 | Azure KMS staging tenant ID | `AZURE_KMS_STAGING_TENANT_ID` | .github/workflows/byok_matrix_weekly.yml | Azure | https://portal.azure.com/AppRegistrations | Staging-account variant of #30 | rotate-on-compromise | Security Lead | N/A (tenant ID) | gha-secret |
| 83 | Azure KMS staging client ID | `AZURE_KMS_STAGING_CLIENT_ID` | .github/workflows/byok_matrix_weekly.yml | Azure | https://portal.azure.com/AppRegistrations | Staging-account variant of #31 | rotate-on-compromise | Security Lead | Same as #31 | gha-secret |
| 84 | Azure KMS staging client secret | `AZURE_KMS_STAGING_CLIENT_SECRET` | .github/workflows/byok_matrix_weekly.yml | Azure | https://portal.azure.com/AppRegistrations | Staging-account variant of #32 | 90d | Security Lead | Same as #32 | gha-secret |
| 85 | AWS BYOK staging key ID (GHA alias) | `STAGING_AWS_KEY_ID` | .github/workflows/byok_kill_switch_drill_weekly.yml | AWS | https://console.aws.amazon.com/iam | Staging-account variant of #23 (GHA naming) | 90d | Security Lead | Same as #23 | gha-secret |
| 86 | GCP BYOK staging service account (GHA alias) | `STAGING_GCP_SA` | .github/workflows/byok_kill_switch_drill_weekly.yml | GCP | https://console.cloud.google.com/iam-admin/serviceaccounts | Staging-account variant of #28 (GHA naming) | 90d | Security Lead | Same as #28 | gha-secret |
| 87 | Azure BYOK staging client ID (GHA alias) | `STAGING_AZURE_CLIENT_ID` | .github/workflows/byok_kill_switch_drill_weekly.yml | Azure | https://portal.azure.com/AppRegistrations | Staging-account variant of #31 (GHA naming) | rotate-on-compromise | Security Lead | Same as #31 | gha-secret |
| 88 | Vault staging token (GHA) | `STAGING_VAULT_TOKEN` | .github/workflows/byok_kill_switch_drill_weekly.yml | (customer Vault) | (customer-side) | Short-lived token issued by customer staging Vault | ≤ 30d | Security Lead | Customer revokes; we re-bind | gha-secret |
| 89 | Vault staging address (GHA) | `VAULT_STAGING_ADDR` | .github/workflows/byok_matrix_weekly.yml | (customer Vault) | (config) | Same as #35 but separate GHA secret name | rotate-on-compromise | Security Lead | N/A (URL) | gha-secret |
| 90 | Azure workload-identity federated token file path | `AZURE_FEDERATED_TOKEN_FILE` | corelink-byok-azure (`crates/corelink-byok-azure/src/entra.rs:141`) | Azure | https://portal.azure.com/AppRegistrations | OIDC federation: AKS / GHA OIDC writes the projected SA token to this path; consumer reads file contents and POSTs to Entra `/oauth2/v2.0/token` with `client_assertion_type=urn:ietf:params:oauth:client-assertion-type:jwt-bearer` | rotate-on-compromise | Security Lead | N/A (file path; the token inside the file is short-lived per workload-identity issuer) | cf-wrangler (path) + customer-side (token contents) |
| 91 | Azure BYOK matrix-test CMK resource URL | `AZURE_TEST_KEY_RESOURCE` | corelink-byok-matrix-test (`crates/corelink-byok-matrix-test/tests/byok_matrix_test.rs`) | Azure | https://portal.azure.com | Create test Key Vault + RSA-2048 CMK in staging tenant; copy `https://<vault>.vault.azure.net/keys/<name>/<version>` | rotate-on-compromise | Security Lead | Rotate CMK version in Key Vault; update GHA secret | gha-secret |
| 92 | Azure BYOK matrix-test region | `AZURE_TEST_REGION` | corelink-byok-matrix-test (`crates/corelink-byok-matrix-test/tests/byok_matrix_test.rs`) | Azure | https://portal.azure.com | Azure region literal e.g. `eastus2` paired with #91 | rotate-on-compromise | Security Lead | N/A (config, not secret) | gha-secret |
| 93 | Drata API base URL | `DRATA_API_BASE_URL` | corelink-drata-sync (`crates/corelink-drata-sync/src/drata.rs:127`) | Drata | https://app.drata.com | Drata workspace → Integrations → API → base URL (default `https://api.drata.com`) | rotate-on-compromise | Security Lead | N/A (URL) | cf-wrangler |
| 94 | Drata API key (compliance sync) | `DRATA_API_KEY` | corelink-drata-sync (`crates/corelink-drata-sync/src/drata.rs`) | Drata | https://app.drata.com | Drata workspace → Settings → API Keys → Create key with `evidence:write` + `controls:read` scopes (onboarded 2026-05) | 180d | Security Lead | Revoke key in Drata UI; create new; redeploy corelink-drata-sync | cf-wrangler |
| 95 | HTTP listener port (R2-12 webhook stack) | `HTTP_PORT` | apps/server (`apps/server/src/main.rs:72`) | (internal) | (deploy config) | Override default `50052`; container runtime injects | rotate-on-compromise | SRE Lead | N/A (port literal, not a secret) | cf-wrangler |
| 96 | k6 Prometheus remote-write server URL (load-test telemetry) | `K6_PROMETHEUS_RW_SERVER_URL` | tests/load/k6 (`tests/load/k6/scenarios/endurance-24h.js`) + .github/workflows/endurance-2h-nightly.yml | (internal Prometheus) | (deploy config) | Internal pushgateway / Prometheus RW endpoint URL | rotate-on-compromise | SRE Lead | N/A (URL) | gha-secret |
| 97 | k6 staging BYOK CMK identifier | `K6_STAGING_BYOK_CMK_ID` | .github/workflows/load-test-nightly.yml | (internal) | (deploy config) | Pre-provisioned staging CMK ID used by load-test BYOK scenarios | rotate-on-compromise | Security Lead | Rotate staging CMK; update GHA secret | gha-secret |
| 98 | k6 staging MFA stub token | `K6_STAGING_MFA_STUB` | .github/workflows/load-test-nightly.yml | (internal) | (deploy config) | Stub MFA bypass token honoured ONLY by staging build (production HARD-rejects) | 30d | DevOps | Regenerate via `openssl rand -hex 32`; update GHA secret + staging `cf-wrangler` in lockstep | gha-secret |
| 99 | k6 staging PAT (load-test client auth) | `K6_STAGING_PAT` | .github/workflows/load-test-nightly.yml + endurance-2h-nightly.yml | (internal) | (admin-ui) | Mint long-lived staging PAT via admin-ui (staging tenant) → My PATs → Create; pairs with #65 production variant | 90d | DevOps | Revoke PAT via admin-ui; reissue; update GHA secret | gha-secret |
| 100 | k6 staging Stripe webhook signing secret | `K6_STAGING_STRIPE_WHSEC` | .github/workflows/load-test-nightly.yml | Stripe | https://dashboard.stripe.com/test/webhooks | Create webhook endpoint on Stripe test mode → copy signing secret; pairs with #3 production variant | 90d | SRE Lead | Recreate test webhook endpoint; update GHA secret | gha-secret |
| 101 | k6 target host (load-test base URL) | `K6_TARGET_HOST` | tests/load/k6 (`tests/load/k6/cas-write-read.js`, `tests/load/k6/stripe-webhook-burst.js`, `tests/load/k6/dsr-api.js`) | (internal) | (deploy config) | Staging API base URL e.g. `https://api-staging.corelink.dev` | rotate-on-compromise | SRE Lead | N/A (URL) | gha-secret |
| 102 | PagerDuty routing key (compliance digest pager) | `PAGERDUTY_COMPLIANCE_KEY` | .github/workflows/compliance-weekly.yml (`secrets.PAGERDUTY_COMPLIANCE_KEY`) | PagerDuty | https://pagerduty.com | Separate `corelink-compliance` service → Integrations → Events API v2 → copy routing key (third PD integration; pairs with #11, #12) | 365d | SRE Lead | Recreate integration; redeploy workflow | gha-secret |
| 103 | Stripe price ID — Starter plan | `STRIPE_PRICE_ID_STARTER` | corelink-stripe-real (`crates/corelink-stripe-real/tests/live_integration.rs`) + tests/e2e-signup-flow | Stripe | https://dashboard.stripe.com/prices | Products → Starter plan → copy price ID (`price_...`); not a credential but required for e2e signup flow | rotate-on-compromise | SRE Lead | N/A (price ID, not secret; rotate only on plan SKU change) | cf-wrangler |
| 104 | Vault AWS IAM auth role name | `VAULT_AWS_ROLE` | corelink-byok-vault (`crates/corelink-byok-vault/src/auth.rs:15`) | (customer Vault) | (customer-side) | Customer Vault admin configures `auth/aws/role/<name>` and shares role name (pairs with #33..#35) | per customer | Customer | Customer rotates role binding; we re-bind | customer-side |
| 105 | Vault Kubernetes auth role name | `VAULT_K8S_ROLE` | corelink-byok-vault (`crates/corelink-byok-vault/src/auth.rs:14`) | (customer Vault) | (customer-side) | Customer Vault admin configures `auth/kubernetes/role/<name>` and shares role name | per customer | Customer | Customer rotates role binding; we re-bind | customer-side |
| 106 | Vault Kubernetes SA projected token | `VAULT_K8S_SERVICE_ACCOUNT_TOKEN` | corelink-byok-vault (`crates/corelink-byok-vault/src/auth.rs:111`) | (customer Vault) | (customer-side) | Kubernetes projects a short-lived SA token into the pod; consumer reads and POSTs to `/v1/auth/kubernetes/login` | per pod (≤ 1h projected TTL) | Customer | Pod restart re-projects; on compromise customer revokes the SA + recreates | customer-side |
| 107 | Vault TLS skip-verify flag (dev/test only) | `VAULT_SKIP_VERIFY` | corelink-byok-vault (`crates/corelink-byok-vault/src/real.rs:152`) | (internal) | n/a | Set `true` in dev/test only — production build HARD-rejects this value (mirrors `CORELINK_BYOK_VAULT_MOCK` #36 pattern) | rotate-on-compromise | Security Lead | N/A (test flag; production fails closed) | dev only |
| 108 | Vault direct-token auth credential | `VAULT_TOKEN` | corelink-byok-vault (`crates/corelink-byok-vault/src/auth.rs:102`, `src/real.rs`) + .github/workflows/byok_kill_switch_drill_weekly.yml | (customer Vault) | (customer-side) | Customer Vault issues a direct token (developer-mode auth); pairs with #33..#35 AppRole as alternative auth path; staging GHA variant is #88 | ≤ 30d | Customer | Customer revokes token via `vault token revoke`; we re-bind | customer-side |

**Total rows: 108**

## Notes

- **Stripe** (#1–#3) and **PagerDuty alerts** (#11) are the two highest-blast-radius
  rotations. Their compromise procedures include downstream replay obligations
  (`corelink-billing-replay`, `corelink-oncall` synthetic re-page) — see
  `secrets-runbook.md` §3.
- **Slack webhooks** (#13–#19) are functionally six independent secrets; treat
  them as a single rotation batch every 365d to amortize toil.
- **BYOK customer-side** secrets (#33, #34) are NOT in `cf-wrangler` —
  customers provision them in their own Vault and we receive scoped credentials
  per-tenant. We never persist these centrally.
- **Build-metadata** rows (#74, #75) are not secrets in the cryptographic sense
  but are included so the verifier doesn't flag them as missing.

## Drift policy

If you add an `env::var("FOO_BAR")` call in a new crate, you MUST add a row to
this matrix in the SAME PR. The CF deploy gate (`cf-deploy-prod.yml`) calls
`scripts/secrets-checklist-verify.sh` and will fail the deploy if drift exists.

If a secret is removed from the codebase, remove the matrix row in the SAME PR.

See also: `docs/internal/secrets-runbook.md` for operational procedures
(initial population, rotation, compromise response).
