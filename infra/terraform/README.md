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
    corelink-region/        # reusable per-region resource module
  environments/
    staging/                # active pre-GA composition
    production/             # *.example template; activate post-GA only
  regions/
    wnam/                   # independent root + state key
    enam/                   # independent root + state key
    weur/                   # independent root + state key
    sam/                    # independent root + state key
  main.tf                   # legacy root (WI-S13-004); kept for backwards-compat
```

The drift workflow runs one root under `regions/<region>/` per matrix entry.
Each root contains exactly one `corelink-region` module and declares an S3
backend with `use_lockfile = true`. This prevents a plan for one region from
loading another region's resources or state. The legacy root is not used by
the regional drift job.

## Backend setup

Regional state is stored in one Cloudflare R2 bucket under four distinct keys:

| Region | State key | Lock object |
|---|---|---|
| `wnam` | `corelink/wnam/terraform.tfstate` | `corelink/wnam/terraform.tfstate.tflock` |
| `enam` | `corelink/enam/terraform.tfstate` | `corelink/enam/terraform.tfstate.tflock` |
| `weur` | `corelink/weur/terraform.tfstate` | `corelink/weur/terraform.tfstate.tflock` |
| `sam`  | `corelink/sam/terraform.tfstate`  | `corelink/sam/terraform.tfstate.tflock` |

Terraform 1.11.4's native S3 lockfile is enabled in every regional root.
The R2 token must have Object Read & Write permission on the state bucket so
Terraform can create and remove the `.tflock` object. The bucket, endpoint,
and credentials are runtime inputs; no backend config file or credential is
created in the checkout.

The drift workflow fails before `init` when any of these are absent:
`TF_BACKEND_BUCKET`, `TF_BACKEND_ENDPOINT`,
`TF_BACKEND_ACCESS_KEY_ID`, or `TF_BACKEND_SECRET_ACCESS_KEY`. It passes the
non-secret bucket and endpoint to `terraform init` and exposes the two token
parts only through `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.

The repository cannot prove R2 lock contention without the provisioned bucket
and scoped token. That integration check remains a release blocker and must
record two concurrent operations against the exact R2 endpoint before this
workflow is treated as a live drift signal.

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

Runtime credentials are environment variables only; no secret is stored in
Terraform configuration, backend blocks, command output, or plan artifacts.
The drift workflow keeps Cloudflare provider authentication separate from the
R2 backend token:

- Cloudflare provider: `CLOUDFLARE_API_TOKEN` from the dedicated read-only
  `CF_TERRAFORM_DRIFT_API_TOKEN` repository secret.
- R2 backend: `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` mapped from
  `TF_BACKEND_ACCESS_KEY_ID` and `TF_BACKEND_SECRET_ACCESS_KEY`.
- Backend location: `TF_BACKEND_BUCKET` and `TF_BACKEND_ENDPOINT`.

Other environments may use their documented OIDC or workload identity
bindings; those credentials are not reused for Terraform state access.

## Cross-links

- [`docs/internal/secrets-checklist.md`](../../docs/internal/secrets-checklist.md)
- [`specs/_runbooks/RB-TERRAFORM-DRIFT.md`](../../specs/_runbooks/RB-TERRAFORM-DRIFT.md)
- [`specs/_runbooks/RB-FM-206.md`](../../specs/_runbooks/RB-FM-206.md)
- [`ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §2 R-2 (wiring) + §6 R-6
  (sustained staging).
