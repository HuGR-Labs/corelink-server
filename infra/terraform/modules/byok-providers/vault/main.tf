# byok-providers/vault — CoreLink-side AppRole template for customer-hosted Vault.
#
# Provisions in CoreLink's OWN Vault (a dedicated namespace per customer):
#   * Policy granting transit/encrypt + transit/decrypt on a single key.
#   * AppRole with the policy attached.
#
# Customer-hosted Vault is OUT OF SCOPE — customer creates their own AppRole
# in their Vault that grants CoreLink access. This module is for the
# CoreLink-side mirror that orchestrates per-tenant transit envelopes.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    vault = {
      source  = "hashicorp/vault"
      version = "~> 4.3"
    }
  }
}

locals {
  policy_name = "corelink-byok-${var.customer_tenant_id}"
  role_name   = "corelink-byok-${var.customer_tenant_id}"
  namespace   = "corelink/byok/${var.customer_tenant_id}"
  policy_hcl  = <<-HCL
    path "transit/encrypt/${var.transit_key_name}" {
      capabilities = ["update"]
    }
    path "transit/decrypt/${var.transit_key_name}" {
      capabilities = ["update"]
    }
  HCL
}

resource "vault_policy" "byok" {
  name   = local.policy_name
  policy = local.policy_hcl
}

resource "vault_approle_auth_backend_role" "byok" {
  backend        = var.approle_backend_path
  role_name      = local.role_name
  token_policies = [vault_policy.byok.name]
  token_ttl      = var.token_ttl_seconds
  token_max_ttl  = var.token_max_ttl_seconds
  secret_id_ttl  = var.secret_id_ttl_seconds
}
