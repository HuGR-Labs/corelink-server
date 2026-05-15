# environments/staging — Variables.
# All TF_VAR_* sensitive inputs come from an OIDC-bound vault read at
# apply time; never literal in code.

variable "cf_account_id" {
  type        = string
  description = "Cloudflare account ID for the staging account."
  sensitive   = true
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID for the staging zone."
  sensitive   = true
}

variable "zone_name" {
  type        = string
  description = "Apex zone (e.g. staging.corelink.dev)."
  default     = "staging.corelink.dev"
}

variable "dkim_records" {
  type        = map(string)
  description = "DKIM selector hostname → CNAME target."
  default     = {}
}

# ---- Sensitive secrets ----

variable "tf_secret_stripe_test" {
  type      = string
  sensitive = true
}

variable "tf_secret_stripe_webhook" {
  type      = string
  sensitive = true
}

variable "tf_secret_clerk" {
  type      = string
  sensitive = true
}

variable "tf_secret_clerk_pub" {
  type      = string
  sensitive = true
}

variable "tf_secret_pagerduty_routing" {
  type      = string
  sensitive = true
}

variable "tf_secret_slack_sev1" {
  type      = string
  sensitive = true
}

variable "tf_secret_slack_sev2" {
  type      = string
  sensitive = true
}

variable "tf_secret_hubspot" {
  type      = string
  sensitive = true
}
