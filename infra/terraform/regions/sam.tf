# WI-S14-001 — SAM region (sa-east) Terraform invocation.
# Auto-apply FORBIDDEN — see RB-FM-206 + RB-region.

module "sam" {
  source = "../modules/corelink-region"

  region_name      = "sam"
  r2_location_hint = "sam"
  d1_location      = "sam"
  do_jurisdiction  = "none" # SAM: no current regulatory jurisdiction mandate
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
