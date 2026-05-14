# WI-S14-001 — Terraform module `corelink-region` outputs.
# 4 canonical outputs consumed by downstream WIs (WI-S14-002..009).

output "r2_bucket_name" {
  description = "R2 bucket name (corelink-cas-{region}). Consumed by WI-S14-002 insert checks + WI-S14-003 replica worker."
  value       = cloudflare_r2_bucket.corelink_cas.name
}

output "d1_instance_id" {
  description = "D1 database instance ID. Consumed by WI-S14-002 tenant region column + WI-S14-006 CMK revocation state."
  value       = cloudflare_d1_database.corelink_meta.id
}

output "do_namespace_id" {
  description = "DO Worker script name/namespace ID. Consumed by WI-S14-002 region_enforcer + WI-S14-006 kill switch."
  value       = cloudflare_workers_script.corelink_do_region.name
}

output "zone_id" {
  description = "Cloudflare zone ID passed through for downstream use."
  value       = var.cf_zone_id
}

output "kv_namespace_id" {
  description = "KV namespace ID (corelink-session-{region}). Consumed by WI-S14-002 session scope enforcement."
  value       = cloudflare_workers_kv_namespace.corelink_session.id
}

output "region_name" {
  description = "Region name passthrough (wnam/enam/weur/sam). Enables composed module outputs."
  value       = var.region_name
}

output "do_jurisdiction" {
  description = "DO jurisdiction setting applied. Used by verify_do_jurisdiction.sh + WI-S14-009 PRR."
  value       = var.do_jurisdiction
}
