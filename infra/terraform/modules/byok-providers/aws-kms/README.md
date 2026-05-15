# byok-providers / aws-kms

CoreLink-side AWS IAM role + inline policy for a single AWS-KMS BYOK
customer. Customer-side CMK + key-policy is provisioned by the customer.

Variables:

| Var | Purpose |
|---|---|
| `customer_tenant_id` | CoreLink-internal tenant identifier. |
| `customer_kms_key_arns` | Customer-owned CMK ARNs the role is allowed to use. |
| `corelink_principal_arn` | CoreLink runtime principal allowed to assume the role. |
| `external_id` | STS ExternalId for trust-policy hardening. |

Outputs: `role_arn`, `role_name`.

Operational flow:

1. Customer creates CMK in their AWS account.
2. Customer sends ARN(s) + their AWS account number to CoreLink ops.
3. Ops runs `terraform apply` of an environment composition referencing
   this module → produces `role_arn`.
4. Ops hands `role_arn` + `external_id` to customer.
5. Customer attaches matching key-policy on their CMK granting decrypt to
   `role_arn` conditioned on `aws:PrincipalTag/ExternalId == external_id`.
