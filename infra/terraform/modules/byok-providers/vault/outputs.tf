output "role_name" {
  description = "AppRole role name. Customer fetches role_id / secret_id via `vault read auth/<backend>/role/<role_name>`."
  value       = vault_approle_auth_backend_role.byok.role_name
}

output "policy_name" {
  description = "Vault policy name attached to the AppRole."
  value       = vault_policy.byok.name
}
