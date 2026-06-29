# BYOK Encryption-at-Rest — SOTA Technical Plan (v1, pre-audit)

**Status:** AUDITED (v2) — two brutal audits (crypto + systems) folded in; see §11 for the verdict + the
mandatory pre-build revisions. **NOT yet cleared to build** until the §11 CRITICALs are designed-in.
**Author:** CoreLink tech lead. **Date:** 2026-06-28.
**Goal:** real customer-key encryption-at-rest that **preserves the content-addressed dedup moat**, is
**fail-closed**, and delivers **crypto-shred erasure** — without a from-scratch redesign.

> This plan is grounded in a code recon (see companion map): the crypto primitives already exist and are
> real (`EnvelopeEncryptor` AES-256-GCM, `DekCache`, `KmsProvider` trait, a genuine `AwsKmsRealProvider`);
> the `byok_envelope` D1 table exists (mig 0030); they are simply **unwired** (CAS PUT/GET handle
> plaintext, the orchestrator's only caller is tests, the table is never written). So this is **wiring +
> two crypto-bug fixes + a convergence layer**, not greenfield.

---

## 0. The core insight (why this is feasible, settling the "dead-end" question)

Dedup needs *same plaintext → same stored object*. Encryption needs *ciphertext reveals nothing*. These
reconcile **inside a closed per-org cache** because CoreLink dedup is **already tenant-scoped** (the R2
key is `region / HMAC_tdk(tenant) / digest` — `r2_s3.rs:453`). Cross-tenant dedup happens **only** via the
explicit `_public` namespace. Therefore:

- **Encrypt only PRIVATE content.** `_public` (deterministic public deps) has no secret → stays plaintext →
  cross-org dedup of public content is **untouched**.
- **Within an org, use a per-org convergence secret** so identical plaintext yields identical ciphertext →
  **intra-org dedup is preserved**. The per-org secret means a *different* org encrypting the same bytes
  gets *different* ciphertext → no cross-org correlation (and cross-org private dedup never happened anyway).

We lose nothing that matters (cross-org dedup of *private* content — which we would never do) and keep the
thing that matters (intra-org dedup + public-dep dedup).

---

## 1. Key hierarchy (3 tiers — envelope + BYOK)

```
CMK  (Customer Master Key)        ── lives in customer KMS (AWS/GCP/Azure/Vault) OR a CoreLink-managed
 │                                    KEK for the "managed" tier. NEVER leaves the KMS. The BYOK root.
 ├─ wraps ─▶ TCS (Tenant Convergence Secret, 32B random, per-tenant)
 │             └─ used ONLY to DERIVE per-blob DEKs (HKDF). Wrapped form in D1; plaintext only in the
 │                ≤300s DekCache window. Enables deterministic dedup without exposing the CMK.
 └─ (Model B only) wraps ─▶ per-blob random DEK directly (no TCS, no dedup)
```

- **CMK** = the customer's key. Customer revokes/deletes it → crypto-shred (everything below dies).
- **TCS** = a per-tenant 32-byte secret, generated once, **wrapped by the CMK** (`KmsProvider::wrap_dek`),
  stored in a new `tenant_byok_secret` row. Unwrapped on demand via KMS, cached ≤300s (existing
  `DekCache` TTL invariant). The TCS is what makes convergence per-org without ever doing HKDF on the CMK.
- **DEK** (Model A) = `HKDF-SHA256(ikm=TCS, salt=tenant_id, info="corelink/byok/v1" ‖ namespace ‖ plaintext_digest)`.
  Deterministic ⇒ same plaintext in the org ⇒ same DEK ⇒ same ciphertext ⇒ dedup hits.

---

## 2. The two policy modes (per-tenant knob)

| Mode | DEK | Nonce | Dedup | Confidentiality trade | Default for |
|---|---|---|---|---|---|
| **A — convergent (dedup-preserving)** | deterministic HKDF(TCS, digest) | deterministic (digest-derived) | ✅ intra-org | leaks *plaintext-equality within the org only* (acceptable: it's their own data) | the cache product |
| **B — max-isolation** | random per blob | random | ❌ none on private content | zero equality leak | enterprise tenants who contractually can't accept (A) |

The mode is a `tenant_byok_config.mode` column. **A is the default** (dedup is the moat). B is an explicit
enterprise opt-in that trades the dedup economics for zero equality-leak.

**GCM nonce-reuse safety for Mode A (critical):** a deterministic nonce is normally fatal for AES-GCM. Here
it is **safe by construction**: in Mode A the nonce is derived from the plaintext digest, so a given
`(DEK, nonce)` pair only ever encrypts *one* plaintext (the one whose digest produced both) → the ciphertext
is always identical → there is no second distinct plaintext under the same `(key,nonce)`, which is the
precondition for the GCM forgery/keystream-reuse attack. This is the standard convergent-encryption result.
Mode B uses a fresh random nonce per blob (no determinism needed).

> ⚠️ **AUDIT BREAK (C-1, CRITICAL) — the safety argument FAILS for multipart/chunked blobs.** CoreLink
> uploads large objects via a real multipart pipeline (`corelink-r2-multipart`) carrying ONE whole-object
> digest. Deriving `(DEK, nonce)` from the *whole-object* digest reuses the same `(DEK,nonce)` across every
> distinct part → catastrophic GCM keystream-recovery + tag forgery. The single-shot argument above is only
> valid for single-shot objects. **MANDATORY REDESIGN (see §11):** each chunk is its own convergent unit —
> `(key_i, nonce_i) = KDF(TCS, namespace, algo, digest(chunk_i), chunk_index)` — never the whole-object
> digest reused across parts; bind `chunk_index` in AAD. This must be designed before any wiring.

---

## 3. Scope — what gets encrypted

| Content | Encrypted? | Why |
|---|---|---|
| `_public` namespace (deterministic public deps) | **No** | no secret to protect; preserves cross-org public dedup |
| Private tenant content, tenant has BYOK **on** | **Yes** (Mode A or B) | the customer-key requirement |
| Private tenant content, tenant has BYOK **off** | No (today's CF-managed R2 encryption) | unchanged for the SMB base |
| AC (action cache) metadata | Phase 2 | start with CAS blobs (the bytes); AC after |

---

## 4. The storage-key hardening (closes the confirmation attack)

Today the R2 key embeds the **raw plaintext digest** (`r2_s3.rs:453`). An attacker with R2 read who can
*guess* a plaintext can confirm its presence by computing its digest — the convergent-encryption
confirmation attack. **Fix for BYOK tenants:** the digest component of the key becomes
`HMAC-SHA256(TCS, plaintext_digest)` (the prefix is already HMAC-keyed; extend it to the digest). Then the
on-disk key reveals nothing guessable without the TCS (which dies with the CMK). The *logical* dedup key
(in-tenant) stays the plaintext digest in the write handler's HEAD check, mapped to the HMAC'd physical key.

---

## 5. The two mandatory crypto-bug fixes (DD-confirmed, fix BEFORE wiring)

1. **Body AEAD binds no AAD** (`envelope.rs:81-83`/`180-181` pass nonce only). Fix: bind
   `AAD = JCS{tenant_id, plaintext_digest, key_id, namespace, mode, v}` into the **body** `encrypt`/`decrypt`,
   not just `wrap_dek`. Without this a ciphertext could be relabeled/moved.
2. **Warm-DEK cache skips the AAD/tenant check** because `CacheKey::from_wrapped` (`dek_cache.rs:60-85`)
   excludes tenant/blob_hash. Fix: cache key = `(key_arn, tenant_id, blob_hash, enc_context_hash)`, and run
   the AAD-match on BOTH the hit and miss arms. Without this a cached DEK is returned for the wrong blob.

---

## 6. Fail-closed semantics (no silent downgrade — the misrepresentation guard)

- BYOK **on** + KMS unreachable on **write** → **REFUSE the write (5xx)**. NEVER fall back to plaintext.
- BYOK **on** + KMS unreachable on **read** → **503**, never serve the raw stored bytes.
- BYOK toggle is **monotonic per tenant within a content epoch**: you cannot silently flip on and leave old
  plaintext blobs looking "encrypted". Turning BYOK on schedules a **backfill** (§7); until backfill
  completes the tenant is `byok=partial` and the account API reports it honestly (no "fully encrypted" claim).
- A boot-time assertion: in prod the active `KmsProvider` MUST NOT be `InMemoryFake`/any XOR mock — refuse
  to start otherwise. (Today the default build IS the fake — this guard prevents shipping it.)

---

## 7. Migration & rollout (additive, reversible-where-safe, armored)

**Phase 0 — primitives hardening (no behavior change):** fix the 2 AAD bugs; add the convergence-secret +
HKDF DEK derivation + deterministic-nonce path to `corelink-byok`; **delete the GCP/Azure/Vault XOR mock
modes** (a half-working cipher is worse than none) keeping only real KMS paths + a clearly test-only fake
behind `#[cfg(test)]`; property-test the round-trip + the convergence determinism + the AAD binding.

**Phase 1 — schema (additive):** new `tenant_byok_config(tenant_id PK, mode, cmk_provider, cmk_key_id,
cmk_region, state, created_at_ms)` + `tenant_byok_secret(tenant_id PK, tcs_wrapped BLOB, cmk_key_id,
wrapped_at_ms)`; reuse `byok_envelope` (0030) for Mode-B per-blob wrapped DEKs. All nullable/new tables —
zero impact on existing tenants.

**Phase 2 — wire the data plane (gated by `tenant_byok_config.state='active'`):** the CAS write handler
(`r2_s3.rs:929`), AFTER content-hash verify and BEFORE the R2 PUT, branches: BYOK-on → encrypt
(`EnvelopeEncryptor`) → PUT ciphertext at the HMAC'd key + (Mode B) write `byok_envelope`. The read handler
(`:720`) branches symmetrically → unwrap → decrypt → verify plaintext digest. BYOK-off tenants + `_public`
→ unchanged plaintext path. Dedup HEAD check uses the HMAC'd physical key.

**Phase 3 — onboarding + backfill:** an admin flow to register a CMK (validate via `check_access` before
activating), generate+wrap the TCS, set `state='active'`, and enqueue a **backfill job** that re-encrypts
existing plaintext blobs (read plaintext → encrypt → PUT ciphertext → swap). Tenant is `partial` until done.

**Phase 4 — crypto-shred erasure mode:** extend DSR erasure (`adapter_r2_cas.rs`) with a `crypto_shred`
mode = revoke/disable the CMK + delete `tenant_byok_secret` + evict the DekCache (`evict_all_for_key`)
→ all DEKs underivable → ciphertext permanently dead (complements, doesn't replace, the physical sweep).

**Rotation:** rotate CMK = re-wrap TCS (+ Mode-B DEKs) under the new CMK version — **no blob re-encryption**.
Standard envelope rotation; supported by `wrap_dek`/`unwrap_dek` already.

---

## 8. KMS provider posture

- **Ship AWS KMS first** (`AwsKmsRealProvider` is genuine `aws_sdk_kms` — `byok_aws/real.rs:278`). Common
  enterprise case.
- **GCP/Azure/Vault:** wire the REAL HTTP paths; **delete the XOR mock modes** before any prod exposure.
- **Managed tier:** for non-enterprise tenants who want "encrypted with a dedicated key we hold" (a step
  above CF-managed, below true BYOK), the CMK is a CoreLink-held KEK in CF Secrets / a managed KMS — same
  code path, different custody. Honest claim ladder: *CF-managed R2* → *managed dedicated key* → *true BYOK
  (customer KMS)* → *HYOK (every op hits customer KMS, no DEK cache)*.

---

## 9. What this explicitly does NOT do (honest scope)

- Does not encrypt `_public` content (by design — it's public).
- Mode A does not hide plaintext-equality **within** an org (documented trade; Mode B for those who need it).
- Does not retroactively encrypt without a backfill (tenant is `partial` until backfill completes — reported honestly).
- Does not make us SOC2-compliant (that's an external audit) — it makes the "encryption-at-rest / BYOK /
  crypto-shred" claim **true**, which is the prerequisite.

---

## 10. Effort estimate (rough, for the build-vs-descope decision)

| Phase | Scope | Size |
|---|---|---|
| 0 | AAD fixes + HKDF/convergence + nonce + mock deletion + proptests | M (1 focused build) |
| 1 | 2 additive migrations + config plumbing | S |
| 2 | wire CAS write/read encrypt/decrypt + key hardening + fail-closed | L (the core) |
| 3 | CMK onboarding + backfill job | M |
| 4 | crypto-shred erasure mode | S (builds on §2) |

**Total: a few focused waves, not a quarter.** The crypto is bought (real primitives); the work is the
convergence layer + the data-plane wiring + the fail-closed/backfill rigor.

---

## 11. AUDIT VERDICT + MANDATORY PRE-BUILD REVISIONS

Two brutal audits (cryptography + systems/data-plane). **Verdict: architecturally sound, SHIPPABLE-WITH-FIXES,
but NOT buildable as v1 — the plan was materially ahead of the code and had real exploitable breaks.** The
fixes below are **mandatory** and are now part of the plan. Phase order is revised: a **Phase 0.5 "make-real"**
precedes any data-plane wiring.

### CRITICAL — must be designed-in before ANY wiring
- **[C-1 crypto] Multipart nonce reuse = catastrophic GCM break.** Per-chunk convergent units (see §2 box);
  never reuse a whole-object-digest `(DEK,nonce)` across parts; bind `chunk_index`+`total_len` in AAD.
- **[C1 systems] Read path re-verifies the STORED bytes before decrypt → 100% read outage for BYOK.**
  (`r2_s3.rs:784-809` runs `verify_content_hash` on ciphertext.) Restructure read: GET → (BYOK? decrypt) →
  verify-plaintext-digest. HEAD/`exists`/`findMissingBlobs` probes MUST skip the re-verify (can't decrypt).
  Note: AES-GCM is not key-committing — the post-**decrypt** plaintext-digest re-verify is the commitment
  defense; it is security-critical, keep it mandatory + constant-time.
- **[C2 systems] Mode A (convergent) is NOT implemented; naive wiring of the existing random-DEK encryptor
  causes DATA LOSS.** The dedup HEAD-skip (`r2_s3.rs:1018`) + a fresh random wrapped-DEK on a duplicate write
  desyncs the `byok_envelope` row from the stored ciphertext → permanently undecryptable. Build the
  deterministic HKDF(TCS) DEK + digest-nonce path FIRST; make the D1 envelope write **idempotent + HEAD-gated
  in lockstep** with the R2 HEAD skip.
- **[C3 systems] Accounting reserves plaintext size but R2 stores ciphertext → quota drift/bypass.**
  (`AccountingCasHandler` is the OUTER decorator, `routes.rs:430`; delete releases the ciphertext size.)
  Encrypt **above** the accountant, OR account the committed (ciphertext) size on both write and delete.
- **[C4 systems] AWS KMS client uses `aws_config::from_env().load()` — the exact pattern banned in CF
  Containers** (`byok_aws/real.rs:185` vs the 60-90s cold-start hang documented at `r2_s3.rs:101`). Mirror
  `r2_s3.rs`: explicit static credentials + explicit FIPS endpoint, never the provider chain.
- **[C-2 crypto] Domain separation is broken.** KDF/AAD bind no `algo` (attacker-chosen `req.algo`), and
  `info` uses raw concatenation (canonical-collision: `"ab"‖"cd"` == `"a"‖"bcd"`). Use **length-framed /
  structured (JCS)** KDF inputs; bind `algo`, `mode`, `tcs_version`/epoch, `surface`.

### HIGH — required for correctness/confidentiality
- **[H-1/H3] Warm-DEK cache returns a DEK on HIT with NO AAD check** (`envelope.rs:125`), cache key omits
  tenant/blob (`dek_cache.rs:60`). Fix: key = `(key_arn, tenant_id, blob_hash, enc_context_hash)` AND run the
  AAD match on BOTH arms.
- **[H-2/H4] AAD set incomplete + body binds no AAD** (`envelope.rs:81` body, `:215` AAD = only
  `{tenant_id, blob_hash}`). Bind into the BODY AEAD a real **JCS** of `{tenant_id, plaintext_digest, key_id,
  namespace, mode, algo, surface, chunk_index, total_len, v}`.
- **[H-4 crypto] §4's HMAC'd-key does NOT close the confirmation attack.** An upload-dedup oracle (write a
  guessed plaintext, observe dedup/quota) + GCM size leakage survive. AND the raw plaintext digest is LOGGED
  (`envelope.rs:101,189`) — stop logging it. Compute the HMAC on-the-fly; NEVER persist a `digest→hmac` D1
  map (re-leaks the digest). Steer low-entropy/secret content to Mode B; consider size-padding for high-value.
- **[H1 systems] Silent-plaintext surfaces.** §7.2 wires only single-shot CAS. AC (`R2AcHandler`) + the
  package-proxy `MoatCache` paths store plaintext while the account claims "encrypted." Gate the "encrypted"
  claim on ALL surfaces, or wire AC+MoatCache before claiming; report per-surface honestly until then.
- **[H5 systems] Control-plane ownership.** `tenant_byok_config`/`_secret` must be **signup-worker-owned**
  (the downgrade authority), state transitions **monotonic + audited**; bind CMK→tenant at activation (not
  just `check_access`) so an attacker can't swap `cmk_key_id` or flip BYOK off→plaintext.
- **[H5/H-5 crypto] Crypto-shred must be schedule-deletion (+ window) or imported-key-material deletion, NOT
  DISABLE** (reversible). Eviction must clear the **TCS** cache (not just per-blob DEKs); state the ≤TTL
  window honestly ("unreadable within N min", not "instant"). HYOK tier ⇒ no DEK/TCS cache.
- **[H2 systems] KMS region vs 5-region CAS.** Pin/validate the CMK region against the tenant residency
  region at onboarding; document the cross-region KMS RTT on cache-miss reads; a CMK region unreachable →
  503. Consider regional KMS replicas.
- **[H6 systems] Customer lockout = build outage.** A fat-fingered IAM policy is indistinguishable from
  shred → all reads 503 → builds hard-fail. Add a grace/alert window distinguishing transient
  (Throttled/ApiError) from hard (Revoked/NotFound); document that BYOK accepts build-outage-on-key-loss;
  consider an optional escrow tier.
- **[H7 systems] Backfill atomicity + near-cap stall.** Idempotent + resumable with a per-blob state marker;
  read dual-probes (old plaintext key + new ciphertext key) during `partial`; backfill PUTs must pre-reserve
  quota headroom (else a near-cap tenant stalls in `partial` forever).

### MED — fix before GA
- **[M-1] Zeroize plaintext buffers post-verify** (`envelope.rs:194` plaintext Vec not zeroized).
- **[M-2 crypto / M2 systems] Boot guard NOW: refuse to start in prod with `InMemoryFake`/`FipsLevel::None`**
  (default build IS the XOR fake, `byok_orchestrator.rs:270`). Two-phase rollback keeps the decrypt path until
  reverse-backfill completes (a code-only revert strands ciphertext).
- **[M1 systems] `_public` poisoning** via the URL-map authority — harden it; confirm no BYOK private write
  resolves to `PUBLIC_NAMESPACE`.
- **[M3] Gate the future multipart-enable PR on BYOK** (same as the erasure gate, `adapter_r2_cas.rs:91`).
- **[M4] Mode-B CMK rotation re-wraps every per-blob DEK** (millions of KMS calls) — scope a batched rotation job.

### Revised effort (post-audit)
The CRITICALs add a **Phase 0.5 (make-real): the convergence/HKDF/chunk-AEAD layer + the 2 AAD fixes + the
read-path reorder + accounting-layer move + KMS-client fix + boot guard** — a substantial L-sized foundation
**before** the §7.2 wiring. Net: still a few focused waves, but Phase 0.5 is the gate and it is non-trivial.
**Recommendation: this is a real, fundable build — but it is an ENTERPRISE-tier investment. Do NOT start it
until the product decision (do we sell the enterprise tier?) is made; until then, descope the BYOK claim.**
```
