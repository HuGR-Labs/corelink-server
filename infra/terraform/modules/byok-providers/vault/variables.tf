variable "customer_tenant_id" {
  type        = string
  description = "CoreLink-internal tenant identifier."
}

variable "transit_key_name" {
  type        = string
  description = "Name of the transit key the policy grants encrypt/decrypt on."
}

variable "approle_backend_path" {
  type        = string
  description = "Mount path of the AppRole auth backend in Vault."
  default     = "approle"
}

variable "token_ttl_seconds" {
  type        = number
  description = "Default token TTL (seconds)."
  default     = 1800 # 30min
}

variable "token_max_ttl_seconds" {
  type        = number
  description = "Maximum token TTL (seconds)."
  default     = 3600 # 1h
}

variable "secret_id_ttl_seconds" {
  type        = number
  description = "Secret ID TTL (seconds)."
  default     = 86400 # 24h
}
