-- 0061_adapter_oci_kv.sql
--
-- OCI registry adapter (`/v2/*` + `/token`): durable, MUTABLE KV for
-- manifests + tag lists. Blob BYTES are immutable + content-addressed, so
-- they live in CAS via the 2-level moat (`adapter_cache_map`, migration
-- 0058); this table holds the mutable OCI metadata the adapter's
-- `ManifestKvStore` port needs:
--
--   oci_manifest:<repo>:<reference>   → manifest bytes (tag- or digest-ref)
--   oci_manifest_ct:<repo>:<reference>→ manifest content-type
--   oci_tags:<repo>                   → tag-list cache
--   oci_blob_index:<oci-digest>       → OCI-sha256-digest → CAS blob key
--
-- Per-tenant scoped via `tenant_id` (the adapter never trusts the path).
-- Values are hex-encoded TEXT (the D1 HTTP API is JSON-only), mirroring
-- `adapter_npm_meta` (migration 0059). `list_prefix` (GET
-- /v2/<repo>/tags/list derivation) is a left-prefix scan of the PK
-- `(tenant_id, kv_key)`, so no secondary index is required.
--
-- Additive-only (INV-AUTH-MIGRATION-ADDITIVE): pure CREATE ... IF NOT EXISTS.

CREATE TABLE IF NOT EXISTS adapter_oci_kv (
  tenant_id  TEXT    NOT NULL,
  kv_key     TEXT    NOT NULL,
  value_hex  TEXT    NOT NULL,
  updated_ms INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (tenant_id, kv_key)
);
