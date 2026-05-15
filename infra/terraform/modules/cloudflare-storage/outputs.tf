# cloudflare-storage — Outputs.

output "region_r2_bucket_names" {
  description = "Map of region → R2 bucket name."
  value       = { for k, m in module.region : k => m.r2_bucket_name }
}

output "region_d1_database_ids" {
  description = "Map of region → D1 database ID."
  value       = { for k, m in module.region : k => m.d1_instance_id }
}

output "region_kv_namespace_ids" {
  description = "Map of region → session KV namespace ID."
  value       = { for k, m in module.region : k => m.kv_namespace_id }
}

output "global_kv_namespace_ids" {
  description = "Map of logical global KV name → namespace ID."
  value       = { for k, ns in cloudflare_workers_kv_namespace.global : k => ns.id }
}

output "global_do_host_worker_name" {
  description = "Global DO host Worker script name (null if not created)."
  value       = var.create_global_do_host ? cloudflare_workers_script.global_do_host[0].name : null
}
