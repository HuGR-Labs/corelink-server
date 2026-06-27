---
type: "CrateCluster"
title: "Adapter-host crate cluster (surfaces + KMS)"
description: "The translation layers that map third-party cache protocols (package managers, Bazel REAPI, Turborepo) and external KMS providers onto CoreLink's canonical CAS/auth SPI traits."
source_files:
  - "crates/corelink-adapter-host/src/lib.rs"
  - "crates/corelink-bazel-bridge/src/lib.rs"
  - "crates/corelink-bazel-bridge/Cargo.toml"
  - "crates/corelink-bazel-bridge/src/digest.rs"
  - "crates/corelink-bazel-bridge/src/find_missing.rs"
  - "crates/corelink-bazel-bridge/src/adapter.rs"
  - "crates/corelink-byok/src/lib.rs"
checkpoint_sha: "30789129fe9bd4fdff000ecea3e6abefed996a2f"
provenance: "AUTHORED"
tags: ["crates", "adapters", "bazel", "byok", "kms", "surfaces"]
timestamp: "2026-06-26T00:00:00Z"
---

# Adapter-host crate cluster (surfaces + KMS)

CoreLink sells one cache but speaks many protocols: cargo, npm, pip, brew, OCI, Bazel REAPI, Turborepo — plus it must encrypt at rest under a customer's own KMS. This cluster is the set of translation crates that keep that protocol/provider sprawl out of the core. Each adapter maps a foreign wire format onto the same canonical workspace SPI traits (`CasReadHandler`, `CasWriteHandler`, `PatValidator`, `KvBackend`, `AuditEmitter`) so the CAS core never learns what a Homebrew bottle or an OCI manifest is; and `corelink-byok` keeps the four KMS providers behind a compile-time microkernel so exactly one provider SDK is linked per build.

# Role

These crates are the outer ring of the [adapter / package-manager surfaces](/surfaces/public-packages.md) and the [Bazel REAPI](/surfaces/bazel-reapi.md) / [Turborepo](/surfaces/turborepo.md) surfaces. They translate inbound — foreign protocol to canonical SPI — and `corelink-byok` translates outbound — canonical envelope-encryption to a specific cloud KMS. The boundary is deliberate: the adapters carry the protocol quirks so the CAS/AC core stays protocol-agnostic.

# How it works

- `corelink-adapter-host` bridges each package-manager adapter's local async port traits (`CasStore`, `TenantResolver`, `KvStore`, `BlobStore`, `ManifestKvStore`) to the canonical sync workspace SPI traits, calling sync handlers from async ports via `spawn_blocking` so the runtime thread is never blocked (`crates/corelink-adapter-host/src/lib.rs:1-29`).
- It physically absorbed the 5 Wave-34 adapter crates (brew/cargo/npm/oci/pip) as inline submodules, so one crate now hosts both the absorbed adapter source and its SPI bridge under stable `corelink_adapter_host::<adapter>::*` paths (`crates/corelink-adapter-host/src/lib.rs:31-66`).
- `corelink-bazel-bridge` maps the REAPI v2 REST subset Bazel speaks (`GET/POST /<instance>/blobs/…`, `findMissingBlobs`) onto the existing CAS/AC handler traits, REST-only with no gRPC runtime dependency (`crates/corelink-bazel-bridge/src/lib.rs:1-50`).
- `corelink-byok` is a microkernel: a thin core (`KmsProvider` trait + envelope encryption + `DekCache`) plus exactly one feature-selected provider plugin (`aws`/`gcp`/`azure`/`vault`), with multi-provider builds rejected at compile time (`crates/corelink-byok/src/lib.rs:1-43`).

# Invariants

- The absorbed package-manager adapters stay free of workspace SPI imports — the bridge crate composes them at boot, so a protocol adapter never couples to the CAS core directly. There is **no positive code enforcer**: this is a NEGATIVE / dep-graph property (the absorbed adapter submodules import no `corelink-handler-*`/SPI trait); the `//!` only states the intent (`crates/corelink-adapter-host/src/lib.rs:1-7`).
- Bazel digests are validated at the boundary: `Digest::parse`/`new` reject a hash that is not 64 lowercase hex and a size > 4 GiB (`crates/corelink-bazel-bridge/src/digest.rs:85-114`); on a write `cas_put` rejects a PUT body whose length ≠ `size_bytes` with `SizeMismatch` (`crates/corelink-bazel-bridge/src/adapter.rs:135-147`); and `findMissingBlobs` rejects batches > 4096 digests (`crates/corelink-bazel-bridge/src/find_missing.rs:138-141`).
- The bridge is REST-only by invariant `INV-BAZEL-NO-GRPC`. There is **no positive code enforcer** — the invariant is a NEGATIVE / dep-graph guard: `corelink-bazel-bridge`'s `[dependencies]` contains no tonic/prost/gRPC runtime crate (`crates/corelink-bazel-bridge/Cargo.toml:13-24`), which `cargo-deny` can lock down; the `//!` only states the intent (`crates/corelink-bazel-bridge/src/lib.rs:48-50`). The invariant id is spelled correctly as `INV-BAZEL-NO-GRPC` in the code (the prior `GROPC` one-char typo was fixed).
- At most one BYOK provider is active per build, enforced by a compile-time guard, eliminating runtime branching on the crypto hot path (`crates/corelink-byok/src/lib.rs:105-150`).

# Gotchas

- `KvBackend` uses RPITIT (`impl Future`) so it is not object-safe; bridges targeting it are generic over `K: KvBackend + …`, never `dyn`.
- The package-manager adapters write public-dedup content to the `_public` namespace, which has its own fail-closed storage-chain requirement (a `tenant_storage_state` row + sentinel R2 prefix) — a separate gotcha from the bridge itself.
- BYOK default build links no provider (trait + in-memory fake only), matching the test/CI build; a real provider is a deliberate per-deployment cargo-feature choice that cargo-deny can then lock down.

# Citations

1. `crates/corelink-adapter-host/src/lib.rs:1-29` — the bridge crate mapping adapter ports to canonical SPI traits via `spawn_blocking`.
2. `crates/corelink-adapter-host/src/lib.rs:31-66` — the 5 absorbed package-manager adapters under stable submodule paths.
3. `crates/corelink-bazel-bridge/src/lib.rs:1-50` — REAPI v2 REST → CAS/AC trait mapping, REST-only (the `INV-BAZEL-NO-GRPC` intent is stated in the `//!`).
3b. `crates/corelink-bazel-bridge/Cargo.toml:13-24` — `INV-BAZEL-NO-GRPC` negative enforcer: `[dependencies]` carries no tonic/prost/gRPC crate (cargo-deny-lockable).
4. `crates/corelink-bazel-bridge/src/digest.rs:85-114` — `Digest::parse`/`new`: 64-lowercase-hex hash + `validate_size` (≤ 4 GiB).
5. `crates/corelink-bazel-bridge/src/adapter.rs:135-147` — `cas_put`: PUT body length MUST equal `size_bytes` (`SizeMismatch`).
6. `crates/corelink-bazel-bridge/src/find_missing.rs:138-141` — `FIND_MISSING_BLOB_CAP` (4096) batch rejection.
7. `crates/corelink-byok/src/lib.rs:1-43` — the BYOK microkernel: thin core + one feature-gated provider.
8. `crates/corelink-byok/src/lib.rs:105-150` — compile-time mutual-exclusion guards (one provider per build).
