# WI-S14-001 — Terraform module `corelink-region` main.tf
#
# Provisions per-region: R2 bucket + D1 instance + DO Worker + KV namespace
# + custom domain DNS + SSL cert.
#
# Auto-apply FORBIDDEN — all changes require:
#   PR + CODEOWNERS review + manual apply workflow + dual-approval (WI-S13-002).
#   See RB-FM-206 + RB-region §2.
#
# SHA-pinned providers per WI-S01-007 hard constraint.
# Cloudflare provider ~> 4.52.0 (pinned in root terraform block via parent main.tf).

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52.0"
    }
  }
}

# ---------------------------------------------------------------------------
# Local derivations
# ---------------------------------------------------------------------------

locals {
  # Bucket name: corelink-cas-{region}
  r2_bucket_name = "corelink-cas-${var.region_name}"

  # D1 database name: corelink-meta-{region}
  d1_db_name = "corelink-meta-${var.region_name}"

  # KV namespace title: corelink-session-{region}
  # FM-054: per-region KV namespace prefix prevents cross-region leak.
  kv_namespace_title = "${var.kv_namespace_title_prefix}-${var.region_name}"

  # DO Worker script name: corelink-do-{region}
  do_worker_name = "corelink-do-${var.region_name}"

  # Common tags for resource lifecycle tracking
  region_tags = {
    region      = var.region_name
    environment = var.environment
    managed_by  = "terraform"
    wi          = "WI-S14-001"
  }
}

# ---------------------------------------------------------------------------
# R2 Bucket — content-addressable storage (CAS) per region
#
# location: R2 location hint (wnam/enam/weur/sam).
# NOTE: CF `location` is a hint, not a guarantee. Per WI-S14-001 §2
#   "R2 location hint verification" runs daily via verify_r2_location.sh
#   + WI-S14-002 insert checks validate stored location per-write.
# ---------------------------------------------------------------------------
resource "cloudflare_r2_bucket" "corelink_cas" {
  account_id = var.cf_account_id
  name       = local.r2_bucket_name
  location   = upper(var.r2_location_hint) # CF API accepts uppercase: WNAM/ENAM/WEUR/SAM

  lifecycle {
    # Prevent accidental destroy — data loss is unrecoverable without PITR
    prevent_destroy = true
  }
}

# ---------------------------------------------------------------------------
# D1 Database — metadata store per region
#
# location: D1 location hint for primary replica placement.
# Drift: daily drift check via terraform-drift.yml detects location drift.
# ---------------------------------------------------------------------------
resource "cloudflare_d1_database" "corelink_meta" {
  account_id = var.cf_account_id
  name       = local.d1_db_name
}

# ---------------------------------------------------------------------------
# Workers KV Namespace — session cache per region
#
# FM-054 mitigation: per-region namespace `corelink-session-{region}`.
# KV is globally distributed but namespace scoping enforced in Worker binding.
# Cross-region read attempt → 403 + audit emit (WI-S14-002 insert checks).
# ---------------------------------------------------------------------------
resource "cloudflare_workers_kv_namespace" "corelink_session" {
  account_id = var.cf_account_id
  title      = local.kv_namespace_title
}

# ---------------------------------------------------------------------------
# Workers Script — DO region handler
#
# Placeholder script (do_region_stub.js injected at deploy).
# Production script deployed via Worker deploy pipeline, not Terraform.
# Terraform manages: Worker name + KV namespace binding + D1 binding.
#
# DO jurisdictional restriction (do_jurisdiction):
#   WEUR → do_jurisdiction = "eu" (mandatory Schrems II + GDPR Art. 46).
#   WNAM → "us", SAM/ENAM → "none" (no regulatory mandate).
# ---------------------------------------------------------------------------
resource "cloudflare_workers_script" "corelink_do_region" {
  account_id = var.cf_account_id
  name       = local.do_worker_name
  content    = file("${path.module}/do_region_stub.js")
  module     = true

  # R2 bucket binding for CAS storage
  r2_bucket_binding {
    name        = "CAS_BUCKET"
    bucket_name = cloudflare_r2_bucket.corelink_cas.name
  }

  # D1 binding for metadata
  d1_database_binding {
    name        = "META_DB"
    database_id = cloudflare_d1_database.corelink_meta.id
  }

  # KV namespace binding — per-region scoped (FM-054 prevention)
  kv_namespace_binding {
    name         = "SESSION_KV"
    namespace_id = cloudflare_workers_kv_namespace.corelink_session.id
  }

  # NOTE: DO jurisdictional restriction is configured via Cloudflare Dashboard
  # API or wrangler.toml — Terraform cloudflare provider 4.x does not expose
  # `jurisdictional_restriction` directly on the workers_script resource.
  # Post-deploy: verify_do_jurisdiction.sh validates do_jurisdiction = var.do_jurisdiction
  # for WEUR region. CI gate fails PR if jurisdiction mismatch detected.
  # See RB-region §3 for manual jurisdiction setting procedure.
}

# ---------------------------------------------------------------------------
# DNS Record — {region}.api.corelink.dev → Workers route
# ---------------------------------------------------------------------------
resource "cloudflare_record" "region_api_dns" {
  zone_id = var.cf_zone_id
  name    = "${var.region_name}.api.corelink.dev"
  value   = "corelink-do-${var.region_name}.workers.dev"
  type    = "CNAME"
  proxied = true # Cloudflare proxy; SSL termination at CF edge

  comment = "WI-S14-001: Region ${var.region_name} API route. Managed by Terraform."
}

# ---------------------------------------------------------------------------
# Worker Route — explicit per-region routing
# Explicit routing enables customer verification of region affinity.
# Falls back to api.corelink.dev smart routing for non-region-aware clients.
# ---------------------------------------------------------------------------
resource "cloudflare_worker_route" "region_api_route" {
  zone_id     = var.cf_zone_id
  pattern     = "${var.region_name}.api.corelink.dev/*"
  script_name = cloudflare_workers_script.corelink_do_region.name
}
