# Atomicity + dedup gates — consumer run

- Corpus in: 14
- Atomicity violations: 5 NEEDS_SPLIT + 0 FRAGMENT
- Dedup violations: 0
- Survivors: 9

## NEEDS_SPLIT
- `region-enum-variants-jurisdiction` — It bundles a reference (the Region enum's variants and their R2/D1 region-code mappings) with an independent rule (Weur mandates DO jurisdiction = "eu"), each of which stands alone.
  - proposed: `region-enum-variants-jurisdiction` — In corelink-server the `Region` enum (corelink-region/src/region.rs) defines variants Wnam (us-west), Enam (us-east), We
  - proposed: `weur-mandates-eu-do-jurisdiction` — The Weur region variant mandates a Durable Object jurisdiction of "eu".
- `oci-blobstore-chunked-upload` — The discovered fact (BlobStore's chunked-upload protocol resists the CAS handler SPI) is independent from the routine clean mappings (ManifestKvStore→KvBackend, TenantResolver→PatValidator), which stand alone as their own reference unit.
  - proposed: `oci-blobstore-resists-cas-handler-spi` — In corelink-server, the OCI adapter's `BlobStore` port uses a chunked upload protocol (open_upload, append_chunk, finali
  - proposed: `oci-adapter-port-spi-mappings` — In corelink-server's OCI adapter, `ManifestKvStore` maps to the `KvBackend` SPI and `TenantResolver` maps to `PatValidat
- `secrecy-per-crate-not-workspace` — It bundles a dependency-management fact (secrecy pinned per-crate rather than via workspace.dependencies) with a separate API-version fact (corelink-core uses the 0.10 ExposeSecret/SecretString API), each of which stands alone.
  - proposed: `secrecy-per-crate-not-workspace` — In corelink-server, `secrecy` is pinned to `"0.10"` directly in each crate rather than via `workspace = true`, so it is 
  - proposed: `corelink-core-uses-secrecy-0-10-api` — corelink-core uses the secrecy 0.10 `ExposeSecret`/`SecretString` API.
- `secretwrap-expose-returns-str` — It bundles a reference fact about expose()'s signature with an independent decision-rule about preferring it to avoid a secrecy dependency, each of which stands alone.
  - proposed: `secretwrap-expose-returns-str` — In corelink-core (corelink-core/src/types/secret.rs), `SecretWrap::expose()` returns `&str` directly.
  - proposed: `prefer-secretwrap-expose-over-secrecy` — Prefer `SecretWrap::expose()` over `secrecy::ExposeSecret::expose_secret` so crates avoid a direct `secrecy` dependency.
- `corelink-adapter-host-crate-purpose` — It combines the crate's bridging purpose (a reference/concept unit) with an independent 500-LOC-per-file size convention (a rule), each of which stands alone.
  - proposed: `corelink-adapter-host-crate-purpose` — In corelink-server, the `corelink-adapter-host` crate (Wave 35) bridges the Wave-34 adapter ports (cargo/brew/oci/npm/pi
  - proposed: `corelink-adapter-host-files-under-500-loc` — The `corelink-adapter-host` source files (lib.rs, cargo.rs, brew.rs, oci.rs, npm.rs, pip.rs) are each kept under 500 LOC

## FRAGMENT

## DUPLICATES