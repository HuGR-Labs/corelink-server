# WI-S14-001 — ENAM region (us-east) Terraform invocation.
# Auto-apply FORBIDDEN — see RB-FM-206 + RB-region.

module "enam" {
  source = "../modules/corelink-region"

  region_name      = "enam"
  r2_location_hint = "enam"
  d1_location      = "enam"
  do_jurisdiction  = "us"   # ENAM: US jurisdiction
  cf_zone_id       = var.cf_zone_id
  cf_account_id    = var.cf_account_id
  environment      = var.environment
}

output "enam_r2_bucket_name" {
  value = module.enam.r2_bucket_name
}

output "enam_d1_instance_id" {
  value = module.enam.d1_instance_id
}

output "enam_do_namespace_id" {
  value = module.enam.do_namespace_id
}

output "enam_kv_namespace_id" {
  value = module.enam.kv_namespace_id
}
