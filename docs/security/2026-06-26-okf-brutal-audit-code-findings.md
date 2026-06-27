# Code/security findings surfaced by the OKF brutal audit — 2026-06-26 (VERIFIED)

These are **code/product observations** surfaced while adversarially auditing the OKF wiki against the
live code (Round-3 lenses) and then **independently re-verified** by skeptic agents whose job was to
PROVE each finding wrong (Round 4). The severities below are the **post-verification** severities.

> **Verification correction (read first):** Round 3 flagged #1 (attestation) and #2 (batch) as
> *pre-launch blockers*. Adversarial verification **refuted that severity** — both have a true kernel
> but are **NOT** the severe issues first claimed. Neither is a launch blocker. Corrected below.
>
> **2026-06-27 CORRECTION (red-team round, cold-checked) — retracting a propagated hallucination:**
> An earlier "2026-06-27 update" here claimed the code team had FIXED the Stripe re-fingerprint no-op
> ("a real `GET /v1/customers/:id` check at `adapter_stripe.rs:195-230`"). **That was WRONG and is retracted:**
> `adapter_stripe.rs` is only **184 lines long** — `:195-230` does not exist; the claim was a sub-agent
> hallucination I failed to cold-check and then propagated. The real code at
> `adapter_stripe.rs:140-149` STILL returns `Ok(CANONICAL_EMPTY_TENANT_HASH)` unconditionally (comment:
> "GET each customer … **deferred**"). The Stripe re-fingerprint is a **deferred no-op, NOT closed.**
> **This also retracts the V1 refutation of #1 below** ("no sign-while-PII-survives path"): there IS one —
> see #1-REOPENED. Lesson re-learned: cold-check every "it's fixed" claim before recording it.
> Separately, #2's batch hardening gap is STILL OPEN (the "guard now exists at `:843`" claim was also a
> cold-checked hallucination — `CasPutGuard` is only at `cas.rs:722`, not in `handle_batch_write`).

Severity = engineering/security severity, post-verification.

---

## 1-REOPENED. GDPR erasure attestation — **VerifiedComplete can be signed while PII survives** — **HIGH (compliance)**

Round-3 called it "theater"; the round-4 "V1" refutation **over-corrected** to "no sign-while-PII-survives
path." The red-team round (cold-checked) shows the refutation was **wrong**, and the original concern was
closer to right. Three layers, all confirmed in code:

- **The dual gate only protects the backends that actually re-scan.** `VerifiedComplete` requires every
  backend's `verification_hash == CANONICAL_EMPTY_TENANT_HASH` (`orchestrator.rs ~485-510`). D1, R2-CAS,
  and R2-AC genuinely re-scan (`adapter_d1.rs`, `adapter_r2_cas.rs:196-218`). **But the Stripe adapter
  returns the clean sentinel UNCONDITIONALLY without ever querying Stripe** —
  `adapter_stripe.rs:140-149` (`Ok(CANONICAL_EMPTY_TENANT_HASH)`, comment "GET each customer … deferred";
  the file is 184 lines — there is no "real check at :195-230", that was a retracted hallucination). The
  8 not-shipped backends do the same (`adapter_not_applicable.rs`). So a Stripe pseudonymize that silently
  no-ops or partially applies is **never caught**: `VerifiedComplete` is reachable — and a signature minted —
  while the subject's email/name/phone still live in Stripe. The reconciliation that is actually live is a
  check of **3 backends, not 12**.
- **The attestation is fail-OPEN dropped and unserved.** `sign_and_persist` is best-effort fail-open
  (`attestation.rs:108-120` — seed unset/malformed → returns silently, no artifact), yet `handle_verify`
  returns `200 verified_complete` regardless (`routes/dsr.rs ~498-516`). It is **not durably persisted**
  (`erasure_attestations` D1 index row only; migration `0032` has no signature/JCS column; no R2 PutObject)
  and there is **no serving route** (`GET /v1/public/attestation/{id}` + `/keys/erasure/{region}.pub` have
  zero registrations). On any env without `ERASURE_ATTESTATION_SEED_HEX` every "completed" DSR yields
  `verified_complete` + ZERO signed artifact; even when signed it is unfetchable. The customer's "proof" is
  an unsigned, repudiable JSON label.
- **TDK-rotation blindspot (MED):** R2-CAS erase + verify both address via `derive_prefix(load_tdk(), tenant)`
  (`adapter_r2_cas.rs:177` erase, `:205` verify); rotating `R2_TDK_HEX` between write and erase makes both
  look under the WRONG prefix → erase deletes 0, verify counts 0 → `CANONICAL_EMPTY` → `VerifiedComplete`
  while the blobs survive under the old-TDK prefix (the file flags the identical mode for multipart at `:91-99`).

**Status / fix shape.** The deep parts (audit-chain seal WI-S09-007, attestation R2-persist + serving
WI-S11-008, the real Stripe re-fingerprint per ADR-S11-013) are **tracked deferred launch work** — building
them is the real remaining engineering. But the **exploit (false `VerifiedComplete`) is closable now without
building them, by failing HONEST**: a deferred/not-yet-verified backend must return an *indeterminate* marker
that yields `VerifiedPartial`, NOT the clean `CANONICAL_EMPTY` sentinel that yields `VerifiedComplete`; and
`handle_verify` must not return `verified_complete` when the attestation drops in prod. **This changes the
GDPR erasure-verification protocol's semantics → flagged to the Owner as a compliance/legal stakeholder call,
not auto-applied.** The wiki concepts have been corrected to stop claiming `VerifiedComplete` proves
cross-backend PII destruction.

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

---

## Red-team exploit round (2026-06-27) — "build a working chain or prove the wired block"

8 red-teamers attacked the live system (default-broken, no benefit-of-the-doubt). Result:

- **CORE IS SEALED (0 working chains, each block verified wired):** tenant isolation (RT1 — prefix is
  secret-HMAC-keyed, cross-tenant 403 fires before any storage touch, `_public` takes only server-fetched
  digest-verified bytes), auth/privesc (RT2 — the leaked-`PAT_SIGNING_KEY` native forgery is closed by the
  `NativePatGate` Argon2id re-verify, wired into every billable handler + boot-FATAL in prod), money bypass
  (RT3 — `subscription_state='active'` read identically worker+container, webhook sig+timestamp+idempotency,
  fail-open is read-only and grants no paid tier), cache poisoning (RT4 — compute-then-ct_eq on every durable
  surface, no cross-tenant poison). These held under a genuine exploit attempt — a strong positive result.
- **#R7 [HIGH, cross-tenant DoS] OCI rate-limiter unbounded bucket map → shared `_oci` OOM.** The limiter's
  `Arc<Mutex<HashMap<BucketKey,…>>>` has no eviction (`corelink-ratelimit/src/limiter.rs ~196,317`), and the
  OCI bucket key is the attacker-controlled, pre-auth repo string from the URL path
  (`ratelimit_layer.rs:513-556`). `GET /v2/<random>/manifests/…` with distinct repos grows the map without
  bound → OOM-kills the SINGLETON `_oci` container (one DO for all OCI tenants). Inconsistency confirming the
  oversight: the Argon2 map (`PER_TENANT_MAP_CAP=10_000`) and the Turbo maps ARE capped. **Fix in flight**
  (LRU-cap the limiter map, mirroring the Argon2 pattern).
- **#R8 [1 CRITICAL + 2 HIGH + 4 MED] the OKF gate itself is bypassable.** CRITICAL **callee-swap**: a concept
  can cite a line that DELEGATES to an enforcer in an uncited file (`pat-moat` cited `adapter_pat.rs:538-543`,
  but the real HMAC verify is in the uncited `corelink-pat/src/verify.rs`) → rewrite the callee and the
  content-anchor sees the cited line byte-identical = "fresh". HIGH: the C10b route glob is non-recursive
  (misses `routes/dsr/` etc.) and the per-PR CI trigger omits `worker/**`+`apps/**` (edge-plane PRs skip the
  gate). **Fix in flight** (recursive glob, crate-root file-granular, CI paths, an authoring rule that an
  invariant must cite the executed enforcer not a delegate, a C6c test-as-sole-grounding ban).
- **#R6 = #1-REOPENED above** (false `VerifiedComplete`), now HIGH.

**Net (post red-team):** the **core data-plane (isolation / auth / money / integrity) is genuinely hard** —
verified, not asserted. The real open exposure is **compliance**: false `VerifiedComplete` (#1-REOPENED) and
the **mutable, unsealed, unread audit trail** (#5 — the same D1 credential the app uses can rewrite any
`audit_outbox` row undetectably, since nothing seals `emitted_at` / applies R2 Object-Lock). Plus the OCI
DoS (#R7, fix in flight) and our own gate's bypasses (#R8, fix in flight). The deep compliance closures
(audit seal, attestation persist+serve, real Stripe re-fingerprint) are tracked deferred launch work; their
**exploits are closable now by failing honest** (return `VerifiedPartial` for unverified backends) — flagged
to the Owner as a compliance/legal stakeholder call.

---

## Audit #2 (2026-06-27) — code findings (the wiki/gate fixes landed; these are CODE, for the Owner/code-team)

- **[MED, GDPR] `team_member` PII survives erasure (incomplete D1 erase-set).** Migration `0074_team_member`
  carries `email_hash` (HMAC of invitee email) + Clerk `user_id`, tenant-keyed, but appears NOWHERE in the
  DSR erase path (`crates/corelink-container/src/routes/dsr/adapter_d1.rs` erase-set, nor `remaining_rows`).
  On a tenant erasure, member PII survives AND the D1 verify still returns `CANONICAL_EMPTY` → `VerifiedComplete`
  attested over surviving PII. Policy-inconsistent: the project's own ratified standard erases
  `survey_responses.recipient_hash` as tenant-linked PII (`adapter_d1.rs:72-75`), so `email_hash` must be too.
  Even the "D1 verify-PROVEN" plane has an incomplete table-set (0074 postdates the 2026-06-11 erase-set
  cold-verify). **Secondary:** NO exhaustive migration-vs-eraseset completeness gate (only a no-overlap/no-retain
  test), so `pilot_tenants` (0065), `abuse_score_history` (0013) sit unclassified. **Fix:** add `team_member`
  (+ audit the other tenant-linked tables) to the erase-set + `remaining_rows`; add a completeness gate.
- **[MED, latent — server-secret-gated] runner-mint binds entitlement to a caller-supplied `owner_tenant`**
  with no dispatcher→tenant binding, behind the runner_mint key with a SHARED fallback to
  `CORELINK_INTERNAL_AUTH_KEY` (`worker/src/lib/runner_mint.ts:119-133,147-185`). Leak of EITHER key → mint a
  cas:rw PAT for ANY runners-entitled tenant (bounded: cas:rw, 90-min TTL, entitled tenants). **Fix:** require
  the dedicated runner_mint key (no shared fallback) and/or bind `owner_tenant` to the dispatcher.
- **[MED, latent] admin "dual-approval" is a sham two-person control** — `approver` is an UNVERIFIED arbitrary
  body string with no ledger/identity lookup (`admin.rs:642-646` `DualApprovalToken::new(id, approver)`); one
  leaked operator key satisfies "two-person" with any approver string. **Fix:** verify the approver against a
  real operator identity, or drop the dual-approval claim.
- **[LOW, latent] event-log DO `/_eventlog/read` has no per-scope gate** (`event_log_do.ts:286-318`) — IF the
  DO is ever wired (currently UNWIRED, zero callers), any tenant PAT (even cas:read) reads the full event log.
  Pre-wiring design trap.
- **[LOW, auth-gated] uncapped `planned` HashSet** (`ratelimit_layer.rs:307`) — one entry per distinct tenant
  ever seen, never evicted; bounded only by the post-auth tenant boundary. Cap it like the bucket map.

None are externally-reachable script-kiddie chains (team_member requires being the erasing tenant's own member;
the rest need a leaked server secret or a future wiring step). Audit #2 ALSO confirmed the iteration-1 honesty
edits are factually correct (backend split = exactly 3 real-rescan / 9 sentinel) and the new-surface sweep
(Bazel/Turbo/sccache/admin/introspect/webhooks) found **0 externally-reachable chains** — all wired-blocked.
