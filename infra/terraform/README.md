# `infra/terraform/` — CoreLink Infrastructure as Code

Modular Terraform configuration for the CoreLink production stack on
Cloudflare + multi-cloud BYOK.

## Directory layout

```
infra/terraform/
  modules/
    cloudflare-base/        # account/zone/DNS/Worker-route/Pages
    cloudflare-storage/     # multi-region R2/D1/KV/DO + global KV
    cloudflare-secrets/     # wrangler-secret-put wrapper
    byok-providers/
      aws-kms/              # AWS IAM role + policy (CoreLink-side)
      gcp-kms/              # GCP SA + WIF (CoreLink-side)
      azure-kv/             # AAD app + SP + secret (CoreLink-side)
      vault/                # AppRole + transit policy (CoreLink-side)
    corelink-region/        # legacy per-region module (WI-S14-001; kept)
  environments/
    staging/                # active pre-GA composition
    production/             # *.example template; activate post-GA only
  regions/                  # historical per-region instantiation (WI-S14)
  main.tf                   # legacy root (WI-S13-004); kept for backwards-compat
```

New environments should compose `modules/cloudflare-base` +
`modules/cloudflare-storage` + `modules/cloudflare-secrets`. The legacy
`main.tf` + `regions/` stay until WI-S14 is fully migrated.

## Backend setup

State is stored in an S3-compatible backend with locking:

| Env | Bucket | Lock table |
|---|---|---|
| staging    | `corelink-tfstate-staging`    | `corelink-tfstate-lock`      |
| production | `corelink-tfstate-production` | `corelink-tfstate-lock-prod` |

Encryption: AES-256 server-side; production additionally requires
MFA-delete and a bucket policy denying `s3:DeleteObject` without MFA.

Real backend config (bucket name + region + DynamoDB lock table) is passed
to `terraform init` via `-backend-config=backend-<env>.hcl`. The real
`backend-<env>.hcl` is git-ignored; an `*.example` template is committed.

```bash
cd infra/terraform/environments/staging
cp backend-staging.hcl.example backend-staging.hcl   # then fill in real values
terraform init -backend-config=backend-staging.hcl
```

R2 + KV backend (Cloudflare-native) is an option once Terraform's S3
backend supports R2 with KV locks (tracked: hashicorp/terraform#34859).
Until then we use AWS S3 + DynamoDB for staging and a separate
locked-down S3 bucket in a CoreLink-owned AWS account for production.

## Workspaces

Each environment is its own root module (separate state file); we do NOT
use `terraform workspace` to multiplex envs because workspace mistakes
can mix prod/staging state. Hard isolation only.

## Plan-vs-apply protocol

Auto-apply is FORBIDDEN. The CI workflow runs `terraform plan` only.
`terraform apply` runs via a separate manual workflow that:

1. Requires PR with explicit plan-output attached as artifact.
2. Requires CODEOWNERS approval (SRE Lead + Security Lead for production).
3. Re-runs `terraform plan` against current state and aborts if the new
   plan differs from the approved plan (RB-FM-206 §3).
4. Sets `-lock-timeout=5m`; refuses to proceed if the lock cannot be
   acquired cleanly.

See `specs/_runbooks/RB-FM-206.md` and `specs/_runbooks/RB-TERRAFORM-DRIFT.md`.

## Quality gates (CI)

`.github/workflows/terraform-lint.yml` runs on every PR touching
`infra/terraform/**`:

- `terraform fmt -recursive -check`
- `terraform validate` (per environment)
- `tflint` with the Cloudflare/AWS/GCP/Azure rulesets
- `tfsec` static analysis with SARIF upload

All third-party actions are SHA-pinned per `WI-S01-007` supply-chain rule.

Daily drift detection: `.github/workflows/terraform-drift.yml` (existing,
WI-S13-004) runs `terraform plan -detailed-exitcode` per region; non-empty
diff opens a `RB-TERRAFORM-DRIFT` incident.

## Secrets

No literal secrets in code. Every secret pushed by `cloudflare-secrets`
has a row in `docs/internal/secrets-checklist.md`; CI fails if a key is
pushed that isn't matrixed.

OIDC-bound credentials only:

- Cloudflare: `CF_API_TOKEN` injected via GitHub OIDC → CF Workers OIDC.
- AWS: `AWS_ROLE_ARN` assumed via `aws-actions/configure-aws-credentials`
  using GitHub OIDC.
- GCP: `google-github-actions/auth` with Workload Identity Federation.
- Azure: `azure/login@v2` with federated credentials.
- Vault: GitHub OIDC → Vault JWT auth backend.

## Cross-links

- [`docs/internal/secrets-checklist.md`](../../docs/internal/secrets-checklist.md)
- [`specs/_runbooks/RB-TERRAFORM-DRIFT.md`](../../specs/_runbooks/RB-TERRAFORM-DRIFT.md)
- [`specs/_runbooks/RB-FM-206.md`](../../specs/_runbooks/RB-FM-206.md)
- [`ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §2 R-2 (wiring) + §6 R-6
  (sustained staging).
