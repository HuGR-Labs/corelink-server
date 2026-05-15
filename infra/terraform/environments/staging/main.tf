# environments/staging — Pre-GA staging composition.
#
# Wires cloudflare-base + cloudflare-storage + cloudflare-secrets into a
# single applicable unit for the staging environment.
#
# Auto-apply FORBIDDEN. See:
#   * specs/_runbooks/RB-FM-206.md
#   * specs/_runbooks/RB-TERRAFORM-DRIFT.md
#   * specs/work/WI-S13-002 dual-approval gate.
#
# Backend: state lives in an S3-compatible backend (R2 + DynamoDB-style
# lock or AWS S3 + DynamoDB). The backend config is supplied via
# `-backend-config=backend-staging.hcl` at `terraform init` time; the file
# itself is git-ignored. See ../../README.md §Backend setup.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52.0"
    }
  }

  backend "s3" {
    # Injected at init:
    #   bucket         = "corelink-tfstate-staging"
    #   key            = "infra/terraform/staging.tfstate"
    #   region         = "us-east-1"
    #   dynamodb_table = "corelink-tfstate-lock"
    #   encrypt        = true
  }
}

provider "cloudflare" {
  # CF_API_TOKEN injected via OIDC at runtime — no long-lived secret.
}

# ---------------------------------------------------------------------------
# Zone-level resources
# ---------------------------------------------------------------------------

module "cf_base" {
  source = "../../modules/cloudflare-base"

  cf_account_id = var.cf_account_id
  cf_zone_id    = var.cf_zone_id
  zone_name     = var.zone_name
  environment   = "staging"

  create_email_records = true
  dkim_records         = var.dkim_records
}

# ---------------------------------------------------------------------------
# Per-region R2 / D1 / KV / DO + global KV
# ---------------------------------------------------------------------------

module "cf_storage" {
  source = "../../modules/cloudflare-storage"

  cf_account_id = var.cf_account_id
  cf_zone_id    = var.cf_zone_id
  environment   = "staging"

  regions = {
    wnam = { r2_location_hint = "wnam", d1_location = "wnam", do_jurisdiction = "us" }
    enam = { r2_location_hint = "enam", d1_location = "enam", do_jurisdiction = "none" }
    weur = { r2_location_hint = "weur", d1_location = "weur", do_jurisdiction = "eu" }
    sam  = { r2_location_hint = "sam", d1_location = "sam", do_jurisdiction = "none" }
  }
}

# ---------------------------------------------------------------------------
# Worker secrets for the staging Worker.
# Values arrive via TF_VAR_<...> sensitive variables that the apply runner
# sources from an OIDC-bound vault read. Plaintext NEVER lives in Terraform
# code.
# ---------------------------------------------------------------------------

module "cf_secrets" {
  source = "../../modules/cloudflare-secrets"

  worker_name = "corelink-server"
  environment = "staging"

  secrets = {
    STRIPE_SECRET_KEY_TEST        = var.tf_secret_stripe_test
    STRIPE_WEBHOOK_SECRET         = var.tf_secret_stripe_webhook
    CLERK_SECRET_KEY              = var.tf_secret_clerk
    CLERK_PUBLISHABLE_KEY         = var.tf_secret_clerk_pub
    PAGERDUTY_ROUTING_KEY         = var.tf_secret_pagerduty_routing
    SLACK_WEBHOOK_URL_ALERTS_SEV1 = var.tf_secret_slack_sev1
    SLACK_WEBHOOK_URL_ALERTS_SEV2 = var.tf_secret_slack_sev2
    HUBSPOT_PRIVATE_APP_TOKEN     = var.tf_secret_hubspot
  }
}

# ---------------------------------------------------------------------------
# Outputs (consumed by downstream verification scripts).
# ---------------------------------------------------------------------------

output "api_apex_fqdn" {
  value = module.cf_base.api_apex_fqdn
}

output "region_r2_buckets" {
  value = module.cf_storage.region_r2_bucket_names
}

output "secret_keys_pushed" {
  value = module.cf_secrets.secret_keys_pushed
}
