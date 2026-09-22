# WI-S14-001 / #1721 — the WEUR root owns exactly one region module.
# Auto-apply FORBIDDEN — see RB-FM-206 and RB-TERRAFORM-DRIFT.

provider "cloudflare" {}

module "weur" {
  source = "../../modules/corelink-region"

  region_name      = "weur"
  r2_location_hint = "weur"
  d1_location      = "weur"
  do_jurisdiction  = "eu"
  cf_zone_id       = var.cf_zone_id
  cf_account_id    = var.cf_account_id
  environment      = var.environment
}

output "weur_r2_bucket_name" {
  value = module.weur.r2_bucket_name
}

output "weur_d1_instance_id" {
  value = module.weur.d1_instance_id
}

output "weur_do_namespace_id" {
  value = module.weur.do_namespace_id
}

output "weur_kv_namespace_id" {
  value = module.weur.kv_namespace_id
}
