# Module: `cloudflare-secrets`

Pushes Worker secrets to Cloudflare via `wrangler secret put`, wrapped in a
`null_resource` so Terraform can plan/apply secret rotations as part of an
environment definition. The Cloudflare Terraform provider 4.x does not
expose Worker secrets as a first-class resource yet.

## Hard rules

1. **No literal secret values in code.** Values arrive via Terraform
   sensitive variables sourced from OIDC-bound vault reads.
2. Every secret name pushed here MUST correspond to a row in
   [`docs/internal/secrets-checklist.md`](../../../../docs/internal/secrets-checklist.md).
   `scripts/secrets-checklist-verify.sh` enforces this in
   `.github/workflows/cf-deploy-prod.yml`.
3. Auto-apply FORBIDDEN — see RB-FM-206 / RB-TERRAFORM-DRIFT.
4. Idempotent — `wrangler secret put` overwrites without erroring.

## Prerequisites

- `wrangler` ≥ 3.x installed on the apply runner.
- Cloudflare credentials via OIDC env (`CLOUDFLARE_API_TOKEN` injected at
  runtime; not present in Terraform state).

## Usage

```hcl
module "secrets_staging" {
  source = "../../modules/cloudflare-secrets"

  worker_name = "corelink-server"
  environment = "staging"

  secrets = {
    STRIPE_SECRET_KEY_TEST          = var.stripe_test_secret
    CLERK_SECRET_KEY                = var.clerk_staging_secret
    PAGERDUTY_ROUTING_KEY           = var.pagerduty_staging_routing
    SLACK_WEBHOOK_URL_ALERTS_SEV1   = var.slack_alerts_sev1
    SLACK_WEBHOOK_URL_ALERTS_SEV2   = var.slack_alerts_sev2
    HUBSPOT_PRIVATE_APP_TOKEN       = var.hubspot_token
  }
}
```

## Trigger semantics

Each `null_resource` re-runs when:

- the secret key set changes, or
- the SHA-256 hash of the value changes (the value itself is never put
  into state).

That is sufficient for rotation because each rotation produces a new
value-hash.

## Cross-links

- `docs/internal/secrets-checklist.md` — single source of truth (matrix).
- `docs/internal/secrets-runbook.md` — operational rotation process.
- `specs/_runbooks/RB-SECRETS-DRIFT.md` — drift triage.
- `specs/_runbooks/RB-TERRAFORM-DRIFT.md` — terraform-side drift triage.
