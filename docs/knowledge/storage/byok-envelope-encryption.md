---
type: "StorageComponent"
title: "BYOK envelope encryption at rest (CAS+AC wired, both modes + §4 hardening; gated-inert)"
description: "The BYOK microkernel (a KmsProvider trait + Dek/WrappedDek types + EnvelopeEncryptor + one compile-time-selected provider per build) PLUS the CAS+AC encryption-at-rest wiring in byok_cas.rs. As of Wave 3c BOTH crypto modes are implemented: Mode A (convergent) dedup-preserving envelopes AND Mode B (crypto_mode='random') with a random per-blob DEK wrapped in byok_envelope (no dedup, but idempotent/no-orphan re-PUT), PLUS §4 key-hardening — for a BYOK-active tenant the physical R2 object key embeds HMAC-SHA256(TCS, plaintext_digest), computed on-the-fly (never persisted), which closes the confirmation oracle while keeping intra-tenant convergent dedup and leaving the AAD-bound digest the REAL digest. STILL gated-inert (zero active tenants, no prod KmsProvider). Deferred: partial/backfill dual-read, onboarding/CMK provisioning, prod KmsProvider wiring, broader crypto-shred (bulk shred on state='shredded'); Mode-B envelope-row reclaim on blob delete is DONE (Wave 4a)."
source_files:
  - "crates/corelink-container/src/byok.rs"
  - "crates/corelink-container/src/byok_orchestrator.rs"
  - "crates/corelink-byok/src/lib.rs"
  - "crates/corelink-byok/src/byok_aws/real.rs"
  - "crates/corelink-container/src/storage/byok_cas.rs"
checkpoint_sha: "f1b9f77277635e6ff38d06a9ceb8ce87a2025614"
provenance: "AUTHORED"
tags: ["storage", "byok", "encryption", "kms", "envelope"]
timestamp: "2026-06-29T00:00:00Z"
---

# BYOK envelope encryption at rest (CAS+AC wired, both modes + §4 hardening; gated-inert)

Bring-Your-Own-Key (BYOK) lets a customer hold the master key that wraps the
data-encryption keys (DEKs) protecting their objects at rest. CoreLink ships the BYOK pieces as a
microkernel: a thin `corelink-byok-core` provides the `KmsProvider` trait + `Dek`/`WrappedDek` types +
the `EnvelopeEncryptor`, and exactly ONE provider plugin per deployment (AWS / GCP / Azure / HashiCorp
Vault) realises the trait against a real KMS. Provider selection is compile-time via cargo features —
multi-provider builds are rejected at compile time — which eliminates runtime branching on the hot
crypto path and lets cargo-deny enforce "only one KMS SDK linked per build."

**STATUS — at-rest BYOK encryption is WIRED into BOTH the native CAS and the Action-Cache paths, both
crypto modes, but gated-inert.** The storage glue module
[`byok_cas`](#) (`crates/corelink-container/src/storage/byok_cas.rs:1-9`) is consumed by `R2CasHandler`
/ `R2AcHandler`. The per-tenant engagement decision is a single source of truth: `active` engages
encryption in the tenant's configured `ByokCryptoMode` (Mode A convergent OR Mode B random), `partial`
(backfill dual-read) fails CLOSED, everything else is plaintext
(`crates/corelink-container/src/storage/byok_cas.rs:1136-1147`). The whole path is **gated-inert**:
the onboarding wave that writes `tenant_byok_config.state='active'` rows is deferred, so today **zero**
tenants are active and every existing tenant keeps the exact current plaintext behaviour; the handler
also threads the BYOK collaborators as `Option`s, so a default build (no prod `KmsProvider`) runs the
plaintext path unchanged (`crates/corelink-container/src/storage/byok_cas.rs:11-46`). This is the
at-rest confidentiality complement to the durable [R2 CAS bucket](/storage/r2-cas-bucket.md).

**Wave 3c — §4 key-hardening (audit H-4) is now IMPLEMENTED.** For a BYOK-active tenant the *physical*
R2 object key no longer embeds the raw plaintext digest; it embeds
`hex(HMAC-SHA256(key = TCS, msg = plaintext_digest))`, computed on-the-fly at every write+read by
`harden_digest` and NEVER persisted as a raw-digest→hardened map
(`crates/corelink-container/src/storage/byok_cas.rs:530-536`). This closes the confirmation oracle (an
attacker with R2 read who *guesses* a plaintext cannot confirm its presence without the per-tenant
`TCS`), while staying deterministic per `(TCS, digest)` so intra-tenant convergent dedup still hits.
Crucially this is the STORAGE-KEY digest ONLY — the convergent `CryptoContext`'s `plaintext_digest`
(bound into the AEAD AAD and re-verified on the decrypted plaintext) stays the REAL digest.

**Wave 3c — Mode B (`crypto_mode='random'`, max-isolation) is now IMPLEMENTED.** A Mode-B write mints a
RANDOM per-blob DEK + nonce, wraps the DEK under the customer CMK, persists the wrapped DEK + nonce in a
`byok_envelope` D1 row, then stores `MODE_B_MAGIC ‖ ciphertext` (`CLB2`, distinct from the convergent
`CLB1`) in R2 (`crates/corelink-container/src/storage/byok_cas.rs:919-1029`). Mode B has NO dedup (each
blob a unique DEK), but it is idempotent and orphan-free: the authoritative `(DEK, nonce)` is the
PERSISTED envelope row written `INSERT … ON CONFLICT DO NOTHING` then re-read, and AES-GCM is
deterministic given `(key, nonce, plaintext, aad)`, so every racer / re-PUT produces byte-identical
ciphertext (`crates/corelink-container/src/storage/byok_cas.rs:757-904`). The envelope PK is
surface-qualified (`"cas:<digest>"` / `"ac:<digest>"`) so a CAS blob and an AC entry that share a digest
string never collide on one row (`crates/corelink-container/src/storage/byok_cas.rs:673-683`).

**Still deferred (documented, never silently skipped):** the `partial`/backfill dual-read state (audit
H7) — fail-CLOSED here, NOT plaintext (Wave 4); onboarding / CMK provisioning (the sole writer of
`active` rows); the production `KmsProvider` factory wiring into a real KMS (`byok.rs` /
`byok_orchestrator.rs` still construct a provider behind an `Arc<dyn KmsProvider>` but the default
binary links the in-memory fake); and broader crypto-shred (bulk shred on `state='shredded'`). NOTE:
reclaim of the Mode-B `byok_envelope` row on blob delete is now DONE (Wave 4a) — `CasDeleteHandler`/
`AcDeleteHandler` delete the surface-qualified envelope row after the R2 object (fail-safe ordering: a
reclaim failure warns but never rolls back the delete, since an orphaned wrapped DEK wraps nothing).

# Role
- The CAS+AC storage glue: a per-tenant BYOK config cache + Tcs resolver + the §4 storage-key hardening +
  the Mode-A convergent codec + the Mode-B random-DEK encryptor + the `byok_envelope` persistence seam
  that `R2CasHandler`/`R2AcHandler` call on the write/read path
  (`crates/corelink-container/src/storage/byok_cas.rs:1-9`).
- The feature-gated production factory that constructs the live KMS provider as an `Arc<dyn KmsProvider>`
  (`crates/corelink-container/src/byok.rs:1-29`).
- The BYOK umbrella crate: the single import target re-exporting the core trait + types and gating the
  four providers (`crates/corelink-byok/src/lib.rs:1-21`).

# How it works
1. The per-tenant engagement decision is the single source of truth shared by write and read:
   `active` ⇒ `Encrypt(crypto_mode)` carrying the tenant's `ByokCryptoMode`; `partial` ⇒ FAIL-CLOSED
   (Wave 4); inactive/pending/shredded ⇒ plaintext
   (`crates/corelink-container/src/storage/byok_cas.rs:1136-1147`).
2. `ByokConfigCache::get` resolves the tenant's BYOK config with at most ONE D1 read on a miss and
   caches even the "not configured / inactive" answer, fail-closed on a source error
   (`crates/corelink-container/src/storage/byok_cas.rs:202-230`).
3. For an `active` tenant, `TcsResolver::resolve` reads the CMK-wrapped Tenant Convergence Secret,
   unwraps it via the customer CMK through the injected `KmsProvider`, and caches the plaintext Tcs in a
   ≤300 s window (`crates/corelink-container/src/storage/byok_cas.rs:443-485`).
4. **§4 storage-key hardening:** `harden_digest` returns `hex(HMAC-SHA256(TCS, plaintext_digest))` — the
   value the physical R2 object key embeds for an active tenant. It is computed on-the-fly (never
   persisted), deterministic per `(TCS, digest)` (intra-tenant dedup preserved), and distinct from the
   AAD-bound real digest (`crates/corelink-container/src/storage/byok_cas.rs:530-536`).
5. **Mode A (convergent) write:** `encrypt_cas_blob` produces `MAGIC ‖ nonce ‖ ciphertext` (`CLB1`) via
   `encrypt_convergent` — convergent so identical content yields byte-identical output and dedup is
   preserved (`crates/corelink-container/src/storage/byok_cas.rs:615-626`).
6. **Mode A read:** `decrypt_cas_blob` checks the 4-byte `CLB1` magic and AEAD-decrypts BEFORE the
   content-address integrity check, so a non-ciphertext / tampered object for an active tenant is refused
   rather than served raw (`crates/corelink-container/src/storage/byok_cas.rs:634-653`).
7. **Mode B (random) write/read:** `ModeBEncryptor::encrypt` reads-or-mints the authoritative
   `byok_envelope` row (random DEK + nonce, CMK-wrapped, `INSERT … ON CONFLICT DO NOTHING` then re-read),
   then derives a deterministic `MODE_B_MAGIC ‖ ciphertext` (`CLB2`) from the persisted `(DEK, nonce)`;
   decrypt fetches the row, unwraps the DEK, and AES-GCM-decrypts — no dedup, but idempotent and
   orphan-free (`crates/corelink-container/src/storage/byok_cas.rs:919-1029`).
8. The Mode-B `byok_envelope` rows live behind the async `ByokEnvelopeStore` trait, whose production impl
   `D1ByokEnvelopeStore` runs the `SELECT` + idempotent `INSERT` over the D1 row seam; the row carries the
   wrapped DEK + KMS identity + AAD + 12-byte nonce
   (`crates/corelink-container/src/storage/byok_cas.rs:757-904`).
9. On-disk overheads are single-sourced so quota accounting (audit C3) cannot drift: `CLB1` = 32 B
   (4 magic + 12 nonce + 16 GCM tag) (`crates/corelink-container/src/storage/byok_cas.rs:103`); `CLB2` =
   20 B (4 magic + 16 GCM tag — the nonce lives in the `byok_envelope` row, not inline)
   (`crates/corelink-container/src/storage/byok_cas.rs:664-671`).
10. The AC surface mirrors CAS under `ac_crypto_context_for`, binding `AC_SURFACE` (`"ac"`) so the derived
    key + AEAD AAD are domain-separated — an AC ciphertext can never be decrypted as (or swapped with) a
    CAS ciphertext (audit H1) (`crates/corelink-container/src/storage/byok_cas.rs:582-605`).
11. The provider plugins are exposed as cargo features `aws`/`gcp`/`azure`/`vault`, at most one active per
    build, with no provider as the default test/CI build
    (`crates/corelink-byok/src/lib.rs:34-43`).
12. The container CAN construct the production provider behind an `Arc<dyn KmsProvider>` via a feature-gated
    async factory (`crates/corelink-container/src/byok.rs:1-29`); the orchestrator's `make_provider` is the
    singleton dispatch that builds exactly ONE provider for the binary and emits a boot-time audit event
    (`crates/corelink-container/src/byok_orchestrator.rs:208-219`), and `build_active` is the compile-time
    `cfg`-gated dispatch over the four `byok-*-real` providers with an `InMemoryFake` fallback
    (`crates/corelink-container/src/byok_orchestrator.rs:223-272`).
13. The real AWS enforcer `AwsKmsRealProvider::new` unconditionally enables `use_fips(true)` and resolves
    EXPLICIT static credentials + an explicit FIPS endpoint instead of the CF-cold-start-hanging AWS SDK
    chain (`crates/corelink-byok/src/byok_aws/real.rs:176-256`).

# Invariants
- **Encrypt at rest ONLY for an `active` tenant, in its configured mode.** `partial` is active-but-deferred
  and FAILS CLOSED (never plaintext); inactive/pending/shredded/not-configured run plaintext
  (`crates/corelink-container/src/storage/byok_cas.rs:1136-1147`).
- **The §4 hardened storage key is never persisted and never leaks the digest.** It is computed on-the-fly,
  deterministic per `(TCS, digest)`, and distinct from the REAL AAD-bound `plaintext_digest`
  (`crates/corelink-container/src/storage/byok_cas.rs:530-536`).
- **A Mode-B write never overwrites an existing envelope row.** `put_envelope_if_absent` is
  `INSERT … ON CONFLICT DO NOTHING`; a fresh random DEK over an existing row would orphan the stored
  ciphertext, so the caller re-reads and converges on the persisted winner, making every re-PUT idempotent
  (`crates/corelink-container/src/storage/byok_cas.rs:757-904`).
- **Fail-CLOSED on undetermined state.** For an active tenant an unavailable config/KMS/Tcs surfaces as an
  `Err` (5xx), never a downgrade to plaintext: the config cache propagates the source error
  (`crates/corelink-container/src/storage/byok_cas.rs:202-230`) and the Tcs resolver propagates a
  missing-secret / KMS-unwrap failure (`crates/corelink-container/src/storage/byok_cas.rs:443-485`).
- **Read refuses non-ciphertext for an active tenant.** A stored object lacking the BYOK magic, or any
  AEAD failure, is an error — the raw stored bytes are never returned
  (`crates/corelink-container/src/storage/byok_cas.rs:634-653`).
- **The unwrapped Tcs lives only inside a bounded ≤300 s window** (INV-BYOK-CRYPTO-SOVEREIGNTY): the Tcs
  cache rejects a TTL over 300 s (`crates/corelink-container/src/storage/byok_cas.rs:342-354`).
- AT MOST ONE KMS provider may be linked in any build; a multi-provider build is rejected at compile time
  by the `compile_error!` guards (`crates/corelink-byok/src/lib.rs:105-139`).
- `#![forbid(unsafe_code)]` holds on both the container factory and the umbrella crate's surface
  (`crates/corelink-container/src/byok.rs:10`, `crates/corelink-byok/src/lib.rs:93`).

# Gotchas
- The wiring is gated-inert, not dormant code: the encrypt/decrypt branches ARE on the live CAS/AC path,
  but they only engage for `tenant_byok_config.state='active'`, of which there are currently zero
  (onboarding is a later wave). Do not read "wired" as "encrypting production traffic today."
- The convergent-mode confirmation oracle is now CLOSED, not deferred: the physical R2 key embeds the §4
  `harden_digest(TCS, plaintext_digest)`, so R2-read no longer reveals which plaintexts are present
  (`crates/corelink-container/src/storage/byok_cas.rs:530-536`). Earlier wiki revisions that called this
  "deferred to Wave 3b/3c" are OBSOLETE.
- Mode B (random) deliberately has NO dedup — each blob gets a unique DEK. The caller MUST NOT apply the
  convergent HEAD-skip to a Mode-B write; idempotency comes from the persisted envelope row, not from
  content-addressing (`crates/corelink-container/src/storage/byok_cas.rs:919-1029`).
- Provider selection is a build-time decision, not runtime config: switching a tenant's KMS provider is a
  rebuild + redeploy, not a flag flip. Even with a `byok-*-real` feature enabled, a tenant only encrypts
  once its config row is `active`.

# Citations
1. `crates/corelink-container/src/storage/byok_cas.rs:1-9` — module charter: the glue between the Wave-1 crypto primitives and the Wave-2 config read model, consumed by `R2CasHandler`/`R2AcHandler`.
2. `crates/corelink-container/src/storage/byok_cas.rs:11-46` — GATED-INERT + FAIL-CLOSED safety envelope and the wave scope/deferrals.
3. `crates/corelink-container/src/storage/byok_cas.rs:202-230` — `ByokConfigCache::get`: one D1 read on miss, caches the not-configured/inactive answer, fail-closed on source error.
4. `crates/corelink-container/src/storage/byok_cas.rs:342-354` — `TcsCache::new`: rejects a Tcs TTL over 300 s (INV-BYOK-CRYPTO-SOVEREIGNTY).
5. `crates/corelink-container/src/storage/byok_cas.rs:443-485` — `TcsResolver::resolve`: wrapped-Tcs read → CMK unwrap → ≤300 s cache, fail-closed.
6. `crates/corelink-container/src/storage/byok_cas.rs:530-536` — `harden_digest`: §4 `hex(HMAC-SHA256(TCS, plaintext_digest))` storage key (on-the-fly, never persisted, deterministic per `(TCS, digest)`).
7. `crates/corelink-container/src/storage/byok_cas.rs:582-605` — `ac_crypto_context_for`/`ac_crypto_context`: AC surface (`"ac"`) domain separation (audit H1).
8. `crates/corelink-container/src/storage/byok_cas.rs:615-626` — `encrypt_cas_blob`: convergent envelope `MAGIC ‖ nonce ‖ ciphertext` (`CLB1`), dedup-preserving.
9. `crates/corelink-container/src/storage/byok_cas.rs:634-653` — `decrypt_cas_blob`: magic check + AEAD decrypt, fail-closed (no raw bytes served).
10. `crates/corelink-container/src/storage/byok_cas.rs:103` — `BYOK_CLB1_OVERHEAD` = 32 B (4 magic + 12 nonce + 16 tag), single-sourced.
11. `crates/corelink-container/src/storage/byok_cas.rs:664-671` — `BYOK_CLB2_OVERHEAD` = 20 B (4 magic + 16 tag; Mode-B nonce lives in the `byok_envelope` row).
12. `crates/corelink-container/src/storage/byok_cas.rs:673-683` — `envelope_blob_key`: surface-qualified PK (`"cas:"/"ac:"`) so a digest collision can't orphan a ciphertext.
13. `crates/corelink-container/src/storage/byok_cas.rs:757-904` — `ByokEnvelopeStore` trait + `D1ByokEnvelopeStore`: idempotent `INSERT … ON CONFLICT DO NOTHING` + `SELECT` over the D1 row seam (audit C2).
14. `crates/corelink-container/src/storage/byok_cas.rs:919-1029` — `ModeBEncryptor`: random per-blob DEK wrapped in `byok_envelope`, deterministic `CLB2` ciphertext, no dedup but idempotent/no-orphan.
15. `crates/corelink-container/src/storage/byok_cas.rs:1136-1147` — `engagement_for`: the state → `Encrypt(mode)` / fail-closed / plaintext truth table.
16. `crates/corelink-container/src/byok.rs:1-29` — feature-gated factory returning an `Arc<dyn KmsProvider>`.
17. `crates/corelink-container/src/byok.rs:10` — `#![forbid(unsafe_code)]` on the factory.
18. `crates/corelink-byok/src/byok_aws/real.rs:176-256` — `AwsKmsRealProvider::new`/`with_fips`: unconditional `use_fips(true)` + EXPLICIT static creds + explicit FIPS endpoint (no CF-cold-start-hanging AWS SDK chain).
19. `crates/corelink-byok/src/lib.rs:1-21` — umbrella re-export of the core trait/types + microkernel mutual-exclusion intro.
20. `crates/corelink-byok/src/lib.rs:34-43` — cargo feature reference (`aws`/`gcp`/`azure`/`vault`), default no provider.
21. `crates/corelink-byok/src/lib.rs:93` — `#![forbid(unsafe_code)]` on the umbrella crate surface.
22. `crates/corelink-byok/src/lib.rs:105-139` — compile-time at-most-one-provider `compile_error!` guards.
23. `crates/corelink-container/src/byok_orchestrator.rs:208-219` — `make_provider`: builds the singleton `Arc<dyn KmsProvider>` + boot audit event.
24. `crates/corelink-container/src/byok_orchestrator.rs:223-272` — `build_active`: compile-time `cfg` dispatch over the four `byok-*-real` providers, `InMemoryFake` fallback when none set.
