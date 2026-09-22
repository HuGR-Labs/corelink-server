variable "bucket_name" {
  type        = string
  description = "Globally unique name for the new, dedicated S3 Object-Lock archive bucket."

  validation {
    condition     = length(trimspace(var.bucket_name)) >= 3
    error_message = "bucket_name must be a non-empty S3 bucket name."
  }
}

variable "default_retention_days" {
  type        = number
  description = "Security and Compliance-approved S3 Object Lock COMPLIANCE retention term in days."

  validation {
    condition     = var.default_retention_days >= 1 && floor(var.default_retention_days) == var.default_retention_days
    error_message = "default_retention_days must be a positive whole number."
  }
}

variable "object_key_prefix" {
  type        = string
  description = "Archive object-key prefix granted to the writer role, without a leading slash."
  default     = "audit/"

  validation {
    condition     = startswith(var.object_key_prefix, "audit/") && !startswith(var.object_key_prefix, "/")
    error_message = "object_key_prefix must start with audit/ and must not start with /."
  }
}

variable "writer_principal_arns" {
  type        = list(string)
  description = "Approved workload principals allowed to assume the narrowly scoped writer role."

  validation {
    condition     = length(var.writer_principal_arns) > 0 && alltrue([for arn in var.writer_principal_arns : startswith(arn, "arn:")])
    error_message = "writer_principal_arns must contain at least one ARN."
  }
}

variable "writer_role_name" {
  type        = string
  description = "Name of the dedicated immutable-archive writer role."
  default     = "corelink-audit-object-lock-writer"

  validation {
    condition     = length(trimspace(var.writer_role_name)) > 0
    error_message = "writer_role_name must not be empty."
  }
}

variable "tags" {
  type        = map(string)
  description = "Non-secret tags required by the approved AWS account policy."
  default     = {}
}
