# WI-S14-001 / #1721 — the SAM root owns exactly one region module.
# Auto-apply FORBIDDEN — see RB-FM-206 and RB-TERRAFORM-DRIFT.

provider "cloudflare" {}

module "sam" {
  source = "../../modules/corelink-region"

  region_name      = "sam"
  r2_location_hint = "sam"
  d1_location      = "sam"
  do_jurisdiction  = "none"
  cf_zone_id       = var.cf_zone_id
  cf_account_id    = var.cf_account_id
  environment      = var.environment
}

output "sam_r2_bucket_name" {
  value = module.sam.r2_bucket_name
}

output "sam_d1_instance_id" {
  value = module.sam.d1_instance_id
}

output "sam_do_namespace_id" {
  value = module.sam.do_namespace_id
}

output "sam_kv_namespace_id" {
  value = module.sam.kv_namespace_id
}
