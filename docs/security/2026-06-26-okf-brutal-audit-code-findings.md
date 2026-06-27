# Code/security findings surfaced by the OKF brutal audit — 2026-06-26 (VERIFIED)

These are **code/product observations** surfaced while adversarially auditing the OKF wiki against the
live code (Round-3 lenses) and then **independently re-verified** by skeptic agents whose job was to
PROVE each finding wrong (Round 4). The severities below are the **post-verification** severities.

> **Verification correction (read first):** Round 3 flagged #1 (attestation) and #2 (batch) as
> *pre-launch blockers*. Adversarial verification **refuted that severity** — both have a true kernel
> but are **NOT** the severe issues first claimed. Neither is a launch blocker. Corrected below.
>
> **2026-06-27 update (10x-brutal exhaustive re-read):** the code team has since FIXED the Stripe
> re-fingerprint no-op (#1's old residual is CLOSED — it is now a real `GET /v1/customers/:id` redaction
> check, `adapter_stripe.rs:195-230`). But the exhaustive read found a SHARPER residual on #1 (the signed
> attestation is not durably persisted — see below) and confirmed #2's batch hardening gap is STILL OPEN
> (a sibling agent's "guard now exists" claim was a cold-checked hallucination — `CasPutGuard` is only at
> `cas.rs:722`, not in `handle_batch_write`).

Severity = engineering/security severity, post-verification.

---

## 1. GDPR erasure attestation — **NOT theater** (residual: deferred Stripe re-fingerprint) — **LOW–MED**

Round-3 claimed the attestation signs over fabricated evidence (theater). **Verified WRONG (V1):**
- The `VerifiedComplete` gate is **dual** — `orchestrator.rs:503` requires the canonical-hash check
  **AND** `prior.outcome.is_successful()`. That prior success tombstone is only written if the backend
  erase succeeded **fail-closed** (`adapter.erase(...)?` propagates → no tombstone → `VerifiedPartial`
  → **no signature**, `orchestrator.rs:339,449-477`). **PII surviving the erase BLOCKS VerifiedComplete**
  — there is no sign-while-PII-survives path.
- The signed bundle hard-codes exactly the three fields `validated_hash` checks (`evidence.rs:68-85`),
  so `compute_hash` ≡ `validated_hash` here — not a bypass. The signature is a real Ed25519 binding
  (proven by the crate's tamper tests).
- The region fallback is correct by design (the D1 erase deletes the tenant row; deployment is US-only;
  the published per-region public key still verifies).

**Real residual (sharpened 2026-06-27, BR4 exhaustive read) — MED, compliance-not-yet-live:** the signed
attestation is **NOT durably persisted**. `sign_and_persist` writes ONLY a D1 metadata INDEX row to
`erasure_attestations`, whose schema (`migrations/d1/0032_erasure_attestation.sql`) has **no column for the
Ed25519 signature or the canonical JCS payload** — they are computed then dropped; there is **no R2 PutObject**
(`audit_bucket()` only builds the key string); and the public verify endpoints
`GET /v1/public/keys/erasure/{region}.pub` + `GET /v1/public/attestation/{id}` have **zero route registrations**.
So the wiki's old "portable tamper-evident proof a customer/auditor can verify offline" was hollow — the signing
LOGIC is real, but durable persistence + the serving surface are DEFERRED (WI-S11-008). (The wiki concept has
been rewritten to state this.) Also: the "BYOK CMK-destroy" is not the live flow (`kms_provider="corelink_d1r2_erase"`,
no CMK destroy). **Recommend:** if the GDPR posture depends on a portable offline-verifiable erasure proof, wire
the R2 persist + the serving endpoint (or stop advertising it); the Stripe-no-op part is already CLOSED.

## 2. Batch CAS write lacks the hard concurrency semaphore — **NOT a DoS** — **LOW–MED (hardening)**

Round-3 claimed an unbounded memory DoS. **Verified OVERSTATED (V2), STILL OPEN as a hardening gap (BR3/BR8
2026-06-27):** `handle_batch_write` (`cas.rs:832-839`) still omits the per-tenant `CasPutGuard` 8-slot
semaphore the single PUT holds (`cas.rs:722`) — cold-checked, the gap is real (a claim that "the guard now
exists at :843" was a hallucination). Memory is NOT unbounded: the CAS/AC paths are hard-capped at 10 MiB by
`DefaultBodyLimit` (`main.rs:459-464`). **Caveat (BR3):** 10 MiB is NOT the bound on *every* data-plane path —
the **Turbo PUT path overrides to 100 MiB** (`turbo_v8.rs:103`, carved out at `main.rs:455-458`) and is bounded
by its own `TURBO_PUT_CONCURRENCY_LIMIT` (4/tenant, 16 global) instead. So the batch route has a looser
concurrency bound than the single PUT (rate-limit-derived, not a hard semaphore). **Recommend:** add the
`CasPutGuard` to the batch path as defense-in-depth; not urgent.

## 3. Prod "FATAL" boot watchdog is circular — **MEDIUM (config-drift)** — CONFIRMED

**Verified real (V3):** the only boot `exit(1)` is the native-PAT-gate guard (`main.rs:265`), armed by
`prod = StorageEnv::from_env().is_some() && PAT_SIGNING_KEY` (`:250`). If a `R2_S3_*`/`D1_DATABASE_ID`
var is dropped, `StorageEnv` is `None` → storage falls back to `inmemory` with a mere `warn!`
(`:220-228`, boot does NOT fail) → the `$`-ceiling/byte/request/native-PAT guards all return `None`
(`routes.rs:377-387`) **and** `prod` flips false, disarming the FATAL. **The var whose loss disables the
controls also disables the alarm.** **Recommend:** an independent prod-arming assertion. Owner-judged.

## 4. `_oci` not in the fail-closed sentinel set — **LOW (latent)** — REFUTED as exploit

**Verified NOT exploitable (V3):** the Worker structurally strips `x-corelink-tenant-id` from every
forward and is its sole setter (`index.ts:427-438,473-477`); `_oci` is only a Durable-Object instance
name. No native `AuthTenant` handler can receive `_oci` as a client-supplied tenant. **Latent
belt-and-braces gap, not a live exploit.** **Recommend:** add `_oci` to the sentinel set for hygiene.

## 5. Audit chain has no live producer — **MEDIUM (compliance-not-yet-live)** — CONFIRMED

**Verified real (V3):** the only non-test caller of `HashChainBuilder::append` is `InMemoryR2AuditSink`
(never constructed in the live container build); the live DSR sink writes **unchained** CloudEvents to
`audit_outbox` with `emitted_at = NULL` and no deployed producer seals them (`routes/dsr/audit.rs:82-84`).
Matches CLAUDE.md's own "audit-chain … unwired" caveat. **Recommend:** treat "tamper-evident audit log"
as not-yet-live for launch claims; build the S-09 drain if the compliance posture needs it.

## 6. `$`-ceiling is approximate by design (16-op lease) — **LOW** (likely accepted)

`LeasedQuotaStore` pre-debits a 16-op chunk (`tenant_quota.rs:112-125`). Deliberate hot-path
optimisation → the ceiling is D1-consulted ~once per 16 ops (not per-op), with up to ~16-op
change-latency and slight crash-loss under-charge. **Recommend:** accept + document.

## 7. OCI surface trust model differs from every other plane — **INFO** (verify)

OCI resolves the tenant from the HMAC-verified bearer (`oci.rs:834-841`), header deleted, token-embedded
cap, fail-open rate-limit, uncounted anon reads. Bearer is HMAC-verified so likely sound. **Recommend:**
a focused review that the OCI bearer→tenant path matches the header path's isolation guarantees.

## (product note, not security) SDK network client is a stub

The three first-party SDKs ship only the real client-side BLAKE3 verify core; the network put/get/stat
are explicit stubs (`corelink-wasm/src/lib.rs:180`, `tools/sdks/python/src/lib.rs:110`,
`tools/sdks/go/src/go_bridge.rs`). A customer cannot transfer a blob with the SDK as shipped. Launch/pitch
relevant if the SDKs are sold as a live integration surface. (The wiki concept has been corrected to disclose this.)

---

**Net (post-verification):** there is **no launch-blocker** in this set. The two Round-3 "blockers" were
overstated and are downgraded; the confirmed-real items (#3 circular watchdog, #5 audit-not-sealed) are
MEDIUM defense-in-depth / compliance-not-yet-live, owner-judged. The verification round's value was
precisely catching the over-escalation before it cost engineering time.
