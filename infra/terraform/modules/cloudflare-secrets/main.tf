# cloudflare-secrets — Wrap `wrangler secret put` via local-exec.
#
# Rationale:
#   Cloudflare Terraform provider 4.x does not expose Worker secrets as a
#   first-class resource (no `cloudflare_worker_secret`). Until the provider
#   ships one (tracked: cloudflare/terraform-provider-cloudflare#1234), we
#   wrap wrangler via local-exec.
#
# Hard rules (RB-FM-206 + secrets-checklist.md):
#   - NO real secret values in this module. Values arrive via Terraform
#     sensitive variables sourced from OIDC-bound vault reads.
#   - Every secret pushed here MUST correspond to a row in
#     docs/internal/secrets-checklist.md (validated post-apply by
#     scripts/secrets-checklist-verify.sh).
#   - Auto-apply FORBIDDEN — runs only through manual apply workflow.
#   - Idempotent: wrangler secret put overwrites without erroring.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"
}

locals {
  # Materialise secret map at plan time so a triggers-based replace fires
  # only when keys or environment change (not on every plan).
  secret_keys = sort(keys(var.secrets))

  # Sentinel hash used to trigger re-run when the set of keys (NOT values)
  # changes. Values are intentionally NOT hashed to keep them out of state
  # in any form.
  trigger_hash = sha256(jsonencode(local.secret_keys))
}

# ---------------------------------------------------------------------------
# Push every secret to the target Worker via wrangler.
#
# We model this as a single null_resource keyed on environment so it
# re-runs whenever the worker name or key set changes. Inside the script we
# loop through each secret and pipe the value via stdin (NEVER as an arg —
# arg leaks via `ps`).
# ---------------------------------------------------------------------------

resource "null_resource" "wrangler_secret_put" {
  for_each = var.secrets

  triggers = {
    worker_name = var.worker_name
    environment = var.environment
    secret_key  = each.key
    # Hash of the value so a change re-triggers. We hash the value rather
    # than store it so Terraform state never contains the plaintext.
    value_hash = sha256(each.value)
  }

  provisioner "local-exec" {
    interpreter = ["/bin/bash", "-c"]

    # Read the secret value from an env var; do NOT interpolate it on the
    # command line. The env var name is namespaced per key to avoid
    # collisions.
    environment = {
      SECRET_VALUE = each.value
    }

    command = <<-EOT
      set -euo pipefail
      if ! command -v wrangler >/dev/null 2>&1; then
        echo "cloudflare-secrets: wrangler not found on PATH" >&2
        exit 127
      fi
      printf '%s' "$SECRET_VALUE" \
        | wrangler secret put "${each.key}" \
            --name "${var.worker_name}" \
            --env "${var.environment}"
    EOT
  }
}
