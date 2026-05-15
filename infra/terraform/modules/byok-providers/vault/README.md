# byok-providers / vault

CoreLink-side Vault AppRole role + transit policy for a single Vault BYOK
customer. Customer-hosted Vault remains the source of truth; this module
mirrors the per-tenant access policy on the CoreLink-side Vault.

Variables: see `variables.tf`.

Outputs: `role_name`, `policy_name`.

Customer ops process:

1. CoreLink ops `terraform apply` → role + policy exist.
2. CoreLink ops fetch `role_id` / `secret_id` and store them via the
   `cloudflare-secrets` module under `VAULT_APPROLE_ROLE_ID_<tenant>` and
   `VAULT_APPROLE_SECRET_ID_<tenant>` (matrix rows in
   `docs/internal/secrets-checklist.md`).
