# WI-S14-001 / #1721 — the WNAM root owns exactly one region module.
# Auto-apply FORBIDDEN — see RB-FM-206 and RB-TERRAFORM-DRIFT.

provider "cloudflare" {}

module "wnam" {
  source = "../../modules/corelink-region"

  region_name      = "wnam"
  r2_location_hint = "wnam"
  d1_location      = "wnam"
  do_jurisdiction  = "us"
  cf_zone_id       = var.cf_zone_id
  cf_account_id    = var.cf_account_id
  environment      = var.environment
}

output "wnam_r2_bucket_name" {
  value = module.wnam.r2_bucket_name
}

output "wnam_d1_instance_id" {
  value = module.wnam.d1_instance_id
}

output "wnam_do_namespace_id" {
  value = module.wnam.do_namespace_id
}

output "wnam_kv_namespace_id" {
  value = module.wnam.kv_namespace_id
}
