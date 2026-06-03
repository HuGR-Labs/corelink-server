# WI-S14-001 — Root-module Terraform settings for the per-region composition.
#
# The region invocations (wnam/enam/weur/sam.tf) fan out to the
# modules/corelink-region child module, which provisions the Cloudflare R2 /
# D1 / KV / DO / DNS resources. This block pins the toolchain + provider
# versions for the regions root module to match the rest of the tree
# (environments/staging, modules/*), per WI-S01-007 (all versions pinned).
#
# Auto-apply FORBIDDEN — see RB-FM-206 / RB-TERRAFORM-DRIFT.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52.0"
    }
  }
}
