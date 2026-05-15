output "application_client_id" {
  description = "CoreLink AAD app client ID. Customer admin uses this to consent the app in their tenant."
  value       = azuread_application.byok.client_id
}

output "service_principal_object_id" {
  description = "Object ID of the CoreLink-side SP."
  value       = azuread_service_principal.byok.object_id
}

output "client_secret" {
  description = "Generated client secret. Treat as a sensitive value — push into cloudflare-secrets module immediately."
  value       = azuread_application_password.byok.value
  sensitive   = true
}
