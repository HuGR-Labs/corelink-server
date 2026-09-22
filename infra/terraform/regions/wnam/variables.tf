# Per-region root inputs. Values are injected through TF_VAR_* at runtime.

variable "cf_account_id" {
  type        = string
  description = "Cloudflare account ID. Injected via TF_VAR_cf_account_id."
  sensitive   = true
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID for api.humangr.com."
}

variable "environment" {
  type        = string
  description = "Deployment environment: staging | production"
  default     = "production"

  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "environment must be one of: staging, production"
  }
}
