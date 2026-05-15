# WI-S14-001 — WEUR region (eu-west) Terraform invocation.
# Auto-apply FORBIDDEN — see RB-FM-206 + RB-region.
#
# CRITICAL: do_jurisdiction = "eu" MANDATORY for WEUR.
# Schrems II + GDPR Art. 46: EU data must not transit non-EU infrastructure.
# DO without "eu" jurisdiction may route to US edge per CF capacity.
# Post-deploy: verify_do_jurisdiction.sh MUST confirm jurisdiction = "eu".
# CI gate fails PR if mismatch. (WI-S14-001 §2 risk R-003)

module "weur" {
  source = "../modules/corelink-region"

  region_name      = "weur"
  r2_location_hint = "weur"
  d1_location      = "weur"
  do_jurisdiction  = "eu" # MANDATORY: WEUR EU jurisdiction (Schrems II + GDPR Art. 46)
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

output "weur_do_jurisdiction" {
  description = "WEUR DO jurisdiction — must always be 'eu'. Consumed by CI gate + PRR Compliance attestation."
  value       = module.weur.do_jurisdiction
}
