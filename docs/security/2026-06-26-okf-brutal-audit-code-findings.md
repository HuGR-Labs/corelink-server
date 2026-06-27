# Code/security findings surfaced by the OKF brutal audit (Round 3) — 2026-06-26

These are **code/product observations**, NOT documentation defects — they were surfaced while
adversarially auditing the OKF wiki against the live code (Round-3 lenses B3 "dangerous omissions"
+ B6 "exploit-the-defender"). Each is code-grounded (`path:line` verified in the deployed
`corelink-container` source). The wiki has been corrected to describe the *imperfect reality*; this
doc is the separate **owner/code-team decision list** for whether each becomes a pre-launch fix.

Severity = engineering/security severity (not the wiki's documentation severity).

---

## 1. GDPR erasure attestation signs over fabricated / no-op evidence — **HIGH (compliance/legal)**

The DSR erasure attestation is presented (and persisted, Ed25519-signed) as cryptographic proof of
verifiable deletion. In the live signer it proves only that *a signature was produced*:

- **Stripe arm re-fingerprint is a hardcoded no-op** — `verification_hash` always returns
  `CANONICAL_EMPTY_TENANT_HASH` regardless of the real Stripe account state
  (`crates/corelink-container/src/routes/dsr/adapter_stripe.rs:140-150`). A `VerifiedComplete`
  attestation can therefore be signed **while a tenant's Stripe PII survives**.
- **The signed bundle is a synthetic constant** — the signer hashes a self-referential
  `audit_chain_segment_ids` string with `compute_hash`, NOT the fail-closed `validated_hash`
  (`crates/corelink-container/src/routes/dsr/attestation.rs:139-151`). The "validation fails closed
  on any empty mandatory field" invariant is **vacuous** (the bundle is never empty).
- **Region mis-attribution** — at `VerifiedComplete` the `tenant` row is already deleted, so
  `resolve_region`'s D1 lookup misses and the attestation region falls back to the env default
  `ERASURE_ATTESTATION_REGION` (`attestation.rs:82-102`) — an EU tenant's erasure can be signed with
  the home-region key.

**Why it matters:** this is the artifact a regulator/auditor would trust most, and it is the least
grounded in real deletion evidence. **Recommend: pre-launch fix** — either wire the real
per-backend verification hashes into `validated_hash` and fail closed, or stop persisting/signing the
attestation until it is real (do not ship a signed proof that proves nothing).

## 2. Batch CAS write bypasses the per-tenant concurrency guard — **HIGH (DoS / defense-in-depth)**

The single-object PUT path holds a `CasPutGuard` per-tenant concurrency reservation
(`crates/corelink-container/src/routes/cas.rs:722`, `CAS_WRITE_CONCURRENCY_LIMIT = 8`). The batch
route `handle_batch_write` has **no such guard** (`cas.rs:832-839`). A single tenant can open
unlimited concurrent batch uploads, each buffering up to `BATCH_MAX_BYTES = 8 MiB` — the exact
peak-memory exhaustion the guard exists to bound. **Recommend: pre-launch fix** — apply the same
`CasPutGuard` reservation on the batch path.

## 3. The prod "FATAL" boot watchdog is circular with the controls it guards — **MEDIUM (config-drift safety)**

Prod-detection is `StorageEnv::from_env().is_some() && PAT_SIGNING_KEY set`
(`crates/corelink-container/src/main.rs:246-250`) and the `std::process::exit(1)` backstop fires
**only** for the native PAT gate (`:253-265`). The `$`-ceiling, byte-cap, and request-count guards
each simply return `None` when `StorageEnv::from_env()` is `None` (`tenant_quota.rs:112-113`,
`storage.rs:98-104`) — no boot assertion. So a single dropped/renamed `R2_S3_*` / `D1_DATABASE_ID`
var in a real prod deploy makes prod-detection **false** → the FATAL never fires, the container boots
**without** the Argon2id PAT backstop (a leaked `PAT_SIGNING_KEY` then forges any-tenant native
access) **and** with `$`-ceiling / byte-cap / request-quota all silently off. **The missing config
that disables the controls also disables the watchdog.** **Recommend:** a positive prod-arming
assertion (if the host looks like prod by any independent signal, assert all guards are `Some` or
refuse to boot).

## 4. `_oci` is not in the container fail-closed sentinel set — **MEDIUM**

The wiki (and the design) treat `_public`/`_oci`/etc. as reserved sentinels that must be rejected if
they arrive as a claimed tenant. `crates/corelink-container/src/auth_tenant.rs:19` does **not** include
`_oci`. If `x-corelink-tenant-id: _oci` reaches a native `AuthTenant` handler it is accepted as a real
tenant id rather than 401'd. (Mitigant: the Worker deletes that header on the OCI path; this is the
defense-in-depth backstop that has the hole.) **Recommend:** add `_oci` (and audit the full sentinel
list vs the namespace constants).

## 5. The audit chain has no live producer — **MEDIUM (compliance/integrity)**

`HashChainBuilder::append` (the BLAKE3 tamper-evident link) is **test-only**; the live erasure/audit
path writes plain **unchained** CloudEvents to `audit_outbox`, with chaining deferred to the unbuilt
S-09 drain worker (`crates/corelink-container/src/routes/dsr/audit.rs:7-8,82-84`). The chain logic is
real and property-tested, but the live audit trail is currently mutable, unsealed D1 rows. **Recommend:**
treat "tamper-evident audit log" as not-yet-live for launch claims; prioritise the S-09 drain if the
compliance posture depends on it. (Pairs with #1 — the attestation references this chain.)

## 6. `$`-ceiling is approximate by design (16-op budget lease) — **LOW (revenue, likely accepted)**

`LeasedQuotaStore` (`tenant_quota.rs:112-125,366-621`) pre-debits a 16-op chunk and serves up to ~15
subsequent ops from an in-memory lease without touching D1. This is a deliberate hot-path optimisation,
but it means: the ceiling is consulted in D1 ~once per 16 ops (not per-op atomic), a ceiling
change/revocation has up to ~16-op latency, and a crash can lose the lease (slight under-charge).
**Recommend:** accept (document the bound) unless the under-charge/latency is contractually material.

## 7. OCI surface trust model differs from every other plane — **INFO (verify, likely safe)**

OCI resolves the tenant from the **HMAC-verified bearer token** in-process (`routes/oci.rs:834-841`),
the Worker deletes `x-corelink-tenant-id` (`worker/src/index.ts:1817-1828`), the cap travels inside the
token (`corelink-adapter-host/src/oci/auth.rs:217,289,353`), the rate limiter is a separate per-repo
`oci_limiter` that fail-opens when the header is absent, and no-bearer OCI reads are uncounted. The
bearer is HMAC-verified so tenant-from-bearer is *probably* sound, but this is a wholly separate trust
path that four tenancy concepts described as "header-only". **Recommend:** a focused review that the
OCI bearer→tenant path has the same isolation guarantees the header path does (and that uncounted
anon reads / fail-open rate-limit are acceptable).

---

**Net:** the brutal audit's highest-value output was not wiki typos — it was #1 (attestation theater)
and #2 (batch DoS), both of which I'd treat as pre-launch fixes, plus #3–#5 as owner-judged. Generated
by the OKF brutal Round-3 audit; all line numbers verified in the `feat/okf-wiki-sweep-land` worktree.
