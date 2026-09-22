# WI-S14-001 / #1721 — ENAM has an independent remote state object.
# Backend credentials and bucket endpoint are supplied by the drift workflow at runtime.
# R2 native S3 lock files are enabled; no -lock=false path is permitted.
terraform {
  required_version = ">= 1.11.0, < 2.0.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52.0"
    }
  }

  backend "s3" {
    region                      = "auto"
    key                         = "corelink/enam/terraform.tfstate"
    use_lockfile                = true
    use_path_style              = true
    skip_s3_checksum            = true
    skip_credentials_validation = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_metadata_api_check     = true
  }
}
