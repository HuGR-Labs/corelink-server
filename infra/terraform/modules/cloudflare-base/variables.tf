# cloudflare-base — Variables.
# All sensitive inputs MUST be supplied via TF_VAR_* env vars sourced from
# OIDC-bound credentials. No hard-coded secrets.

variable "cf_account_id" {
  type        = string
  description = "Cloudflare account ID. Injected via TF_VAR_cf_account_id (OIDC-bound)."
  sensitive   = true
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID for the apex domain. Injected via TF_VAR_cf_zone_id."
  sensitive   = true
}

variable "zone_name" {
  type        = string
  description = "Apex zone name (e.g. corelink.dev)."
}

variable "environment" {
  type        = string
  description = "Deployment environment: staging | production."

  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "environment must be one of: staging, production."
  }
}

# -----------------------------------------------------------------------
# Email DNS records — DKIM / SPF / DMARC
# -----------------------------------------------------------------------

variable "create_email_records" {
  type        = bool
  description = "Whether to provision DKIM/SPF/DMARC. Disable for ephemeral envs."
  default     = true
}

variable "email_root" {
  type        = string
  description = "Email root subdomain (usually `@` or `mail`)."
  default     = "@"
}

variable "spf_record_value" {
  type        = string
  description = "SPF record value (e.g. \"v=spf1 include:sendgrid.net ~all\")."
  default     = "v=spf1 include:sendgrid.net ~all"
}

variable "dkim_records" {
  type        = map(string)
  description = "Map of DKIM selector hostname → CNAME target."
  default     = {}
}

variable "dmarc_record_value" {
  type        = string
  description = "DMARC policy TXT value."
  default     = "v=DMARC1; p=quarantine; rua=mailto:dmarc@corelink.dev; ruf=mailto:dmarc@corelink.dev; fo=1"
}

# -----------------------------------------------------------------------
# API apex
# -----------------------------------------------------------------------

variable "create_api_apex" {
  type        = bool
  description = "Whether to provision the api.<zone> apex DNS record."
  default     = true
}

variable "api_subdomain" {
  type        = string
  description = "API apex subdomain (e.g. \"api\" → api.corelink.dev)."
  default     = "api"
}

variable "api_apex_target" {
  type        = string
  description = "CNAME target for the API apex (e.g. corelink-router.workers.dev)."
  default     = "corelink-router.workers.dev"
}

variable "create_apex_worker_route" {
  type        = bool
  description = "Whether to create a Worker route pattern for the API apex."
  default     = true
}

variable "apex_worker_script_name" {
  type        = string
  description = "Worker script name handling apex traffic (smart-router)."
  default     = "corelink-router"
}

# -----------------------------------------------------------------------
# Pages project (admin-ui)
# -----------------------------------------------------------------------

variable "create_pages_project" {
  type        = bool
  description = "Whether to provision a Cloudflare Pages project for admin-ui."
  default     = false
}

variable "pages_project_name" {
  type        = string
  description = "Pages project name."
  default     = "corelink-admin-ui"
}

variable "pages_production_branch" {
  type        = string
  description = "Production branch for Pages project."
  default     = "main"
}

variable "pages_build_command" {
  type        = string
  description = "Pages build command."
  default     = "pnpm --filter admin-ui build"
}

variable "pages_destination_dir" {
  type        = string
  description = "Pages output directory."
  default     = "apps/admin-ui/.next"
}

variable "pages_root_dir" {
  type        = string
  description = "Pages project root dir within the monorepo."
  default     = "/"
}

variable "pages_custom_domain" {
  type        = string
  description = "Custom domain for the Pages project. null disables mapping."
  default     = null
}
