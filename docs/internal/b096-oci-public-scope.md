# B-096 evidence — OCI `_public` network-effect scope

## Decision

The expansion claim is intentionally scoped to the executed surfaces: public pip
wheels/sdists and Homebrew bottles are shared, npm shares only unscoped metadata (not
tarball bytes), and OCI has the curated `_public` path documented below. It is not a
claim that CoreLink's native CAS, or the cache as a whole, deduplicates bytes across
tenants. The native CAS path remains tenant-isolated.

## What is actually admitted

The container-baked trust root has exactly six active, unique pins. The manifest was read
in full and every active token was checked as `sha256:` followed by 64 lowercase hex
characters (6 active / 6 unique):

| Manifest line | Kind | Provenance | Digest |
|---:|---|---|---|
| 93 | OCI layer blob | `docker.io/library/alpine:3.20` (amd64 rootfs layer) | `sha256:25f1d6b1951ac8eb3740558fe94cb83d377bdadf95fd9f98b50d2e1b96130471` |
| 96 | OCI image index | `docker.io/library/alpine:3.20` (multi-arch image index) | `sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc` |
| 97 | OCI image index | `docker.io/library/debian:12` (multi-arch image index) | `sha256:813017f3d62be4b5891a7acca6a01bdcd4b8513daa81b1ab99d3a50385b26931` |
| 98 | OCI image index | `docker.io/library/ubuntu:24.04` (multi-arch image index) | `sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517` |
| 99 | OCI image index | `docker.io/library/node:22-slim` (multi-arch image index) | `sha256:d649c27dae7ba0137b3cef5dd75baa422c08dc3d9e3fc0c23dfb172dc3cc6436` |
| 100 | OCI image index | `docker.io/library/python:3.12-slim` (multi-arch image index) | `sha256:2c941e860699f878900b0edc2403613c234d4b32eda3cc9fa7036991a2a63c4a` |

The six entries are roots, not six total stored objects. The five image-index pins can
server-side promote their digest-verified manifest/config/layer closure into `_public`.
The client-write predicate has only the digest, not a media type, and applies only to
OCI blob upload/finalize: an authenticated client that supplies bytes matching any
owner-pinned digest, including index-shaped bytes, can populate that already-authorized
blob slot. It cannot populate an unpinned slot or change bytes under a pin:
`finalize_upload` verifies the declared SHA-256 before the write. This does not extend to
`PUT /v2/<repo>/manifests/<reference>`: manifest and image-index JSON are validated and
stored in the tenant-scoped `ManifestKvStore`, never in `_public`. This is still a
curated OCI surface because the owner alone commits the eligible digest set, and the
resolver verifies the transitive closure before promotion.

The allowlist is embedded with `include_str!`, validated fail-closed, and has no runtime
mutation path. A malformed, truncated, tagged, uppercase, or non-hex active entry must
disable trust in the whole set rather than create a partially trusted set.

The module's introductory comment records the current WP-G M3 posture
(`public_base_allowlist.rs:18-22`): six pins are active, production's digest-only client
blob dedup gate is on, and manifest PUT remains tenant-scoped. The executable six-pin test
is the current population recorded here.

## Routing and isolation evidence

- Production has `OCI_PUBLIC_DEDUP_ENABLED = "1"` and `OCI_UPSTREAM_ON_MISS = "1"` in
  the base `prod` block and the four regional production blocks (`iad`, `sam`, `lhr`,
  `nrt`, `syd`). These are boot-read flags, so this behavior is part of the deployed
  repin, not a claim about an arbitrary environment variable flip.
- The OCI router loads one baked allowlist and shares it with the blob store and the
  manifest resolver. The client-write predicate is exactly
  `dedup && is_allowlisted(blob_key)`: every non-admitted digest remains under the
  tenant namespace. On the blob upload/finalize path the predicate does not inspect OCI
  media type, so the five pinned index digests are eligible by digest just as the layer
  pin is; content verification still prevents a client from inserting different bytes.
  `PUT /v2/<repo>/manifests/<reference>` is separate: it writes validated manifest/index
  JSON only to the tenant-scoped `ManifestKvStore`, never `_public`. The resolver's M2
  index-root closure promotion adds only digest-verified descendants of owner-pinned roots.
- With dedup enabled, OCI by-digest reads check `_public` by existence first and then
  fall back to the tenant namespace. This is safe only because OCI blobs are addressed
  by a verified SHA-256 digest; the read path does not turn arbitrary tenant data into
  public data.
- Native R2 keys remain `<region>/<tenant_prefix>/<digest>` for BLAKE3 CAS/sccache and
  `<region>/<tenant_prefix>/bazel/sha256/<digest>` for Bazel. In production,
  `tenant_prefix` is `derive_prefix(secret_tdk, tenant_uuid)` and missing derivation
  fails closed. Identical native bytes in two tenants therefore retain distinct HMAC-
  derived keys, and regions remain separate.

This note deliberately does not change storage or HMAC code. It records the strategy
claim at the scope the current implementation can substantiate: pip/brew public bytes,
npm public metadata, and curated OCI `_public` — not CAS-wide cross-tenant sharing.

## Citations

1. `crates/corelink-container/src/public_base_allowlist.manifest:93,96-100` — the complete
   six-pin population and provenance.
2. `crates/corelink-container/src/public_base_allowlist.rs:25-28,45-55,63-78` — binary-baked
   manifest, canonical digest parsing, and fail-closed load.
3. `crates/corelink-container/src/public_base_allowlist.rs:148-186` — executable test
   asserting the six M3 pins and rejecting the old non-active layer example.
4. `wrangler.toml:313,765-766,922-923,1088-1089,1248-1249` — both OCI boot flags in all
   five production blocks.
5. `crates/corelink-container/src/routes/oci.rs:825-858` — one shared flag/allowlist
   load wired into the blob store and resolver.
6. `crates/corelink-container/src/routes/oci.rs:283-291,570-598` — the single
   allowlist-gated client-write predicate and namespace choice.
7. `crates/corelink-container/src/routes/oci.rs:626-660` — existence-based `_public`
   read followed by the tenant fallback.
8. `crates/corelink-container/src/routes/public_pullthrough.rs:157-166,460-523` — an
   allowlisted image-index root promotes its verified closure server-side.
9. `crates/corelink-container/src/storage/r2_s3.rs:595-618,994-1011` — production
   region + HMAC-derived tenant-prefix key layout and fail-closed derivation.

## Verification record

On 2026-09-01, a full-manifest parser reported `active 6 unique 6` and printed all six
rows above. The focused `corelink-server inc6_ --lib` Cargo test was attempted with two
jobs, but package-cache locking and concurrent workspace builds prevented completion;
this note makes no passing-Cargo claim. `validate_okf.py` then reported **165 concepts,
1 deferred, 0 stale/drift**, and `okf_status.py --check` reported no manifest drift;
`validate_docs_reality.py` exited 0 with its pre-existing non-fatal warning set. The
static checks cover the six-pin population, digest-only finalize predicate, and
tenant-scoped manifest route; this record makes no claim that a prose-qualifier deletion
was independently enforced by an executable mutation verifier.
