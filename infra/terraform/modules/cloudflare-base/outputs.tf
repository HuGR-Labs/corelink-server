# cloudflare-base — Outputs.

output "api_apex_fqdn" {
  description = "FQDN for the API apex (api.<zone_name>)."
  value       = "${var.api_subdomain}.${var.zone_name}"
}

output "pages_project_name" {
  description = "Pages project name (null when disabled)."
  value       = var.create_pages_project ? cloudflare_pages_project.admin_ui[0].name : null
}

output "pages_subdomain" {
  description = "Pages-generated subdomain (null when disabled)."
  value       = var.create_pages_project ? cloudflare_pages_project.admin_ui[0].subdomain : null
}

output "dkim_record_ids" {
  description = "Map of DKIM selector → record ID."
  value       = { for k, v in cloudflare_record.dkim : k => v.id }
}
