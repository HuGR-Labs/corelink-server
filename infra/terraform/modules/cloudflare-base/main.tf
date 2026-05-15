# cloudflare-base — Cloudflare account / zone / DNS-records / Worker-route /
# Pages-project / custom-domain module.
#
# Scope: zone-level resources that are not region-scoped (DNS records for
# DKIM/SPF/DMARC, public Worker routes, Pages project, custom domain
# mapping). For region-scoped storage/compute, see cloudflare-storage and
# the legacy corelink-region module.
#
# Auto-apply FORBIDDEN — see RB-FM-206 / RB-TERRAFORM-DRIFT.
# All changes require: PR + CODEOWNERS review + manual apply workflow
# + dual-approval (WI-S13-002).
#
# Provider versions SHA-pinned via parent terraform block (see
# environments/staging/main.tf).

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52.0"
    }
  }
}

# ---------------------------------------------------------------------------
# Locals
# ---------------------------------------------------------------------------

locals {
  base_tags = {
    module      = "cloudflare-base"
    environment = var.environment
    managed_by  = "terraform"
    roadmap     = "R-2 / R-6"
  }
}

# ---------------------------------------------------------------------------
# DNS records — DKIM / SPF / DMARC for transactional mail
# Email deliverability hardening — see docs/internal/secrets-checklist.md
# rows referencing SendGrid / Twilio.
# ---------------------------------------------------------------------------

resource "cloudflare_record" "spf" {
  count   = var.create_email_records ? 1 : 0
  zone_id = var.cf_zone_id
  name    = var.email_root
  type    = "TXT"
  value   = var.spf_record_value
  ttl     = 3600
  comment = "cloudflare-base: SPF for transactional mail. Managed by Terraform."
}

resource "cloudflare_record" "dkim" {
  for_each = var.create_email_records ? var.dkim_records : {}
  zone_id  = var.cf_zone_id
  name     = each.key
  type     = "CNAME"
  value    = each.value
  ttl      = 3600
  comment  = "cloudflare-base: DKIM selector ${each.key}. Managed by Terraform."
}

resource "cloudflare_record" "dmarc" {
  count   = var.create_email_records ? 1 : 0
  zone_id = var.cf_zone_id
  name    = "_dmarc.${var.email_root}"
  type    = "TXT"
  value   = var.dmarc_record_value
  ttl     = 3600
  comment = "cloudflare-base: DMARC policy. Managed by Terraform."
}

# ---------------------------------------------------------------------------
# Apex DNS record — points api.corelink.dev → CF proxy
# Region-specific subdomains are managed in modules/corelink-region.
# ---------------------------------------------------------------------------

resource "cloudflare_record" "api_apex" {
  count   = var.create_api_apex ? 1 : 0
  zone_id = var.cf_zone_id
  name    = var.api_subdomain
  type    = "CNAME"
  value   = var.api_apex_target
  proxied = true
  comment = "cloudflare-base: API apex → CF edge. Managed by Terraform."
}

# ---------------------------------------------------------------------------
# Worker route — apex `api.corelink.dev/*` smart-routes to closest region.
# Per-region explicit routes live in modules/corelink-region.
# ---------------------------------------------------------------------------

resource "cloudflare_worker_route" "api_apex_route" {
  count       = var.create_apex_worker_route ? 1 : 0
  zone_id     = var.cf_zone_id
  pattern     = "${var.api_subdomain}.${var.zone_name}/*"
  script_name = var.apex_worker_script_name
}

# ---------------------------------------------------------------------------
# Cloudflare Pages project — admin-ui hosting (when not Vercel-hosted).
# `var.create_pages_project = false` by default; admin-ui currently uses
# Vercel (see ROADMAP §2 H-2). Toggleable for migration scenarios.
# ---------------------------------------------------------------------------

resource "cloudflare_pages_project" "admin_ui" {
  count             = var.create_pages_project ? 1 : 0
  account_id        = var.cf_account_id
  name              = var.pages_project_name
  production_branch = var.pages_production_branch

  build_config {
    build_command   = var.pages_build_command
    destination_dir = var.pages_destination_dir
    root_dir        = var.pages_root_dir
  }
}

# ---------------------------------------------------------------------------
# Custom domain mapping for Pages project (when enabled).
# ---------------------------------------------------------------------------

resource "cloudflare_pages_domain" "admin_ui" {
  count        = var.create_pages_project && var.pages_custom_domain != null ? 1 : 0
  account_id   = var.cf_account_id
  project_name = cloudflare_pages_project.admin_ui[0].name
  domain       = var.pages_custom_domain
}
