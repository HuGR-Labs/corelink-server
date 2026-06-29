---
type: "StorageComponent"
title: "BYOK envelope encryption at rest (CAS path wired, gated-inert)"
description: "The BYOK microkernel (a KmsProvider trait + Dek/WrappedDek types + EnvelopeEncryptor + one compile-time-selected provider per build) PLUS the Wave-3a CAS wiring: byok_cas.rs now encrypts native CAS objects at rest for tenant_byok_config.state='active' tenants (convergent Mode A), fail-closed. STATUS: wired into the native CAS write/read path but gated-inert — zero active tenants and no prod KmsProvider; AC + ciphertext-accounting + §4 key-hardening remain deferred."
source_files:
  - "crates/corelink-container/src/byok.rs"
  - "crates/corelink-container/src/byok_orchestrator.rs"
  - "crates/corelink-byok/src/lib.rs"
  - "crates/corelink-byok/src/byok_aws/real.rs"
  - "crates/corelink-container/src/storage/byok_cas.rs"
checkpoint_sha: "c631a9522dd13136f7a69e1678b4578b4f933c47"
provenance: "AUTHORED"
tags: ["storage", "byok", "encryption", "kms", "envelope"]
timestamp: "2026-06-28T00:00:00Z"
---

# BYOK envelope encryption at rest (CAS path wired, gated-inert)

Bring-Your-Own-Key (BYOK) lets a customer hold the master key that wraps the
data-encryption keys (DEKs) protecting their objects at rest. CoreLink ships the BYOK pieces as a
microkernel: a thin `corelink-byok-core` provides the `KmsProvider` trait + `Dek`/`WrappedDek` types +
the `EnvelopeEncryptor`, and exactly ONE provider plugin per deployment (AWS / GCP / Azure / HashiCorp
Vault) realises the trait against a real KMS. Provider selection is compile-time via cargo features —
multi-provider builds are rejected at compile time — which eliminates runtime branching on the hot
crypto path and lets cargo-deny enforce "only one KMS SDK linked per build."

**STATUS — at-rest BYOK encryption is now WIRED into the native CAS path, but gated-inert.** The
previous "UNWIRED — no storage call site" status is OBSOLETE for CAS: BYOK Wave 3a added the storage
glue module [`byok_cas`](#) (`crates/corelink-container/src/storage/byok_cas.rs:1-9`), consumed by
`R2CasHandler` on the native CAS write/read path. On write, for an `active` tenant the plaintext is
enveloped with `encrypt_convergent` before the R2 PUT
(`crates/corelink-container/src/storage/byok_cas.rs:510-521`); on read the stored blob is decrypted
BEFORE the content-address integrity check
(`crates/corelink-container/src/storage/byok_cas.rs:529-548`). The whole path is **gated-inert**:
engagement is keyed on `tenant_byok_config.state == 'active'`
(`crates/corelink-container/src/storage/byok_cas.rs:568-583`), the onboarding wave that writes those
rows is deferred, so today **zero** tenants are active and every existing tenant keeps the exact
current plaintext behaviour; the handler also threads the BYOK collaborators as `Option`s, so a
default build (no prod `KmsProvider` wired — the orchestrator falls back to the in-memory fake) runs
the plaintext path unchanged (`crates/corelink-container/src/storage/byok_cas.rs:11-40`). This is the
at-rest confidentiality complement to the durable [R2 CAS bucket](/storage/r2-cas-bucket.md).

**Still deferred (documented, never silently skipped):** the Action Cache (`R2AcHandler`) path; the §4
HMAC'd-digest key hardening (the CAS R2 key keeps the raw plaintext digest for 3a); ciphertext-size
accounting/quota reconciliation; Mode B (`crypto_mode='random'`) and the `partial`/backfill dual-read
state (both **fail-closed** here, NOT plaintext); and the provider/`EnvelopeEncryptor` factory wiring
into a production KMS (`byok.rs` / `byok_orchestrator.rs` still construct a provider behind an
`Arc<dyn KmsProvider>` but the default binary links the in-memory fake).

# Role
- The Wave-3a CAS storage glue: a per-tenant BYOK config cache + Tcs resolver + the convergent
  encrypt/decrypt blob codec that `R2CasHandler` calls on the CAS write/read path
  (`crates/corelink-container/src/storage/byok_cas.rs:1-9`).
- The feature-gated production factory that constructs the live KMS provider as an `Arc<dyn KmsProvider>`
  (`crates/corelink-container/src/byok.rs:1-29`).
- The BYOK umbrella crate: the single import target re-exporting the core trait + types and gating the
  four providers (`crates/corelink-byok/src/lib.rs:1-21`).

# How it works
1. The native CAS write/read path runs through `R2CasHandler`, which threads the BYOK collaborators
   (config cache, Tcs resolver) as `Option`s — when any is `None` (the default binary), or the tenant
   is not `active`, the plaintext path runs unchanged
   (`crates/corelink-container/src/storage/byok_cas.rs:11-40`).
2. The per-tenant engagement decision is a single source of truth shared by write and read:
   `active` + convergent ⇒ encrypt; `active` + random / `partial` ⇒ FAIL-CLOSED; everything else ⇒
   plaintext (`crates/corelink-container/src/storage/byok_cas.rs:568-583`).
3. `ByokConfigCache::get` resolves the tenant's BYOK config with at most ONE D1 read on a miss and
   caches even the "not configured / inactive" answer, so the non-BYOK hot path adds no D1 hop after
   warm-up (`crates/corelink-container/src/storage/byok_cas.rs:171-201`).
4. For an `active` tenant, `TcsResolver::resolve` reads the CMK-wrapped Tenant Convergence Secret,
   unwraps it via the customer CMK through the injected `KmsProvider`, and caches the plaintext Tcs in
   a ≤300 s window (`crates/corelink-container/src/storage/byok_cas.rs:412-454`).
5. On write the plaintext is enveloped as `MAGIC ‖ nonce ‖ ciphertext` via `encrypt_convergent` —
   convergent so identical content yields byte-identical output and dedup is preserved
   (`crates/corelink-container/src/storage/byok_cas.rs:510-521`).
6. On read the stored blob is decrypted (and its 4-byte magic checked) BEFORE integrity verification,
   so a non-ciphertext / tampered object for an active tenant is refused rather than served raw
   (`crates/corelink-container/src/storage/byok_cas.rs:529-548`).
7. The provider plugins are exposed as cargo features `aws`/`gcp`/`azure`/`vault`, at most one active per
   build, with no provider as the default test/CI build
   (`crates/corelink-byok/src/lib.rs:34-43`).
8. The container CAN construct the production provider behind an `Arc<dyn KmsProvider>` via a feature-gated
   async factory (`crates/corelink-container/src/byok.rs:1-29`); the orchestrator's `make_provider` is the
   singleton dispatch that builds exactly ONE provider for the binary and emits a boot-time audit event
   (`crates/corelink-container/src/byok_orchestrator.rs:208-219`).
9. Provider selection is purely compile-time: `build_active` is a `cfg`-gated branch per `byok-*-real`
   feature, falling back to the in-process `InMemoryFake` when no real provider flag is set
   (`crates/corelink-container/src/byok_orchestrator.rs:223-272`).
10. The real AWS enforcer `AwsKmsRealProvider::new` unconditionally enables `use_fips(true)` and resolves
    EXPLICIT static credentials + an explicit FIPS endpoint instead of the CF-cold-start-hanging AWS SDK
    chain (`crates/corelink-byok/src/byok_aws/real.rs:176-256`).

# Invariants
- **The CAS path encrypts at rest ONLY for an `active`, convergent tenant.** `partial`/`random` are
  active-but-unsupported in 3a and FAIL CLOSED (never plaintext); inactive/pending/shredded/not-configured
  run plaintext (`crates/corelink-container/src/storage/byok_cas.rs:568-583`).
- **Fail-CLOSED on undetermined state.** For an active tenant an unavailable config/KMS/Tcs surfaces as an
  `Err` (5xx), never a downgrade to storing/serving plaintext: the config cache propagates the source
  error (`crates/corelink-container/src/storage/byok_cas.rs:171-201`) and the Tcs resolver propagates a
  missing-secret / KMS-unwrap failure (`crates/corelink-container/src/storage/byok_cas.rs:412-454`).
- **Read refuses non-ciphertext for an active tenant.** A stored object lacking the BYOK magic, or any
  AEAD failure, is an error — the raw stored bytes are never returned
  (`crates/corelink-container/src/storage/byok_cas.rs:529-548`).
- **The unwrapped Tcs lives only inside a bounded ≤300 s window** (INV-BYOK-CRYPTO-SOVEREIGNTY): the Tcs
  cache rejects a TTL over 300 s (`crates/corelink-container/src/storage/byok_cas.rs:313-323`).
- AT MOST ONE KMS provider may be linked in any build; a multi-provider build is rejected at compile time
  by the `compile_error!` guards (`crates/corelink-byok/src/lib.rs:105-139`).
- `#![forbid(unsafe_code)]` holds on both the container factory and the umbrella crate's surface
  (`crates/corelink-container/src/byok.rs:10`, `crates/corelink-byok/src/lib.rs:93`).

# Gotchas
- The wiring is gated-inert, not dormant code: the encrypt/decrypt branches ARE on the live CAS path, but
  they only engage for `tenant_byok_config.state='active'`, of which there are currently zero (onboarding
  is a later wave). Do not read "wired" as "encrypting production traffic today."
- Convergent (Mode A) is deliberate: identical content ⇒ byte-identical ciphertext ⇒ dedup preserved
  (`crates/corelink-container/src/storage/byok_cas.rs:510-521`). The trade-off is a confirmation oracle on
  the raw digest; the §4 HMAC'd-digest key hardening that closes it is deferred to Wave 3b.
- Provider selection is a build-time decision, not runtime config: switching a tenant's KMS provider is a
  rebuild + redeploy, not a flag flip. Even with a `byok-*-real` feature enabled, a tenant only encrypts
  once its config row is `active`.

# Citations
1. `crates/corelink-container/src/storage/byok_cas.rs:1-9` — module charter: the Wave-3a glue between the
   Wave-1 crypto primitives and the Wave-2 config read model, consumed by `R2CasHandler` on the CAS write/read path.
2. `crates/corelink-container/src/storage/byok_cas.rs:11-40` — GATED-INERT + FAIL-CLOSED safety envelope and the Wave-3a scope/deferrals.
3. `crates/corelink-container/src/storage/byok_cas.rs:171-201` — `ByokConfigCache::get`: one D1 read on miss, caches the not-configured/inactive answer, fail-closed on source error.
4. `crates/corelink-container/src/storage/byok_cas.rs:313-323` — `TcsCache::new`: rejects a Tcs TTL over 300 s (INV-BYOK-CRYPTO-SOVEREIGNTY).
5. `crates/corelink-container/src/storage/byok_cas.rs:412-454` — `TcsResolver::resolve`: wrapped-Tcs read → CMK unwrap → ≤300 s cache, fail-closed.
6. `crates/corelink-container/src/storage/byok_cas.rs:510-521` — `encrypt_cas_blob`: convergent envelope `MAGIC ‖ nonce ‖ ciphertext`, dedup-preserving.
7. `crates/corelink-container/src/storage/byok_cas.rs:529-548` — `decrypt_cas_blob`: magic check + AEAD decrypt, fail-closed (no raw bytes served).
8. `crates/corelink-container/src/storage/byok_cas.rs:568-583` — `engagement_for`: the state → encrypt / fail-closed / plaintext truth table.
9. `crates/corelink-container/src/byok.rs:1-29` — feature-gated factory returning an `Arc<dyn KmsProvider>`.
10. `crates/corelink-container/src/byok.rs:10` — `#![forbid(unsafe_code)]` on the factory.
11. `crates/corelink-byok/src/byok_aws/real.rs:176-256` — `AwsKmsRealProvider::new`/`with_fips`: unconditional `use_fips(true)` + EXPLICIT static creds + explicit FIPS endpoint (no CF-cold-start-hanging AWS SDK chain).
12. `crates/corelink-byok/src/lib.rs:1-21` — umbrella re-export of the core trait/types + microkernel mutual-exclusion intro.
13. `crates/corelink-byok/src/lib.rs:34-43` — cargo feature reference (`aws`/`gcp`/`azure`/`vault`), default no provider.
14. `crates/corelink-byok/src/lib.rs:93` — `#![forbid(unsafe_code)]` on the umbrella crate surface.
15. `crates/corelink-byok/src/lib.rs:105-139` — compile-time at-most-one-provider `compile_error!` guards.
16. `crates/corelink-container/src/byok_orchestrator.rs:208-219` — `make_provider`: builds the singleton `Arc<dyn KmsProvider>` + boot audit event.
17. `crates/corelink-container/src/byok_orchestrator.rs:223-272` — `build_active`: compile-time `cfg` dispatch over the four `byok-*-real` providers, `InMemoryFake` fallback when none set.
