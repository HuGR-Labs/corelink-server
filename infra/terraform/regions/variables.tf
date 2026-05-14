# WI-S14-001 — Shared variables for all region invocations.

variable "cf_account_id" {
  type        = string
  description = "Cloudflare account ID. Injected via TF_VAR_cf_account_id env var (OIDC-bound)."
  sensitive   = true
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID for api.corelink.dev. Injected via TF_VAR_cf_zone_id."
}

variable "environment" {
  type        = string
  description = "Deployment environment: staging | production"
  default     = "staging"

  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "environment must be one of: staging, production"
  }
}
