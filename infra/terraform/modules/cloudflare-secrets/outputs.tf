# cloudflare-secrets — Outputs.

output "secret_keys_pushed" {
  description = "Sorted list of secret env var names this module pushed. Values are NEVER exposed."
  value       = local.secret_keys
}

output "trigger_hash" {
  description = "SHA-256 of the sorted key set. Changes only when keys change, not values."
  value       = local.trigger_hash
}
