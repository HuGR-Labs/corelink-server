# Module group: `byok-providers/`

Provisions the **CoreLink-side** IAM / role / policy / app-role needed by
each supported BYOK provider so that CoreLink can call the customer's CMK
on demand. **Customer-side CMKs are NOT provisioned here** — customers
own and manage their own keys (that is the whole point of BYOK).

| Sub-module | Provider | Resources |
|---|---|---|
| [`aws-kms/`](./aws-kms/) | AWS KMS | IAM role (assumed by CoreLink), trust policy for customer cross-account, inline policy granting `kms:Encrypt`/`Decrypt`/`GenerateDataKey`/`DescribeKey` on customer ARNs |
| [`gcp-kms/`](./gcp-kms/) | Google Cloud KMS | GCP service account (CoreLink-side), Workload-Identity-Pool federation binding, key-user IAM binding template |
| [`azure-kv/`](./azure-kv/) | Azure Key Vault | Azure AD app registration (CoreLink-side), service principal, key permissions assignment template |
| [`vault/`](./vault/) | HashiCorp Vault | AppRole role + policy template; customer pulls AppRole creds out of band |

## Hard rules

- **No customer credentials in code.** All inputs are scoped to CoreLink's
  own cloud accounts.
- All sub-modules accept `customer_tenant_id` (CoreLink-internal tenant
  identifier) and `customer_principal_id` (customer's AWS account / GCP
  project / Azure tenant / Vault accessor) and produce ARNs / IDs that the
  CoreLink BYOK control plane stores per tenant.
- Sub-modules are intentionally independent — each project loads only the
  providers it needs.

## Composition pattern

```hcl
module "byok_aws_acme" {
  source = "../../modules/byok-providers/aws-kms"

  customer_tenant_id    = "tenant-acme"
  customer_aws_account  = "123456789012"
  customer_kms_key_arns = ["arn:aws:kms:us-east-1:123456789012:key/abcd-..."]
}

module "byok_gcp_globex" {
  source = "../../modules/byok-providers/gcp-kms"

  customer_tenant_id     = "tenant-globex"
  customer_gcp_project   = "globex-prod"
  customer_kms_key_names = ["projects/globex-prod/locations/global/keyRings/corelink/cryptoKeys/cmk1"]
}
```

## Cross-links

- `specs/_runbooks/RB-BYOK-*` — BYOK operational runbooks.
- `docs/internal/secrets-checklist.md` rows 23-46 — provider credentials
  matrix.
- WI-S14-005 / WI-S14-006 — BYOK control plane.
