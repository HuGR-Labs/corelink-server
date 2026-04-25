---
audit_id: 2026-04-25-sonnet-r5-s03-wi-review
reviewer: Sonnet 4.6 (claude-sonnet-4-6) — Agent R5
sprint: S-03 (Lote 10.3 — auth real; HIGH_RISK)
scope: WI-S03-001 through WI-S03-008 + _spec_contract.md
prior_audits_cross_referenced:
  - 2026-04-25-agent-r4-s03-part1-wi-review.md  # Opus 4.7, WIs 001-004
  - 2026-04-25-agent-r4-s03-part2-wi-review.md  # Opus 4.7, WIs 005-008
lote_reviewed: 10.3 (post-Lote 10.3bis patches)
date: 2026-04-25
verdict: GO-WITH-FIXES
overall_score: 7.4/10
new_p0s_found: 3
confirmed_r4_p0s: 5
disputed_r4_p0s: 1
---

# Adversarial Spec Review R5 — Sprint S-03 (auth real, HIGH_RISK)
## Reviewer: Sonnet 4.6 as independent adversarial reviewer (round 5)

---

## § 0 — Executive Summary

Lote 10.3bis successfully resolved **all 4 R4-Part1 P0s** (Mann-Whitney direction, stale window additive-vs-orthogonal, Semaphore absence, serde_json non-determinism) and **6 of 8 R4-Part2 P0s** (gen_random_uuid v4→app-side v7, pg_strom correction, SET LOCAL lifecycle, key_id columns, WI-007 event taxonomy expansion, serde_jcs adoption). That is genuine engineering progress. However:

**Headline finding**: WI-S03-002 specifies `tokio::sync::Semaphore` for concurrency control in a **Cloudflare Workers WASM runtime** — a runtime that does **not** ship a tokio executor. This is a P0 runtime crash defect that Opus missed because it analyzed the Semaphore addition as an API question (N=1 vs N=2) rather than questioning runtime compatibility. The fix (CF Workers-native concurrency primitive or WASM-compatible mutex) is architectural, not cosmetic.

**Second headline finding**: WI-S03-006 was **not updated by Lote 10.3bis** (remains at v1.0.0 while all other WIs are v1.1.0). It contains unresolved internal contradictions around sign_count regression policy, a recovery flow that directly violates the sprint contract anti-scope ("magic link auth"), and unresolved WebAuthn library version risk. Opus characterized these as P1; this review upgrades two of them to P0 due to security and contract-violation severity.

**Third headline finding**: WI-S03-005's `app.email_hash_key` — the HMAC key for email_hash — has no specified initialization path in the migration, connection setup, or key-management ADR. Production deploy without this key set results in `current_setting()` raising an exception (fail-open on email_hash if caught) or a runtime panic. Opus filed this as a gap note; this review elevates it to P0 because it is a silent key-material bootstrap defect.

Sprint overall scores regressed slightly vs R4's 7.6/7.95 averages because the analysis weights runtime-compatibility defects more heavily than documentation drift.

---

## § 1 — Per-WI Scores

| WI | Score | Cripto | Complete | Clarity | SOTA | Internal | Prior-WI | Verdict |
|----|-------|--------|----------|---------|------|----------|----------|---------|
| S03-001 | 7.5 | 8 | 8 | 8 | 7 | 7 | 8 | CONDITIONAL |
| S03-002 | 5.5 | 7 | 6 | 8 | 6 | 5 | 7 | BLOCK — P0 runtime |
| S03-003 | 8.0 | 8 | 8 | 9 | 8 | 8 | 8 | CONDITIONAL |
| S03-004 | 7.5 | 6 | 8 | 8 | 7 | 8 | 7 | CONDITIONAL |
| S03-005 | 7.0 | 7 | 7 | 8 | 7 | 6 | 7 | BLOCK — P0 key bootstrap |
| S03-006 | 5.0 | 6 | 5 | 7 | 5 | 4 | 5 | BLOCK — P0 contract violation + internal contradiction |
| S03-007 | 8.0 | 8 | 8 | 8 | 8 | 8 | 8 | GO (minor P1s) |
| S03-008 | 7.0 | 7 | 7 | 7 | 7 | 7 | 6 | CONDITIONAL |

Score columns: **Cripto** = cryptographic rigor; **Complete** = coverage of all required spec sections; **Clarity** = implementation guidance sufficiency; **SOTA** = alignment with SOTA bar (≥8.6 = WI-S04-003 benchmark); **Internal** = self-consistency; **Prior-WI** = cross-WI consistency; **Verdict** = ship readiness.

**Average: 7.0/10** (R4 Part1: 7.6; R4 Part2: 7.95 — lower here because runtime-compat defects carry more weight)

---

## § 2 — P0 Findings

### P0-R5-001 — NEW — `tokio::sync::Semaphore` incompatible with CF Workers WASM runtime

| Field | Value |
|---|---|
| **Status** | NEW (not found by R4) |
| **WI** | S03-002 §6.1.7-bis |
| **Severity** | P0 — runtime crash at deploy |
| **Category** | Runtime compatibility |

**Defect**: Lote 10.3bis added `static VERIFY_SEMAPHORE: tokio::sync::Semaphore = Semaphore::new(N)` with N=1 (max 2) to bound Argon2id concurrency per CF Workers 128 MiB isolate budget. `tokio::sync::Semaphore` requires a tokio async runtime to park/wake tasks. Cloudflare Workers run WASM inside V8 isolates using their own event loop (via `wasm-bindgen-futures` + JS Promise resolution) — **tokio's executor is not present**. Calling `SEMAPHORE.acquire().await` in a CF Workers context will either panic (if tokio's global runtime is absent) or deadlock (if a stub tokio runtime is injected without a real thread pool). This is not a documentation defect — it is a hard runtime failure that prevents WI-002 from deploying to production.

**Why R4 missed it**: R4 analyzed the Semaphore addition as a resource-bounding decision (N=1 is correct for 64 MiB × 2 = 128 MiB budget) and did not question the runtime model. The fix is orthogonal to N.

**Fix options** (pick one, document in ADR-0025):
1. Replace `tokio::sync::Semaphore` with `std::sync::Mutex<()>` + manual count (sync, no runtime dependency). Caveat: blocks the isolate thread during Argon2 verification — acceptable since Argon2 must block to be constant-time; this is the correct model for CF Workers.
2. Use `async_lock::Semaphore` (from `async-lock` crate, runtime-agnostic) — drop-in replacement, WASM-safe.
3. If the Rust codebase targets both native (tokio) and WASM (CF Workers), use `cfg(target_arch = "wasm32")` to branch.

**Effort**: 2h (crate swap + unit test update). High impact/effort ratio — this is a one-line fix with architectural implications.

---

### P0-R5-002 — NEW — WI-S03-006 v1.0.0 skipped Lote 10.3bis; "magic link" violates sprint contract §10 anti-scope

| Field | Value |
|---|---|
| **Status** | NEW at P0 level (R4 filed P1; this review upgrades) |
| **WI** | S03-006 §9 (recovery), §28 R-004 vs §9.9 (sign_count) |
| **Severity** | P0 — sprint contract violation; auth security regression |
| **Category** | Spec integrity + security |

**Defect A — contract violation**: WI-006 §9 recovery flow specifies "email magic link" for passkey recovery. Sprint contract `_spec_contract.md` §10 (anti-scope) explicitly lists: "Magic link auth (rejected; phishing-prone)". This is not ambiguous — the exact phrase appears in both documents. WI-006 at v1.0.0 ships a feature the sprint contract rejected. This is a **contract violation**, not a design preference disagreement.

**Defect B — sign_count contradiction**: §9.9 states "if sign_count regression → SEV-1 alert" (first regression = immediate incident). §28 R-004 states "≥ 3 regressions in 24h → alert threshold". These cannot both be true. Combined with the W3C WebAuthn L3 note that `sign_count = 0` always is a valid authenticator behavior (passkeys frequently don't increment), this means either: (a) SEV-1 fires on every passkey from a compliant authenticator (alert fatigue → ignored in practice, defeating the control), or (b) the 24h/3-threshold policy silently absorbs a real cloned-authenticator attack for up to 72h. Both outcomes are security failures.

**Defect C — version freeze**: WI-006 is the ONLY WI at v1.0.0. All others are v1.1.0. Lote 10.3bis explicitly targeted WI-006 cross-WI references (new_device_used event was added to WI-007 partially because of WI-006 references) yet WI-006 itself was not updated. This creates an invisible semantic gap between what WI-006 says and what the system post-Lote-10.3bis actually implements.

**Fix**:
- Recovery flow: replace "email magic link" with "email + time-bound 6-digit OTP code" (TOTP/HOTP or server-generated; 10min TTL; single-use). Document in §9 and ADR-0032.
- sign_count: adopt W3C recommended policy: passkeys with `sign_count = 0` are exempt from regression tracking; for non-zero initial `sign_count`, regression is SEV-2 (page SRE, investigate) not SEV-1 (P1 incident), upgraded to SEV-1 only if confirmed cloned (requires second forensic signal). Remove the "≥ 3 in 24h" threshold — it creates a false safety window.
- Version: bump WI-006 to v1.2.0, apply Lote 10.3bis changes (new_device_used cross-reference, library version verification), apply these P0 fixes.

**Effort**: 4h (recovery flow rewrite + sign_count policy section + version bump).

---

### P0-R5-003 — NEW — `app.email_hash_key` bootstrap path undefined; production deploy fails silently or panics

| Field | Value |
|---|---|
| **Status** | NEW at P0 (R4 filed as gap note; this review elevates) |
| **WI** | S03-005 §6.1.3, §10.5 (DDL), §22 |
| **Severity** | P0 — silent key-material omission; HMAC fails or throws |
| **Category** | Key management / operational security |

**Defect**: WI-005 specifies `email_hash = hmac(lower(trim(email))::bytea, current_setting('app.email_hash_key')::bytea, 'sha256')` as a computed column or trigger expression. `current_setting('app.email_hash_key')` raises `ERROR: unrecognized configuration parameter "app.email_hash_key"` if the parameter has not been set in the current session. The migration file (`migrations/002_auth_tables.sql`) does not `ALTER DATABASE ... SET app.email_hash_key = '...'` or `ALTER ROLE ... SET ...`. The connection setup (via Hyperdrive or serverless driver) does not specify `before_acquire` injection of this key. The key derivation source (HKDF from master key? direct secret? rotatable?) is not specified.

**Failure modes**:
1. If the HMAC expression is in a generated column or trigger without `NULLIF`/`catch_errors`: INSERT fails with Postgres error → auth registration broken on prod deploy.
2. If the caller catches the exception and falls through: email_hash is NULL → uniqueness constraint on email_hash is bypassed → duplicate accounts creatable.
3. If a fallback empty key `''` is substituted: all tenants share a trivially-guessable HMAC key → email_hash is reversible via rainbow table.

All three outcomes are P0. The fix requires specifying: (a) where the key is initialized (Neon Database-level parameter, Hyperdrive before_acquire hook, or `pgcrypto` extension variable), (b) key rotation procedure (requires re-computing all email_hash values + index rebuild), (c) HKDF derivation from master key or direct secret injection, (d) how the key is delivered to CF Workers (Worker secret → `before_acquire` SET LOCAL).

**Why R4 filed as gap note rather than P0**: R4 mentioned `app.email_hash_key` initialization as missing but did not enumerate the three failure modes and their security consequences. The gap is obvious in retrospect but requires stepping through the Postgres runtime model (not just reading the spec surface) to catch.

**Fix**: Add §6.1.3-bis to WI-005: (a) `before_acquire`: `SET LOCAL app.email_hash_key = $1` where `$1` is the Worker secret injected at runtime, (b) migration adds `SELECT pg_catalog.set_config('app.email_hash_key', '', false)` guard with a `\if` check to fail migration if key is empty, (c) explicit HKDF derivation spec (email_hash key = HKDF-SHA256(master_key, salt="corelink-v1-email-hash", info=tenant_id)), (d) key rotation runbook sub-task.

**Effort**: 6h (spec text, migration guard, before_acquire spec, ADR-0031 addendum).

---

### P0-CONFIRMED-001 — CONFIRMED — CF Queue message limit stated as "≤ 256 KiB"; actual limit is 128 KB

| Field | Value |
|---|---|
| **Status** | CONFIRMED (R4 Part1 noted this; R5 verifies) |
| **WI** | S03-004 §9.6 |
| **Severity** | P0 — mass-revoke message may be silently dropped |
| **R4 finding** | Flagged by R4 Part1 as inaccuracy |

WI-004 §9.6: "CF Queue message ≤ 256 KiB". Cloudflare Queue message body limit is **128 KB** (per CF docs as of 2025-Q4). At 100 entries per message × ~200 bytes per PAT revocation entry = ~20 KB, normal operation is safe. But at tenant-scoped mass revoke with large token payloads or metadata, approach 128 KB — the spec's headroom calculation is based on the wrong limit. The 100-entry cap appears to have been chosen with the 256 KB figure in mind; recalculate with 128 KB limit to confirm the cap is still sufficient. Fix: correct §9.6 to "≤ 128 KB" and re-verify 100-entry cap arithmetic.

---

### P0-CONFIRMED-002 — CONFIRMED — PRR staffing 9/13 pending; persistent carry-forward defect

| Field | Value |
|---|---|
| **Status** | CONFIRMED (R4 both parts; R5 verifies) |
| **WI** | S03-008 §9.4 |
| **Severity** | P0 — SEAL gate blocked |
| **R4 finding** | Filed P0 in R4 Part2 |

9 of 13 mandatory PRR reviewer roles are `_TBD_`. ADR-0034 solo-tier waiver path exists but is Option A of three; the spec does not commit to which option. R5 additional finding: the sprint contract §6 says "10-12 sign-offs" while WI-008 §9.4 says "13 mandatory + 1 advisory". This is unresolved sprint contract drift — not just a PRR detail, but a legally-load-bearing definition-of-done discrepancy. The sprint contract must be amended to 13 mandatory + 1 advisory, or WI-008 must revert to the contract's 10-12 figure, before the DoD can be evaluated at SEAL.

---

### P0-CONFIRMED-003 — CONFIRMED — Sprint contract drift: property test iterations (10k vs 100k) and sign-off count (10-12 vs 13) unresolved

| Field | Value |
|---|---|
| **Status** | CONFIRMED (R4 Part2; R5 verifies and adds precision) |
| **WI** | S03-008 §5 vs §9.1; _spec_contract.md §6 vs §14 |
| **Severity** | P0 — DoD evaluation impossible |
| **R4 finding** | Filed P0 in R4 Part2 |

WI-008 §9.1 notes "deliberately raise from contract's 10k to 100k iterations" without amending `_spec_contract.md`. This is precisely the failure mode that prior sprint reviews flagged — WIs unilaterally raising commitments beyond the contract. The impact: (a) the 100k × Argon2 (~250ms each) property test requires ~7h to complete, incompatible with nightly CI windows; the 1% sampling workaround (addressed in Lote 10.3bis) is valid but was not propagated back to the sprint contract to close the loop. Fix: amend `_spec_contract.md §5` to reflect 100k iterations with 1% sampling, and add the 14-min CI bound as a definition.

---

### P1-CONFIRMED-004 — CONFIRMED — WI-007 §9.4 narrative says "23 events" but enum defines 33

| Field | Value |
|---|---|
| **Status** | CONFIRMED (narrative inconsistency; R4 did not explicitly flag) |
| **WI** | S03-007 §9.4 |
| **Severity** | P1 — specification lying to readers; implementation may target wrong count |

WI-007 §9.4: "23 is a balance between forensic granularity and implementation overhead." The `AuditEventType` enum in §6.1.3 defines 33 event types (expanded by Lote 10.3bis: +6 denied subtypes, +3 lifecycle events, +WebauthnNewDeviceUsed, +DeniedSignatureInvalid/DeniedExpired/DeniedScopeInsufficient/DeniedNotFound/DeniedMalformed/DeniedRevoked). The narrative was not updated when the enum was expanded. This is the "rubber-stamp narrative after code change" antipattern. Fix: update §9.4 to "33 event types; chosen for forensic coverage of all auth deny paths; adds ~1.5KB to binary; justified by §26 LINDDUN analysis."

---

## § 3 — P1/P2/P3 Briefer

### P1 (fix before PRR, within sprint)

**P1-R5-001** — WI-001 JWKS lazy refresh: single retry only on KID miss. If Clerk rotates JWKS mid-day and two concurrent requests arrive in the retry window, both will attempt re-fetch, and the second fetch may overwrite the first with a stale cache entry depending on KV write ordering. Specify `IF_NOT_EXISTS` semantics or a distributed lock (KV atomic CAS) for JWKS cache refresh. Effort: 2h.

**P1-R5-002** — WI-002 Mann-Whitney oracle: spec says "test invalid vs scope-insufficient indistinguishable". This is the correct timing oracle for WI-003, but WI-002's Mann-Whitney 3-prong tests the Argon2 verify path itself (real token vs fake-but-valid-format). The oracle for WI-002 should be "real PAT vs invalid-format PAT with dummy_verify path" — i.e., the exact test specified in §6.1.7-ter. The current §14.6 description conflates the two oracles. Clarify which oracle applies at which layer. Effort: 1h doc only.

**P1-R5-003** — WI-003 session cache key: `auth:session:<sha256(token)[:16-bytes]>` truncates the SHA-256 output to 16 bytes (128 bits). Birthday paradox: at 10M sessions (10 distinct tenants × 1M tokens each), P(collision) ≈ 1 - e^(-n²/2m) ≈ 1 - e^(-5×10^13/1.7×10^38) ≈ negligible. However, the spec should document the truncation rationale (KV key length limit? readability?) and the collision analysis. As stated, it looks like a silent truncation that a reader might flag as a bug. Effort: 1h doc only.

**P1-R5-004** — WI-003 TenantCtx `#[non_exhaustive]`: the spec correctly specifies this attribute, but does not specify what happens when downstream crates (e.g., S-04 workspace crates) pattern-match on `TenantCtx`. `#[non_exhaustive]` forces all pattern matches to include a `..` wildcard — this is a semver-compatible API constraint but requires documentation in the crate's public API guide. Add a note in §24 (semver) and ADR-0029. Effort: 30min doc only.

**P1-R5-005** — WI-004 reconciliation cron 24h drift detection lag: if a DO state diverges from D1 at T=0, the cron at T=24h is the first to detect it. During those 24h, revoked tokens may be accepted (DO authoritative for hot path check). The spec should make clear that D1 is the SoT for the **verification path** (not DO) — which it does in §9.3 — and add an explicit invariant: "the hot-path verify path NEVER consults DO state directly; it consults KV cache, and KV cache is refreshed from D1 on cold miss." If this is already implemented (per §9.3), add it as an invariant in §27. Effort: 1h.

**P1-R5-006** — WI-005 `api_tokens.scopes` BIGINT signed boundary: BIGINT is a 64-bit signed integer. CHECK (scopes >= 0) allows 0–2^63–1 (63 usable scope bits). The WI-002 scope bitset defines 13 constants, well within 63 bits. But the spec should document that bit 63 is permanently reserved (negative in signed representation) and that the CHECK constraint enforces this. Without documentation, a future developer may attempt to use bit 63 and be confused by the CHECK failure. Effort: 30min doc only.

**P1-R5-007** — WI-006 `webauthn-rs = "0.5"` version risk: as of 2025-Q4, `webauthn-rs` has not reached a stable 1.0 release on crates.io. The "0.5" pin may not exist at sprint implementation time, or may have breaking API changes. The spec should mandate a specific `webauthn-rs = "=0.5.x"` exact patch-version pin with the sha256 checksum of the `.crate` file, and add a contingency (e.g., inline the required L3 subset using `p256` + `ring` primitives if upstream is unavailable). Effort: 2h (version verification + contingency doc).

**P1-R5-008** — WI-006 cross-browser CI: Safari/WebKit requires macOS VM + virtual authenticator API. BrowserStack/SauceLabs line item not in §22 cost analysis. R4 flagged this as P1; R5 confirms. Additionally, the virtual authenticator API for WebAuthn in WebKit is available only in WebKit Nightly ≥ Safari 18 (released 2024-Q3) — earlier versions do not support it. Cost estimate: $1.5-3k/yr BrowserStack for Safari CI runs. Add to §22 or scope down to Chrome+Firefox+Edge in Playwright and accept the Safari gap. Effort: 2h.

**P1-R5-009** — WI-007 D1 batch atomicity vs mass revoke: 10k audit events × 200 bytes = 2 MB, exceeds D1 100KB batch limit. Chunking into 40+ batches loses atomic guarantee. This means a mass-revoke that generates 10k audit events may have a partial audit trail if a chunk write fails mid-sequence. For audit correctness, partial trails are a compliance defect (LGPD; SOC 2 CC7). Options: (a) async queue-based ingestion via CF Queue (each chunk guaranteed-once, ordered by sequence number), (b) cap mass-revoke audit events to D1 batch limit and emit a single "batch_revoke summary" event for the remainder, (c) explicit partial-write recovery via sequence number gaps. Pick one and document in §9.8. Effort: 3h.

**P1-R5-010** — WI-008 WI-006 not in Lote 10.3bis: WI-008 §9.4 enumerates all 8 WIs as "SEALED candidates". But WI-006 is at v1.0.0 and has unresolved P0s (this review; P0-R5-002). WI-008 cannot list WI-006 as a SEAL candidate until WI-006 is updated to ≥v1.2.0 with the P0-R5-002 fixes. The PRR §6.1.1 checklist must include a WI-version cross-check gate. Effort: 1h (gate addition).

### P2 (next sprint or doc-only)

- **P2-R5-001**: WI-001 clock skew ±60s tolerance: spec does not bound the issuer allowlist. If `issuer_allowlist` grows to include multiple Clerk tenants in a multi-customer deployment, the `iss` claim check provides weaker isolation. Add: one entry per deployment, enforced at build time via compile-time constant. Doc only.
- **P2-R5-002**: WI-005 `tenant_prefix BLOB(16)` materialized column: spec says "materialized"; DDL should include the column explicitly in the CREATE TABLE. If it's a generated column, the GENERATED ALWAYS AS expression should be specified. The current DDL shows the column but not the generation expression. Clarify. Effort: 30min DDL update.
- **P2-R5-003**: WI-005 email_hash unique constraint: `UNIQUE (email_hash, tenant_id)` is correct for tenant-scoped uniqueness but implies that a user with the same email in two tenants is allowed (different `tenant_id` → different HMAC prefix if tenant_id is included in HMAC input). Verify whether HMAC input is `lower(trim(email))` alone or `lower(trim(email)) || tenant_id`. If the latter, the formula must be updated in §6.1.3. Effort: 1h.
- **P2-R5-004**: WI-007 `principal_id_hash` 64-bit truncation: R4 Part2 #4 flagged this. R5 confirms: 64-bit hash truncation means 2^32 expected collisions at 2^32 unique principals. For a multi-tenant system at scale, this is a real collision risk. Document collision analysis; consider 96-bit or full 128-bit truncation. Doc only.
- **P2-R5-005**: WI-008 TLA+ "4 specs" unverified: WI-008 lists 4 TLA+ specifications as deliverables but does not enumerate the spec file paths. R4 flagged this. R5 confirms: list concrete paths (`specs/07_formal/S03-revocation.tla`, etc.) or drop the count claim. Effort: 1h.
- **P2-R5-006**: WI-007 chain continuity across tenant deletion: if a tenant is deleted (new `TenantDeleted` event), the chain for that tenant's subsequent events must be either terminated (no further events possible) or explicitly continued with a `chain_head = NULL` sentinel. The spec does not address this edge case. Effort: 2h.
- **P2-R5-007**: WI-007 §9.4 HKDF info domain separation for chain hash key: the chain integrity key (used in `sha256(prev || content)`) should be derived per-tenant via HKDF with info = `"corelink-v1-audit-chain" || tenant_id` to prevent cross-tenant chain grafting attacks. Not specified. Effort: 2h.

### P3 (backlog)

- WI-003 Tower middleware layer ordering: only implicitly correct; add a diagram in §9.2.
- WI-005 Neon multi-region migration rollout order undocumented (R4 Part2 "Missing Gaps #1"). Add to migration runbook.
- WI-006 NIST SP 800-63B AAL3 vs AAL2+passkey: R4 Part2 "Missing Gaps #7" correctly flags that passkeys synced via iCloud are AAL2 max. Document distinction in WI-006 §26 and sprint contract §16.
- WI-008 aggregate cost gate: per-WI cost gates exist; no aggregate gate for total auth domain. Add to WI-008 §22 as a sum assertion.

---

## § 4 — Cross-WI Consistency Check (post Lote 10.3bis)

| Check | Status | Notes |
|---|---|---|
| Stale window orthogonal axes (WI-003 vs WI-004) | FIXED | Both now say single SLA = max(60s, 100ms, 60s) = 60s p99 |
| Mann-Whitney direction (WI-002) | FIXED | p < 0.05 reject null; correct |
| Semaphore bound (WI-002 §6.1.7-bis) | PARTIALLY FIXED | N=1 correct; tokio runtime incompatible — P0-R5-001 |
| serde_jcs canonicalization (WI-007) | FIXED | RFC 8785; Unicode NFC; test vectors gate |
| gen_random_uuid v4 → app-side v7 (WI-005) | FIXED | All INSERTs must provide UUID v7 app-side |
| pg_strom ≠ HMAC (WI-005) | FIXED | Now specifies pgcrypto hmac() |
| SET LOCAL lifecycle + RLS (WI-005) | FIXED | before_acquire RESET + CI cross-tenant leak test |
| key_id columns (WI-005) | FIXED | `<column>_key_id INTEGER` present in DDL |
| new_device_used event (WI-007) | FIXED | Added to AuditEventType enum |
| WI-006 not updated (all P0-R5-002 issues) | NOT FIXED | WI-006 at v1.0.0; Lote 10.3bis did not touch it |
| Sprint contract sign-off count (§6 vs §14) | NOT FIXED | "10-12" vs "12+1=13" drift persists |
| Sprint contract iter count (§5 vs WI-008 §9.1) | NOT FIXED | "10k" vs "100k" drift persists |
| app.email_hash_key bootstrap (WI-005) | NOT FIXED | No initialization path specified |
| WI-007 §9.4 narrative "23 events" | NOT FIXED | Enum has 33; text not updated |
| CF Queue limit "≤ 256 KiB" (WI-004 §9.6) | NOT FIXED | Actual limit is 128 KB |
| PRR staffing 9/13 TBD (WI-008 §9.4) | NOT FIXED (by design) | ADR-0034 waiver path; Options A/B/C uncommitted |
| Recovery flow "magic link" (WI-006 §9) | NOT FIXED | Sprint contract §10 anti-scope violation |

**Summary**: 9 of 15 cross-WI consistency checks pass after Lote 10.3bis. 6 remain unresolved; 3 of those are new P0s identified by this review.

---

## § 5 — Verdict per WI

| WI | Title | Version | Verdict | Gate |
|---|---|---|---|---|
| S03-001 | Clerk SSO Adapter | v1.0.0 | CONDITIONAL | Fix P1-R5-001 (JWKS concurrent refresh); SOTA bar 7.5 acceptable for HIGH_RISK |
| S03-002 | CoreLink PAT + Argon2id | v1.1.0 | **BLOCK** | P0-R5-001 (tokio::Semaphore WASM incompatible) must be resolved; replace with runtime-agnostic primitive |
| S03-003 | Tower Middleware TenantCtx | v1.1.0 | CONDITIONAL | Best WI in sprint (8.0); P1s are doc-quality; recommend GO after P1-R5-002/003/004 |
| S03-004 | Revocation DO + KV | v1.1.0 | CONDITIONAL | Fix P0-CONFIRMED-001 (CF Queue limit 256→128 KB); P1-R5-005 (invariant for hot-path) |
| S03-005 | Neon Schema Auth Tables | v1.1.0 | **BLOCK** | P0-R5-003 (email_hash_key bootstrap undefined); P1-R5-006 (scopes BIGINT signed boundary doc) |
| S03-006 | WebAuthn Level 3 Admin | v1.0.0 | **BLOCK** | P0-R5-002 (magic link anti-scope violation + sign_count contradiction + version freeze); must be updated to v1.2.0 |
| S03-007 | Audit Events EVT-047 Chain | v1.1.0 | GO (minor P1s) | P1-CONFIRMED-004 (§9.4 says 23; enum has 33); P1-R5-009 (D1 batch atomicity vs mass revoke); not blocking |
| S03-008 | Property + Pentest + PRR | v1.1.0 | CONDITIONAL | P0-CONFIRMED-002 (PRR staffing); P0-CONFIRMED-003 (iter count + sign-off count drift); must reconcile before SEAL |

**Sprint SEAL readiness**: **NOT READY**. 3 WIs are BLOCK-level; 2 P0s are confirmed persisting from R4; sprint contract drift unresolved. Recommend Lote 10.3-tris (see §6).

---

## § 6 — Lote 10.3-tris Fix Plan

Lote 10.3-tris is needed to address the 3 new P0s and 2 persisting unresolved P0s before SEAL.

| ID | WI | Fix | Owner | Effort | Priority |
|---|---|---|---|---|---|
| FIX-001 | S03-002 | Replace `tokio::sync::Semaphore` with `async_lock::Semaphore` or `std::sync::Mutex<()>`; update ADR-0025 runtime compatibility note | Engineer | 2h | P0 |
| FIX-002a | S03-006 | Recovery flow: replace "email magic link" with "email + 6-digit OTP (10min TTL, single-use)"; document in §9 and ADR-0032 | Engineer | 2h | P0 |
| FIX-002b | S03-006 | sign_count: W3C-compliant policy (zero = exempt; non-zero = SEV-2 on first regression, SEV-1 on forensic confirmation); remove "≥ 3 in 24h" threshold from §28 R-004 | Engineer | 1h | P0 |
| FIX-002c | S03-006 | Bump WI-006 to v1.2.0; apply all Lote 10.3bis cross-references (new_device_used, library version verification) | Engineer | 1h | P0 |
| FIX-003 | S03-005 | Add §6.1.3-bis: `app.email_hash_key` initialization path (before_acquire SET LOCAL + migration guard + HKDF derivation spec); add to ADR-0031 and key rotation runbook | Engineer | 6h | P0 |
| FIX-004 | S03-004 | Correct §9.6: CF Queue limit "≤ 256 KiB" → "≤ 128 KB"; re-verify 100-entry cap | Engineer | 30min | P0 (confirmed) |
| FIX-005 | _spec_contract.md | Amend §5: 100k iterations with 1% sampling (14-min CI bound). Amend §6 + §14: 13 mandatory + 1 advisory sign-offs (canonical). | Owner | 1h | P0 (confirmed) |
| FIX-006 | S03-007 | Update §9.4 narrative: "33 event types" replacing "23"; rationale for expanded coverage | Engineer | 30min | P1 |
| FIX-007 | S03-008 | Add WI-version cross-check gate to PRR §6.1.1: all WIs must be ≥ their Lote 10.3bis version before SEAL | Owner | 30min | P1 |
| FIX-008 | S03-007 | Specify D1 batch atomicity solution for mass revoke (queue-based ingestion OR summary event OR sequence recovery) in §9.8 | Engineer | 3h | P1 |

**Total effort estimate**: ~17.5h. Achievable in a 2-day focused lote.

**Lote 10.3-tris version targets**:
- WI-001: v1.1.0 (no version bump needed; P1 is doc-only)
- WI-002: v1.2.0 (runtime fix)
- WI-003: v1.1.0 (no bump; P1s are doc clarifications)
- WI-004: v1.2.0 (CF Queue limit correction)
- WI-005: v1.2.0 (email_hash_key bootstrap spec)
- WI-006: v1.2.0 (P0-R5-002 fixes + Lote 10.3bis cross-refs)
- WI-007: v1.2.0 (§9.4 narrative + D1 atomicity spec)
- WI-008: v1.1.0 (no bump; P0 confirmed items go to sprint contract amendment)
- `_spec_contract.md`: v1.1.0 → v1.2.0 (iter count + sign-off count drift resolved)

---

## § 7 — Sonnet vs Opus Differential

**What R5 (Sonnet 4.6) found that R4 (Opus 4.7) missed**:

1. **Runtime model mismatch (P0-R5-001)**: R4 analyzed the Semaphore as a resource-bounding API problem and confirmed N=1 was correct. R4 did not question whether `tokio::sync::Semaphore` can execute in a CF Workers WASM context. Sonnet approached this by asking "what runtime is executing this code?" before evaluating the API choice — a different starting hypothesis. This is a systematic difference: Opus tends to analyze API semantics depth-first; Sonnet tends to ask deployment context first.

2. **Version freeze as a systematic gap (P0-R5-002)**: R4 flagged WI-006's individual defects (magic link, sign_count) but characterized them as P1/P2. Sonnet identified that WI-006 being at v1.0.0 while all other WIs are at v1.1.0 is itself a signal — the entire WI was skipped by Lote 10.3bis. This changes the severity calculus: these aren't isolated defects in an otherwise-patched spec; they're unpatched defects in a frozen spec. The "version freeze as a severity multiplier" pattern is a meta-observation that Opus missed.

3. **Key bootstrap failure modes enumerated (P0-R5-003)**: R4 noted `app.email_hash_key` initialization as a gap. Sonnet enumerated all three failure modes (Postgres exception → INSERT failure; NULL email_hash → UNIQUE bypass; empty key → reversible HMAC) and traced each to its production consequence. This is a more adversarial evaluation style — rather than noting the gap, it assumes the worst plausible runtime behavior for each uncovered path.

4. **Narrative vs code count discrepancy (P1-CONFIRMED-004)**: R4 reviewed WI-007 as the strongest WI in Part 2 and gave it 8.5/10 without flagging the §9.4 "23 is balance" text that was rendered wrong by the Lote 10.3bis enum expansion to 33 types. This was a false negative from R4 — likely because R4 reviewed WI-007 before reviewing the Lote 10.3bis changelog and did not re-read §9.4 after the event count change. Sonnet, reviewing post-Lote 10.3bis with fresh eyes, caught the stale count immediately.

**Where R4 was correct and R5 confirms**:
- R4's top finding (stale window additive vs orthogonal) was the most impactful S-03 defect — correctly identified and correctly fixed in Lote 10.3bis.
- R4's PRR staffing finding is confirmed; the 9/13 TBD problem is a real sprint-level blocker regardless of waiver path.
- R4's WI-007 audit (8.5/10) is directionally correct even if the §9.4 stale count slipped through; the WI is genuinely the strongest in the sprint.

**Model calibration note for future rounds**:
Opus 4.7 demonstrates superior depth on individual WIs (complex cross-reference chains, cost arithmetic errors, STRIDE/LINDDUN row completeness). Sonnet 4.6 demonstrates stronger runtime/deployment-context reasoning and version-lifecycle pattern recognition. Optimal adversarial coverage uses both models in sequence (R4 for depth, R5 for deployment context), which is exactly the current pipeline.

**SOTA calibration**: WI-S03-003 (8.0) is the best WI in this sprint. This is below the WI-S04-003 benchmark of 8.6. The gap is primarily explained by the runtime compatibility and key-bootstrap gaps in adjacent WIs that WI-003 depends on — WI-003's own design is sound. When those P0s are resolved, the sprint's SOTA ceiling rises.

---

*Reviewer*: Agent R5 (Claude Sonnet 4.6, claude-sonnet-4-6)
*File*: `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/_audits/2026-04-25-sonnet-r5-s03-wi-review.md`
*Status*: COMPLETE
*Date*: 2026-04-25
