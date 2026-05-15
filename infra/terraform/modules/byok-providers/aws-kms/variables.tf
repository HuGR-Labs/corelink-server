variable "customer_tenant_id" {
  type        = string
  description = "CoreLink-internal tenant identifier (e.g. tenant-acme)."
}

variable "customer_kms_key_arns" {
  type        = list(string)
  description = "List of customer-owned KMS key ARNs that CoreLink is permitted to use."

  validation {
    condition     = length(var.customer_kms_key_arns) > 0
    error_message = "At least one customer KMS key ARN is required."
  }
}

variable "corelink_principal_arn" {
  type        = string
  description = "ARN of the CoreLink runtime principal that assumes this role (e.g. arn:aws:iam::<corelink-acct>:role/corelink-runtime)."
}

variable "external_id" {
  type        = string
  description = "STS ExternalId used to harden the trust relationship (per-tenant random)."
  sensitive   = true
}
