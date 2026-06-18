# byok-providers/gcp-kms — CoreLink-side GCP service-account + WIF binding.
#
# Provisions in CoreLink's OWN GCP project:
#   * Service Account that customer IAM bindings grant key-user on.
#   * Workload Identity Pool Provider for cross-org federation.
#
# Customer-side CryptoKey + key IAM binding is OUT OF SCOPE.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.30"
    }
  }
}

locals {
  sa_account_id = "corelink-byok-${replace(var.customer_tenant_id, "_", "-")}"
}

resource "google_service_account" "byok" {
  project      = var.corelink_gcp_project
  account_id   = local.sa_account_id
  display_name = "CoreLink BYOK service account for ${var.customer_tenant_id}"
}

# Workload Identity Pool — federation entry for the CoreLink runtime.
resource "google_iam_workload_identity_pool" "byok" {
  project                   = var.corelink_gcp_project
  workload_identity_pool_id = "corelink-byok-${var.customer_tenant_id}"
  display_name              = "CoreLink BYOK pool — ${var.customer_tenant_id}"
}

# OIDC provider inside the pool — trusts CoreLink's runtime issuer.
resource "google_iam_workload_identity_pool_provider" "byok" {
  project                            = var.corelink_gcp_project
  workload_identity_pool_id          = google_iam_workload_identity_pool.byok.workload_identity_pool_id
  workload_identity_pool_provider_id = "corelink-runtime"

  oidc {
    issuer_uri        = var.corelink_oidc_issuer
    allowed_audiences = [var.corelink_oidc_audience]
  }

  attribute_mapping = {
    "google.subject" = "assertion.sub"
  }

  # Fail-closed at the PROVIDER (defense-in-depth, closes trivy GCP-0068): only
  # accept tokens whose `sub` is the expected CoreLink runtime subject. Without
  # this the provider would federate ANY `sub` from the issuer — even though the
  # SA `wif_binding` below already restricts impersonation to this exact subject.
  # Pinning the condition here rejects an unexpected subject at federation time,
  # one layer earlier than the IAM binding.
  attribute_condition = "assertion.sub == \"${var.corelink_runtime_subject}\""
}

# Allow the runtime principal (mapped via the WIF provider) to impersonate
# the SA above.
resource "google_service_account_iam_member" "wif_binding" {
  service_account_id = google_service_account.byok.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "principal://iam.googleapis.com/projects/${var.corelink_gcp_project_number}/locations/global/workloadIdentityPools/${google_iam_workload_identity_pool.byok.workload_identity_pool_id}/subject/${var.corelink_runtime_subject}"
}
