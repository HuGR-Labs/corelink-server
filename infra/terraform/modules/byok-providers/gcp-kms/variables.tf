variable "customer_tenant_id" {
  type        = string
  description = "CoreLink-internal tenant identifier."
}

variable "corelink_gcp_project" {
  type        = string
  description = "CoreLink-owned GCP project ID where the SA + WIF pool live."
}

variable "corelink_gcp_project_number" {
  type        = string
  description = "Numeric project number for the same project (used in WIF principal URIs)."
}

variable "corelink_oidc_issuer" {
  type        = string
  description = "OIDC issuer URL that signs CoreLink runtime tokens."
}

variable "corelink_oidc_audience" {
  type        = string
  description = "OIDC audience claim expected on incoming tokens."
}

variable "corelink_runtime_subject" {
  type        = string
  description = "Subject claim of the CoreLink runtime principal (sub claim value)."
}
