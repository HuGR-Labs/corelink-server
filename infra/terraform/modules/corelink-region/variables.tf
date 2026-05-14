# WI-S14-001 — Terraform module `corelink-region` variables.
# Per-region inputs; validated at plan time.
# Auto-apply FORBIDDEN — see RB-FM-206 + RB-region.

# ---------------------------------------------------------------------------
# Required per-region inputs
# ---------------------------------------------------------------------------

variable "region_name" {
  type        = string
  description = "Region identifier (wnam/enam/weur/sam)"

  validation {
    condition     = contains(["wnam", "enam", "weur", "sam"], var.region_name)
    error_message = "region_name must be one of: wnam, enam, weur, sam"
  }
}

variable "r2_location_hint" {
  type        = string
  description = "R2 bucket location hint (wnam/enam/weur/sam mapped to CF storage regions)"

  validation {
    condition     = contains(["wnam", "enam", "weur", "sam"], var.r2_location_hint)
    error_message = "r2_location_hint must be one of: wnam, enam, weur, sam"
  }
}

variable "d1_location" {
  type        = string
  description = "D1 instance primary location hint (wnam/enam/weur/sam)"

  validation {
    condition     = contains(["wnam", "enam", "weur", "sam"], var.d1_location)
    error_message = "d1_location must be one of: wnam, enam, weur, sam"
  }
}

variable "do_jurisdiction" {
  type        = string
  description = "DO jurisdictional restriction (none/eu/us). WEUR MUST be eu. (FF-HR-003 Schrems II)"

  validation {
    condition     = contains(["none", "eu", "us"], var.do_jurisdiction)
    error_message = "do_jurisdiction must be one of: none, eu, us"
  }
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID for {region}.api.corelink.dev"
}

variable "cf_account_id" {
  type        = string
  description = "Cloudflare account ID. Injected via TF_VAR_cf_account_id env var (OIDC-bound; no hard-coded tokens)."
  sensitive   = true
}

# ---------------------------------------------------------------------------
# Optional flags
# ---------------------------------------------------------------------------

variable "environment" {
  type        = string
  description = "Deployment environment: staging | production"
  default     = "staging"

  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "environment must be one of: staging, production"
  }
}

variable "kv_namespace_title_prefix" {
  type        = string
  description = "KV namespace title prefix (default: corelink-session). Overrideable for disaster-recovery testing."
  default     = "corelink-session"
}
