# byok-providers / azure-kv

CoreLink-side multi-tenant Azure AD app registration + SP + client secret
for a single Azure-Key-Vault BYOK customer. Customer admin consents the
app in their AAD tenant out of band and grants `get`/`unwrapKey`/`wrapKey`
on the target keys.

Variables: see `variables.tf`.

Outputs: `application_client_id`, `service_principal_object_id`,
`client_secret` (sensitive).

`client_secret` MUST be piped into `cloudflare-secrets` module immediately
(matrix row `BYOK_AZURE_CLIENT_SECRET_<tenant>`) and rotated per
`docs/internal/secrets-checklist.md`.
