# cloudflare-storage — Variables.

variable "cf_account_id" {
  type        = string
  description = "Cloudflare account ID. Injected via TF_VAR_cf_account_id (OIDC-bound)."
  sensitive   = true
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID. Injected via TF_VAR_cf_zone_id."
  sensitive   = true
}

variable "environment" {
  type        = string
  description = "Deployment environment: staging | production."

  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "environment must be one of: staging, production."
  }
}

variable "regions" {
  description = <<-EOT
    Map of region_name → per-region config.
    Keys MUST be in {wnam, enam, weur, sam}.
    Example:
      {
        wnam = { r2_location_hint = "wnam", d1_location = "wnam", do_jurisdiction = "us"   }
        enam = { r2_location_hint = "enam", d1_location = "enam", do_jurisdiction = "none" }
        weur = { r2_location_hint = "weur", d1_location = "weur", do_jurisdiction = "eu"   }
        sam  = { r2_location_hint = "sam",  d1_location = "sam",  do_jurisdiction = "none" }
      }
  EOT
  type = map(object({
    r2_location_hint = string
    d1_location      = string
    do_jurisdiction  = string
  }))
}

variable "kv_namespace_title_prefix" {
  type        = string
  description = "KV namespace title prefix for per-region session caches."
  default     = "corelink-session"
}

variable "global_kv_namespaces" {
  type        = map(string)
  description = "Map of logical name → KV title for globally-scoped namespaces (e.g. feature flags)."
  default = {
    flags  = "corelink-flags"
    config = "corelink-config"
  }
}

variable "create_global_do_host" {
  type        = bool
  description = "Whether to declare the global DO host Worker script."
  default     = true
}
