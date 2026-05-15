# cloudflare-secrets — Variables.

variable "worker_name" {
  type        = string
  description = "Target Worker name receiving the secrets (matches name= in wrangler.toml)."
}

variable "environment" {
  type        = string
  description = "Wrangler env: staging | production."

  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "environment must be one of: staging, production."
  }
}

variable "secrets" {
  type        = map(string)
  description = <<-EOT
    Map of secret env var name → value. Keys MUST correspond to rows in
    docs/internal/secrets-checklist.md. Values MUST come from OIDC-bound
    vault reads (TF_VAR_<...>) — NEVER literal in code.
    Example:
      {
        STRIPE_SECRET_KEY     = var.tf_secret_stripe_live
        CLERK_SECRET_KEY      = var.tf_secret_clerk_prod
        PAGERDUTY_ROUTING_KEY = var.tf_secret_pagerduty_prod
      }
  EOT
  sensitive   = true
}
