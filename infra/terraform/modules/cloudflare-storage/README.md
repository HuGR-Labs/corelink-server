# Module: `cloudflare-storage`

Composes per-region Cloudflare storage/compute via the existing
`corelink-region` module and adds globally-scoped Workers KV namespaces and
a Durable-Object host Worker.

Provisions per region:

- R2 bucket `corelink-cas-{region}` (delegated).
- D1 database `corelink-meta-{region}` (delegated).
- Workers KV namespace `corelink-session-{region}` (delegated).
- Region DO Worker script + DNS + Worker route (delegated).

Adds globally:

- Workers KV namespaces (`corelink-flags`, `corelink-config`) used for
  feature flags / config snapshots that intentionally need global reach.
- Optional DO host Worker script (`corelink-do-host-<env>`) that exposes
  DO classes for sessions / rate-limit.

## Usage

```hcl
module "cf_storage" {
  source = "../../modules/cloudflare-storage"

  cf_account_id = var.cf_account_id
  cf_zone_id    = var.cf_zone_id
  environment   = "staging"

  regions = {
    wnam = { r2_location_hint = "wnam", d1_location = "wnam", do_jurisdiction = "us"   }
    enam = { r2_location_hint = "enam", d1_location = "enam", do_jurisdiction = "none" }
    weur = { r2_location_hint = "weur", d1_location = "weur", do_jurisdiction = "eu"   }
    sam  = { r2_location_hint = "sam",  d1_location = "sam",  do_jurisdiction = "none" }
  }
}
```

## Outputs

| Output | Description |
|---|---|
| `region_r2_bucket_names` | Map region → R2 bucket name. |
| `region_d1_database_ids` | Map region → D1 ID. |
| `region_kv_namespace_ids` | Map region → session KV ID. |
| `global_kv_namespace_ids` | Map logical → global KV ID. |
| `global_do_host_worker_name` | Global DO host Worker script name. |

## Cross-links

- WI-S14-001 / WI-S14-002 — region module + insert checks.
- `infra/terraform/regions/*` — historical per-region instantiation (kept
  for backwards compatibility with WI-S14 sealed work; new environments
  should call this module instead).
- `RB-TERRAFORM-DRIFT.md` — drift triage.
