# cloudflare-storage — Multi-region R2 / D1 / KV / Durable Objects.
#
# Scope: a thin wrapper around the per-region `corelink-region` module so
# environments can spin up all 4 regions in one call AND declare any
# globally-scoped KV namespaces (e.g. feature flags) + Durable-Object
# bindings that are not per-region (rate-limit DO, sessions DO).
#
# Per-region resources delegate to modules/corelink-region (WI-S14-001).
#
# Auto-apply FORBIDDEN — see RB-TERRAFORM-DRIFT / RB-FM-206.

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
# Per-region fan-out — delegates to the legacy `corelink-region` module
# which already provisions R2 + D1 + KV + DO Worker + region DNS.
# ---------------------------------------------------------------------------

module "region" {
  for_each = var.regions

  source = "../corelink-region"

  region_name               = each.key
  r2_location_hint          = each.value.r2_location_hint
  d1_location               = each.value.d1_location
  do_jurisdiction           = each.value.do_jurisdiction
  cf_account_id             = var.cf_account_id
  cf_zone_id                = var.cf_zone_id
  environment               = var.environment
  kv_namespace_title_prefix = var.kv_namespace_title_prefix
}

# ---------------------------------------------------------------------------
# Global KV namespaces (not region-scoped).
# E.g. feature flags, config snapshots that intentionally need global reach.
# ---------------------------------------------------------------------------

resource "cloudflare_workers_kv_namespace" "global" {
  for_each = var.global_kv_namespaces

  account_id = var.cf_account_id
  title      = "${each.value}-${var.environment}"
}

# ---------------------------------------------------------------------------
# Durable Object Worker for sessions / rate-limiting.
# Stub script — production code deployed via Worker pipeline.
# Terraform manages the script declaration + DO bindings only.
# ---------------------------------------------------------------------------

resource "cloudflare_workers_script" "global_do_host" {
  count = var.create_global_do_host ? 1 : 0

  account_id = var.cf_account_id
  name       = "corelink-do-host-${var.environment}"
  content    = file("${path.module}/do_host_stub.js")
  module     = true
}
