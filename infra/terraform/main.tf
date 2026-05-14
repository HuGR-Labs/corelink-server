# WI-S13-004 — Terraform IaC baseline for CoreLink infrastructure.
# Auto-apply FORBIDDEN — see RB-FM-206.
# All changes require PR + CODEOWNERS review + manual apply workflow + dual-approval (WI-S13-002).

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52.0"
    }
  }

  backend "s3" {
    # Backend config injected via -backend-config=backend-<region>.hcl at init time.
    # See .github/workflows/terraform-drift.yml for per-region init pattern.
    # Region-specific state files prevent cross-region state corruption.
  }
}

# Provider configuration — credentials via OIDC environment variables.
# No long-lived API tokens in code (WI-S13-004 §6.1 OIDC-bound only).
provider "cloudflare" {
  # CF_API_TOKEN or CF_CLIENT_ID + CF_CLIENT_SECRET injected at runtime via OIDC.
}

# -----------------------------------------------------------------------
# CoreLink Workers per region
# Drift detection: daily plan catches any console-level modifications.
# lifecycle.ignore_changes: acceptable drift patterns (see RB-FM-206 §10).
# -----------------------------------------------------------------------

# Placeholder — real Worker resources declared per sprint rollout.
# WI-S13-004 establishes drift detection framework; resources added
# incrementally as infrastructure expands.

locals {
  # WI-S14-001: canonical 4-region set (replaces S-13 placeholder geographic names).
  # Maps: wnam=us-west, enam=us-east, weur=eu-west, sam=sa-east.
  # Per-region resources defined in infra/terraform/regions/{wnam,enam,weur,sam}.tf
  regions = ["wnam", "enam", "weur", "sam"]
}
