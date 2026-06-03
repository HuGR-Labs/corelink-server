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

  required_providers {
    # This module only drives wrangler via null_resource + local-exec; the
    # null provider is the sole provider dependency. Pinned per WI-S01-007
    # (all provider versions constrained).
    null = {
      source  = "hashicorp/null"
      version = "~> 3.2"
    }
  }
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
  # Iterate over secret NAMES only — never the sensitive map itself.
  # Terraform forbids a sensitive value (or anything derived from one) as a
  # for_each argument, because the resulting instance keys surface in plan
  # output and could leak the secret. The NAMES are not sensitive, so we strip
  # the sensitivity off the key set with `nonsensitive(toset(keys(...)))` and
  # look the actual (still-sensitive) VALUE up by key inside the resource body
  # via `var.secrets[each.key]`. The plaintext values therefore never enter
  # for_each / instance keys / plan output. (each.key == the secret name.)
  for_each = nonsensitive(toset(keys(var.secrets)))

  triggers = {
    worker_name = var.worker_name
    environment = var.environment
    secret_key  = each.key
    # Hash of the value so a change re-triggers. We hash the value rather
    # than store it so Terraform state never contains the plaintext. The
    # value is looked up by key (each.key is the secret NAME) so the
    # sensitive value never lands in for_each / instance keys.
    value_hash = sha256(var.secrets[each.key])
  }

  provisioner "local-exec" {
    interpreter = ["/bin/bash", "-c"]

    # Read the secret value from an env var; do NOT interpolate it on the
    # command line. The value is looked up by key (each.key is the secret
    # NAME) because for_each iterates over names, not the sensitive map — so
    # each.value would be the name, not the secret. The lookup keeps the
    # value sensitive and out of for_each / instance keys.
    environment = {
      SECRET_VALUE = var.secrets[each.key]
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
