---
type: "StorageComponent"
title: "BYOK envelope encryption at rest"
description: "How CoreLink is DESIGNED to wrap data-encryption keys under a customer-controlled KMS (AWS/GCP/Azure/Vault) via a microkernel BYOK core with exactly one provider linked per build — note: no KMS provider is constructed in the live container yet (default build links the in-memory fake)."
source_files:
  - "crates/corelink-container/src/byok.rs"
  - "crates/corelink-byok/src/lib.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["storage", "byok", "encryption", "kms", "envelope"]
timestamp: "2026-06-26T00:00:00Z"
---

# BYOK envelope encryption at rest

Bring-Your-Own-Key (BYOK) lets a customer hold the master key that wraps the data-encryption keys (DEKs)
protecting their objects at rest. CoreLink implements this as a microkernel: a thin
`corelink-byok-core` ships the `KmsProvider` trait + `Dek`/`WrappedDek` types + the `EnvelopeEncryptor`,
and exactly ONE provider plugin per deployment (AWS / GCP / Azure / HashiCorp Vault) realises the trait
against a real KMS. Provider selection is compile-time via cargo features — multi-provider builds of the
PUBLIC provider features (`aws`/`gcp`/`azure`/`vault`) are rejected at compile time — which eliminates
runtime branching on the hot crypto path. The `compile_error!` mutual-exclusion guards are NOT the whole
story, though: they inspect ONLY the public namespace features, so the internal `_internal-*` /
`_matrix-test` features (which the matrix integration tests use) link ALL FOUR provider modules at once
WITHOUT tripping any guard (`crates/corelink-byok/src/lib.rs:159-188`). The "exactly one KMS SDK linked"
guarantee for a PRODUCTION binary is therefore upheld by the public-feature `compile_error!` guards PLUS
cargo-deny's single-SDK-per-binary rule at the workspace boundary — not by the `compile_error!` alone. BYOK is the **designed** envelope-encryption layer: the
container exposes a feature-gated factory that can construct the production provider behind an
`Arc<dyn KmsProvider>`, but at-rest encryption is **not yet on the live R2 write path** — the default build
links no real KMS SDK (it uses the in-memory fake) and the live storage layer (`storage.rs`/`r2_s3.rs`) wires
no `KmsProvider`. It is the intended at-rest confidentiality complement to the durable
[R2 CAS bucket](/storage/r2-cas-bucket.md), to be activated when the production provider is wired in.

# Role
- The feature-gated production factory that constructs the live KMS provider as an `Arc<dyn KmsProvider>`
  (`crates/corelink-container/src/byok.rs:1-9`).
- The BYOK umbrella crate: the single import target re-exporting the core trait + types and gating the
  four providers (`crates/corelink-byok/src/lib.rs:1-21`).

# How it works
1. BYOK envelope encryption wraps each DEK under a customer-controlled KMS via the `KmsProvider` +
   `EnvelopeEncryptor` core re-exported from the umbrella crate
   (`crates/corelink-byok/src/lib.rs:1-21`).
2. The four providers are exposed as cargo features `aws`/`gcp`/`azure`/`vault`, at most one active per
   build, with no provider as the default test/CI build
   (`crates/corelink-byok/src/lib.rs:34-43`).
3. The container constructs the production provider behind an `Arc<dyn KmsProvider>` via a feature-gated
   async factory so future providers drop in behind the same surface
   (`crates/corelink-container/src/byok.rs:1-9`).
4. The AWS factory enforces the FIPS endpoint unconditionally and resolves credentials via the standard
   AWS SDK chain (env, shared config, IRSA, IMDS, SSO)
   (`crates/corelink-container/src/byok.rs:17-29`).

# Invariants
- AT MOST ONE PUBLIC KMS provider feature (`aws`/`gcp`/`azure`/`vault`) may be active in any build; a
  multi-public-provider build is rejected at compile time by the `compile_error!` guards
  (`crates/corelink-byok/src/lib.rs:105-139`). This guard does NOT cover the internal `_internal-*` /
  `_matrix-test` feature path, which deliberately links all four provider modules without tripping it
  (`crates/corelink-byok/src/lib.rs:159-188`); the single-SDK-per-PRODUCTION-binary guarantee is closed by
  cargo-deny at the workspace boundary (`:165-166`), not by the `compile_error!` alone.
- `Dek`/`WrappedDek` zeroize discipline + `SecretString` credential bytes + `subtle::ConstantTimeEq`
  comparisons are preserved by reference across the wave-35 absorption
  (`crates/corelink-byok/src/lib.rs:64-90`).
- `#![forbid(unsafe_code)]` holds on both the container factory and the umbrella crate's surface
  (`crates/corelink-container/src/byok.rs:10`, `crates/corelink-byok/src/lib.rs:93`).

# Gotchas
- The container BYOK factory is activated only under its cargo feature (`byok-aws-real`); a default build
  links no real KMS SDK and uses the in-memory fake — so a `make_aws_kms_provider` call site only exists
  in the feature-gated wire-up, not the default binary.
- Provider selection is a build-time decision, not runtime config: switching a tenant's KMS provider is a
  rebuild + redeploy, not a flag flip.

# Citations
1. `crates/corelink-container/src/byok.rs:1-9` — feature-gated factory returning an `Arc<dyn KmsProvider>`.
2. `crates/corelink-container/src/byok.rs:10` — `#![forbid(unsafe_code)]` on the factory.
3. `crates/corelink-container/src/byok.rs:17-29` — AWS KMS provider construction: FIPS endpoint enforced, SDK credential chain.
4. `crates/corelink-byok/src/lib.rs:1-21` — umbrella re-export of the core trait/types + microkernel mutual-exclusion intro.
5. `crates/corelink-byok/src/lib.rs:105-139` — compile-time at-most-one-PUBLIC-provider `compile_error!` guards.
5b. `crates/corelink-byok/src/lib.rs:159-188` — the `_internal-*`/`_matrix-test` feature path links all four provider modules WITHOUT tripping the guards (the public-namespace gates are the only ones the `compile_error!`s inspect); cargo-deny at the workspace boundary enforces the single-SDK-per-binary rule for production.
6. `crates/corelink-byok/src/lib.rs:34-43` — cargo feature reference (`aws`/`gcp`/`azure`/`vault`), default no provider.
7. `crates/corelink-byok/src/lib.rs:52-62` — charter compliance: `forbid(unsafe_code)`, zeroize, `SecretString`, `ConstantTimeEq`.
8. `crates/corelink-byok/src/lib.rs:64-90` — wave-35 absorption preserving zeroize + credential discipline by reference.
