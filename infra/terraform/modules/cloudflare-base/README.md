# Module: `cloudflare-base`

Zone-level Cloudflare resources that are not region-scoped.

Scope:

- Apex DNS record `api.<zone>` → smart-router Worker.
- Worker route pattern for the API apex.
- DKIM / SPF / DMARC records for transactional mail deliverability.
- Optional Cloudflare Pages project + custom-domain mapping (for `admin-ui`
  when not on Vercel).

Out of scope (use sibling modules):

- Region-scoped R2 / D1 / KV / Workers → `cloudflare-storage` and the legacy
  `corelink-region` module.
- Secret material → `cloudflare-secrets`.
- BYOK provider IAM → `byok-providers/*`.

## Usage

```hcl
module "cf_base" {
  source = "../../modules/cloudflare-base"

  cf_account_id = var.cf_account_id
  cf_zone_id    = var.cf_zone_id
  zone_name     = "corelink.dev"
  environment   = "staging"

  create_email_records = true
  dkim_records = {
    "s1._domainkey" = "s1.domainkey.u123456.wl123.sendgrid.net"
    "s2._domainkey" = "s2.domainkey.u123456.wl123.sendgrid.net"
  }

  create_pages_project = false
}
```

## Operational rules

- All credentials are OIDC-bound; no long-lived API tokens in code.
- `terraform apply` runs ONLY through the manual apply workflow with
  dual-approval (WI-S13-002).
- Drift detected by `RB-TERRAFORM-DRIFT` daily cron.

## Cross-links

- `docs/internal/secrets-checklist.md` — runtime secrets matrix (Stripe,
  Clerk, PagerDuty, Slack, etc.).
- `ROADMAP-TO-GA.md` §2 R-2 (wiring) + §6 R-6 (sustained staging).
- `specs/_runbooks/RB-FM-206.md` — auto-apply prohibition.
- `specs/_runbooks/RB-TERRAFORM-DRIFT.md` — drift triage.
