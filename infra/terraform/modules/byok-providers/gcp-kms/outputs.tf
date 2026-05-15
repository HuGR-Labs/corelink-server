output "service_account_email" {
  description = "Email of the CoreLink-side BYOK SA — customer grants this principal `roles/cloudkms.cryptoKeyEncrypterDecrypter` on their key."
  value       = google_service_account.byok.email
}

output "wif_pool_name" {
  description = "Workload Identity Pool resource name."
  value       = google_iam_workload_identity_pool.byok.name
}
