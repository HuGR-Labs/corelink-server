---
type: "StorageComponent"
title: "BYOK envelope-encryption skeleton (UNWIRED — no storage call site)"
description: "The BYOK microkernel: a KmsProvider trait + Dek/WrappedDek types + EnvelopeEncryptor + one compile-time-selected provider per build. STATUS: a crate skeleton with NO storage-path call site — nothing in the live storage/CAS path wraps a DEK or encrypts at rest today."
source_files:
  - "crates/corelink-container/src/byok.rs"
  - "crates/corelink-container/src/byok_orchestrator.rs"
  - "crates/corelink-byok/src/lib.rs"
checkpoint_sha: "30ec21dc78d79c85f4d7e1e19e13118c66025c9e"
provenance: "AUTHORED"
tags: ["storage", "byok", "encryption", "kms", "envelope"]
timestamp: "2026-06-26T00:00:00Z"
---

# BYOK envelope-encryption skeleton (UNWIRED — no storage call site)

Bring-Your-Own-Key (BYOK) is DESIGNED to let a customer hold the master key that wraps the
data-encryption keys (DEKs) protecting their objects at rest. CoreLink ships the BYOK pieces as a
microkernel: a thin `corelink-byok-core` provides the `KmsProvider` trait + `Dek`/`WrappedDek` types +
the `EnvelopeEncryptor`, and exactly ONE provider plugin per deployment (AWS / GCP / Azure / HashiCorp
Vault) realises the trait against a real KMS. Provider selection is compile-time via cargo features —
multi-provider builds are rejected at compile time — which would eliminate runtime branching on the hot
crypto path and lets cargo-deny enforce "only one KMS SDK linked per build."

**STATUS — at-rest BYOK encryption is NOT wired.** The container can construct a provider behind an
`Arc<dyn KmsProvider>` (`byok.rs` / `byok_orchestrator.rs`), but there is **no storage-path call site**:
`grep` over `storage.rs`, the `storage/` module, and the native CAS write path (`routes/cas.rs`) finds
zero `encrypt` / `wrap_dek` / `KmsProvider` references. `EnvelopeEncryptor` has **no caller** anywhere in
`corelink-container` (its only users tree-wide are the unrelated `corelink-enterprise-inquiry` crate and
`tests/`). So even with a `byok-*-real` cargo feature enabled, no tenant byte is enveloped at rest on the
live path — the provider/encryptor are a compiled-but-unconsumed skeleton. This is the planned at-rest
confidentiality complement to the durable [R2 CAS bucket](/storage/r2-cas-bucket.md), but the wiring is
deferred.

# Role
- The feature-gated production factory that constructs the live KMS provider as an `Arc<dyn KmsProvider>`
  (`crates/corelink-container/src/byok.rs:1-9`). The returned handle is NOT consumed by the storage layer.
- The BYOK umbrella crate: the single import target re-exporting the core trait + types and gating the
  four providers (`crates/corelink-byok/src/lib.rs:1-21`).

# How it works
1. The BYOK microkernel re-exports the `KmsProvider` + `EnvelopeEncryptor` core from the umbrella crate
   (`crates/corelink-byok/src/lib.rs:1-21`) — these are library primitives; nothing in the container's
   storage path calls them.
2. The four providers are exposed as cargo features `aws`/`gcp`/`azure`/`vault`, at most one active per
   build, with no provider as the default test/CI build
   (`crates/corelink-byok/src/lib.rs:34-43`).
3. The container CAN construct the production provider behind an `Arc<dyn KmsProvider>` via a feature-gated
   async factory so future providers drop in behind the same surface
   (`crates/corelink-container/src/byok.rs:1-9`) — but no encrypt/decrypt call site consumes it today.
4. The AWS factory enforces the FIPS endpoint unconditionally and resolves credentials via the standard
   AWS SDK chain (env, shared config, IRSA, IMDS, SSO)
   (`crates/corelink-container/src/byok.rs:17-29`).
5. The orchestrator's `make_provider` is the singleton dispatch: it constructs exactly ONE
   `Arc<dyn KmsProvider>` for the binary and emits one boot-time `audit = true` event recording the
   active provider label, but the returned handle is consumed only by callers of this factory — not by
   the storage write/read path (`crates/corelink-container/src/byok_orchestrator.rs:208-219`).
6. Provider selection is purely compile-time: `build_active` is a `cfg`-gated branch per `byok-*-real`
   feature, falling back to the in-process `InMemoryFake` when no real provider flag is set — so a
   default/CI build links no KMS SDK and round-trips DEKs in memory
   (`crates/corelink-container/src/byok_orchestrator.rs:223-272`).

# Invariants
- **No at-rest encryption is performed on the live storage path.** The orchestrator's `make_provider`
  constructs the singleton `Arc<dyn KmsProvider>` and returns it to its caller — it performs no
  wrap/encrypt and no storage write of its own (`crates/corelink-container/src/byok_orchestrator.rs:208-219`);
  there is no call site that wraps a DEK or encrypts an object body in `storage.rs` / `storage/` /
  `routes/cas.rs`, and `EnvelopeEncryptor` has zero callers in `corelink-container`. Any claim that "the
  storage layer encrypts/decrypts at rest" is FALSE for current code — it is the deferred design target.
- AT MOST ONE KMS provider may be linked in any build; a multi-provider build is rejected at compile time
  by the `compile_error!` guards (`crates/corelink-byok/src/lib.rs:105-139`).
- `Dek`/`WrappedDek` zeroize discipline + `SecretString` credential bytes + `subtle::ConstantTimeEq`
  comparisons are preserved by reference across the wave-35 absorption
  (`crates/corelink-byok/src/lib.rs:64-90`).
- `#![forbid(unsafe_code)]` holds on both the container factory and the umbrella crate's surface
  (`crates/corelink-container/src/byok.rs:10`, `crates/corelink-byok/src/lib.rs:93`).

# Gotchas
- The container BYOK factory is activated only under its cargo feature (`byok-aws-real`); a default build
  links no real KMS SDK and uses the in-memory fake — so a `make_aws_kms_provider` call site only exists
  in the feature-gated wire-up, not the default binary. Even when enabled, the constructed provider is not
  threaded into the storage write/read path.
- Provider selection is a build-time decision, not runtime config: switching a tenant's KMS provider is a
  rebuild + redeploy, not a flag flip.

# Citations
1. `crates/corelink-container/src/byok.rs:1-9` — feature-gated factory returning an `Arc<dyn KmsProvider>` (handle not consumed by storage).
2. `crates/corelink-container/src/byok.rs:10` — `#![forbid(unsafe_code)]` on the factory.
3. `crates/corelink-container/src/byok.rs:17-29` — AWS KMS provider construction: FIPS endpoint enforced, SDK credential chain.
4. `crates/corelink-byok/src/lib.rs:1-21` — umbrella re-export of the core trait/types + microkernel mutual-exclusion intro.
5. `crates/corelink-byok/src/lib.rs:105-139` — compile-time at-most-one-provider `compile_error!` guards.
6. `crates/corelink-byok/src/lib.rs:34-43` — cargo feature reference (`aws`/`gcp`/`azure`/`vault`), default no provider.
7. `crates/corelink-byok/src/lib.rs:52-62` — charter compliance: `forbid(unsafe_code)`, zeroize, `SecretString`, `ConstantTimeEq`.
8. `crates/corelink-byok/src/lib.rs:64-90` — wave-35 absorption preserving zeroize + credential discipline by reference.
9. `crates/corelink-container/src/byok_orchestrator.rs:208-219` — `make_provider`: builds the singleton `Arc<dyn KmsProvider>` + boot audit event; handle not consumed by the storage path.
10. `crates/corelink-container/src/byok_orchestrator.rs:223-272` — `build_active`: compile-time `cfg` dispatch over the four `byok-*-real` providers, `InMemoryFake` fallback when none set.
