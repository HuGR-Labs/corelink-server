# Agent R4 — Lote 10.3 S-03 Part 2 (WIs 005-008) WI Review

**Reviewer**: Agent R4 (Claude Opus 4.7, 1M context, independent review)
**Date**: 2026-04-25
**Scope**: WI-S03-005 (Neon schema + pgcrypto + RLS), WI-S03-006 (WebAuthn L3), WI-S03-007 (Audit EVT-047), WI-S03-008 (PRR ship gate)
**Source files**:
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S03/work_items/WI-S03-005-neon-schema-auth-tables.md`
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S03/work_items/WI-S03-006-webauthn-level3-admin.md`
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S03/work_items/WI-S03-007-audit-events-evt047-chain.md`
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S03/work_items/WI-S03-008-property-pentest-prr-ship-gate.md`
**Cross-references**: `_spec_contract.md` v1.1.0, `sprint.md`, S-03 Part 1 review (2026-04-25-agent-r4-s03-part1-wi-review.md), S-01/S-02 R4 reviews.

---

## Veredito Geral

The four WIs are dense, comprehensive, and broadly faithful to the sprint contract; they reach the §22-§32 density bar set in S-01/S-02 reviews and incorporate the lessons (13-row sign-off, ADR whitelisting, INV declarations, retention/DSR alignment). The package is **substantively GO-WITH-FIXES**: ambition is correctly calibrated and most adversarial surfaces are surfaced, but four classes of defects recur — (a) **technical inaccuracies in the SQL/cryptographic layer** (pgcrypto extension semantics, `pg_strom` misattribution, `gen_random_uuid` is v4 not v7, `SET LOCAL` lifecycle vs. CF Worker connection pooling, deterministic-JSON canonicalization is not native to `serde_json`), (b) **operational/staffing realism gaps** (13 sign-off table presupposes Tier-1 reviewers that current memory/state shows are still `_TBD` or "staffing-blocked"; 1-week internal pentest is undersized for 7-WI surface; external advisor 12-week lead time conflicts with sprint timeline), (c) **scale/cost arithmetic that under-counts production reality** (audit chain 700 GB/yr is correct order-of-magnitude but storage cost claim "$0.10/yr" is ~3 orders of magnitude off; 100 GB Neon storage at $0.000164/GB-hour for 12 months ≠ $72/yr), and (d) **a few INV/Spec drift issues** (sprint contract §1 says "10k iter" property test and "10–12 sign-offs"; WI-008 unilaterally upgrades to "100k iter" and "13 sign-offs" without contract amendment). The schema and WebAuthn WIs in particular have multiple subtle Postgres-16/W3C-spec issues that a real DBA + Crypto SME review will surface; that review is currently `_TBD` so there's a real risk these defects ship into the implementation phase. The audit-events WI is the strongest of the four; the ship-gate WI is the most ambitious but also the most exposed to staffing risk. Net: ship blockers are addressable inside the buffer week; the work itself is high quality.

**Average score: 7.95/10** (8.0 / 7.5 / 8.5 / 7.8).

## Per-WI Findings

### WI-S03-005 (Neon schema + pgcrypto + RLS)

**Score: 8.0/10**

Strong relational design — 7 tables, 6 enums, 11 partial indexes, FK cascade reasoning, and dual-encryption strategy (pgcrypto column + HMAC-SHA256 deterministic email_hash) is correctly motivated. Narrative §2 exceeds 300 words and identifies 9 catastrophic bug classes including the often-missed `last_login_at` write hot-spot. STRIDE+LINDDUN tables are populated, not boilerplate. ADR-0031 is whitelisted. DSR cascade vs. audit-chain pseudonymization split is the right pattern.

**Critical gaps / technical inaccuracies**:

1. **`gen_random_uuid()` is UUID v4, not v7** (line 61, 87, 100, 113, 136, 154). The narrative §9.9 explicitly says "UUID v7 via app-side" and "v7 is canonical em CoreLink", but the SQL DDL uses `DEFAULT gen_random_uuid()` which `pgcrypto` ships as v4. Postgres 16 does **not** ship UUID v7 generation natively (it lands in PG 18). Either the comment is wrong or the DDL is wrong; this is a load-bearing inconsistency because v7 was the design rationale for choosing it (time-ordered, index locality). Fix: either drop the v7 claim, or remove the DEFAULT and require app-side v7 minting on every INSERT.

2. **`pg_strom` is not an HMAC extension** (line 251). `pg_strom` is a GPU-accelerated query executor (HeteroDB). HMAC-SHA256 in Postgres comes from `pgcrypto` itself (`hmac(data, key, 'sha256')`) or from `pg_hmac` if you want a simpler API. This appears to be a hallucination or copy-paste mistake.

3. **`SET LOCAL app.current_tenant` does not survive across statements unless inside a transaction** (line 276, 372, 393, 398). On Cloudflare Workers with sqlx, every checkout from the pool must re-issue `SET LOCAL` *inside* the transaction or use `SET` (session-level), or use a dedicated `set_config(..., is_local=true)` call. Connection reuse without rebinding = RLS bypass risk on the very first stale checkout. This is the highest-severity issue in the WI: the entire RLS Layer-2 defense rests on this binding being correct, and the WI says "app sets ... em conn pool checkout per request" without specifying the transaction wrapper. Fix: explicit `BEGIN; SET LOCAL ...; <queries>; COMMIT;` per request, OR a sqlx connection-pool `before_acquire` hook setting `set_config(...)` with explicit guard. Add an integration test that opens two pool connections concurrently with different tenant_ids and asserts no leak.

4. **`pgp_sym_encrypt(...)` returns `text`, not `bytea`** (line 254, 376). The DDL types `email BYTEA`, but `pgp_sym_encrypt` returns ASCII-armored text; the bytea variant is `pgp_sym_encrypt_bytea`. The Gherkin example mixes both styles. Pick one consistently. Also note that `pgp_sym_encrypt` is non-deterministic (random IV) so `email_hash` is structurally required for lookup — that part is correct.

5. **NOTIFY/LISTEN does not work across Neon's read replicas and is not durable** (line 300-305, §6.1.9). Postgres NOTIFY is connection-scoped and primary-only; if a Worker is reading from a replica or the LISTEN connection drops, every notification in the gap is lost (no replay). The "dual emit (NOTIFY OR direct outbox INSERT)" mitigation is hand-waved — pick one. The right answer is direct outbox INSERT in the same transaction (atomic, durable, at-least-once via drain worker), making NOTIFY redundant. Removing NOTIFY simplifies the architecture and removes a known-fragile primitive.

6. **CF Workers connection pool of 25 per Worker is unrealistic** (§6.1.11 line 313). Cloudflare Workers do not maintain persistent TCP connections across invocations the way a long-lived process does. Neon connection pooling for Workers is typically routed through Hyperdrive or PgBouncer; "max_connections: 25 per Worker" doesn't map onto Workers' execution model. This needs either a Hyperdrive note or a serverless-driver (`@neondatabase/serverless` over HTTP) note.

7. **Multi-key decrypt scheme has no key-id storage on `account.billing_email` or `webauthn_credentials.public_key`** (line 254-261). §6.1.2 says "schema has `<column>_key_id INTEGER` per encrypted column", but the actual DDL in §1 has zero `_key_id` columns. Either add them now or the rotation chaos test (10.5.6) will fail by construction.

8. **Cost arithmetic in §22 underestimates storage**. Storage growth 50 GB × $0.000164 × 8760h = $72/yr is mathematically wrong. $0.000164/GB-hour × 8760 h/yr = $1.44/GB-yr; 50 GB × $1.44 = $72/yr — actually that one checks out, sorry. But "Multi-region replicas (×4): $720/yr base × 4 = ~$2.9k/yr" is unsourced and the base is undefined. Re-derive.

9. **`api_tokens.scopes BIGINT` is signed 64-bit; `CHECK (scopes >= 0)` works but losing the high bit caps you at 63 scopes**. WI-S03-002 (Part 1) uses a u64 PatScopes bitset. The bitset packing may or may not respect the sign bit; document the bitlayout convention to avoid the day someone sets bit 63 and gets a CHECK violation.

10. **DSR cascade test (10.5.7) does not specify pseudonymization preservation criterion mechanically**. "audit chain pseudonymization preserved" is asserted as Gherkin step but the test would need a way to verify the pseudonymous-id space is intact post-erasure. Add: `assert audit_chain.principal_id_hash distinct count > 0 AND user_account.email NULL`.

**Strengths worth preserving**: 12-row risk register, RLS default-on as INV, additive-only migration policy with CI gate, separate HMAC key for email_hash (not pgcrypto master), schema_version table for region-drift detection, partial indexes with `WHERE alive`.

### WI-S03-006 (WebAuthn Level 3)

**Score: 7.5/10**

Implementation strategy is sound: pin `webauthn-rs 0.5` (audited W3C-L3), correctly identify RP ID = eTLD+1, mandatory UV in admin step-up, attestation required on registration, AAGUID allowlist + denylist, sign_count regression as SEV-1, cross-browser CI matrix (16 scenarios). Adversarial test list (origin spoof, RP confusion, alg=none, attestation chain, AAGUID denylist, UV bypass) covers the primary attack classes from W3C §13. Gherkin includes the often-missed `backup_eligible/backup_state` passkey sync scenario.

**Critical gaps / technical inaccuracies**:

1. **`webauthn-rs 0.5` does not yet exist as of cutoff knowledge** — current is `0.4.x` series with `0.5` work-in-progress. Pin to actual published version; if 0.5 is the target, document the release date or fallback. This is a small thing but property tests + cargo-audit assume the dep is actually available.

2. **AAGUID allowlist policy contradicts FIDO MDS dynamics** (§9.5). Saying "allowlist em config = known-good explicit" is fine for high-security tier, but enterprise WebAuthn deployments typically use FIDO MDS authenticator status (`UPDATE_AVAILABLE`, `USER_VERIFICATION_BYPASS`, `REVOKED`) as the policy filter, not a static allowlist. Static allowlist will go stale within months of Yubico/Apple/Google releasing new authenticators — and §28 R-003 lists "AAGUID allowlist outdated (new authenticators)" as MEDIUM impact, but the actual customer impact is HIGH (users with new authenticators get rejected). Either: (a) flip to MDS-driven policy with explicit denylist for known-bad authenticators, or (b) commit to a shorter-than-quarterly review cadence. The current "Quarterly review + customer feedback channel" mitigation is a UX trap.

3. **Step-up token TTL 5min lacks a justification** beyond "5min TTL bounded". Sprint contract §1.6 / CTRL-AUTH-010 says "MFA freshness 30 min". WI-S03-006 §1 picks 5min, §6.1.4 confirms 5min, but never reconciles with the 30min global window. Pick a coherent story: is 5min the per-op bind window, while 30min is the broader session freshness? Document explicitly. Also: 5min is short enough that an admin doing chained ops (revoke 100 PATs sequentially with manual review) could time out mid-flight. Test for that.

4. **Cross-browser CI matrix is hand-wavy on WebAuthn-in-CI**. §6.1.6 says "Playwright CI" but Playwright's WebAuthn support requires `--enable-features=WebAuthnVirtualAuthenticatorAPI` (Chrome) and equivalent flags per browser. Safari WebKit in CI generally cannot run WebAuthn ceremonies (no virtual authenticator API in WebKit headless). The "Safari WebKit" cell in the matrix is likely unrunnable in CI without a real macOS VM. §6.2 acknowledges "no native CTAP testing in CI" but doesn't acknowledge Safari/WebKit is similarly limited. Real cross-browser coverage requires BrowserStack or SauceLabs with real OS VMs — that's a $/yr line item that's not in §22 cost analysis.

5. **`origin allowlist exact match` claim is incomplete**. WebAuthn origin matching has a specific algorithm in W3C §13.4.9: for `Top.origin` matching it's the *full* origin (scheme + host + port). The WI says "no prefix bypass" but doesn't specify port handling, scheme handling (ws://?), or `subdomain.app.corelink.humangr.com` rejection. Concrete test: assert `https://app.corelink.humangr.com:443` and `https://app.corelink.humangr.com` both allowed if both intended; assert `https://app.corelink.humangr.com.evil.com` rejected by exact match.

6. **iCloud Keychain passkey sync edge cases under-documented**. The `backup_eligible/backup_state` Gherkin (line 413) only covers happy path. Real edge cases: (a) user enrolls on iPhone, iCloud Keychain sync delayed 24h, user authenticates from iPad with `backup_state=false` — should this be accepted or rejected? (b) Account-level iCloud compromise → restored passkey from attacker's device. The WI says "out-of-scope WebAuthn (account-level security)" but the `auth.webauthn.new_device_used` event isn't actually defined in WI-007's 23 event types. Cross-WI inconsistency.

7. **Sign_count regression false-positive rate**. The §28 R-004 mitigation "threshold > 3 events em 24h" is reasonable for noisy authenticators, but contradicts §9.9 "high signal-to-noise; warrants SEV-1 alert". With threshold > 3 you no longer SEV-1 on first regression, you SEV-2 with a counter. Pick one. Note also that some passkey implementations correctly leave sign_count = 0 always (W3C allows this); the spec says "If [sign_count] is 0, the user has chosen not to use this counter." Your code must accept sign_count = 0 on every read and not treat 0 → 0 as a regression.

8. **"FIDO Alliance compliance test suite" (§10.5.10)** — the FIDO Alliance Functional Certification is a paid program, not a freely-runnable test suite. The "open-source" qualifier is incorrect. There is no open-source FIDO compliance suite that's authoritative; closest is `webauthn-rs` upstream test corpus + the W3C WPT WebAuthn tests. Fix the claim.

9. **Recovery flow has a circular dependency**. §6.1.10: "If all credentials lost: account recovery via Clerk SSO email magic link → register new authenticator". But sprint contract §10 explicitly anti-scopes "Magic link auth (rejected; phishing-prone)". Either Clerk's recovery flow doesn't use a magic link (it's an email + verification code flow) — in which case rephrase — or you've imported the anti-scope item back through the recovery door. This is exactly the kind of policy-vs-implementation drift the AppSec review will catch.

10. **AAGUID privacy tradeoff under-discussed**. §9.7 mentions encrypted public keys but doesn't address that AAGUID itself is a privacy signal (links a user to a specific authenticator model). Pure passkey ecosystem debates whether to expose AAGUID at all. Worth a sentence in §26 LINDDUN Identifiability row.

**Strengths**: error enum is concrete and exhaustive, `cargo-fuzz` on COSE/CBOR, AAGUID denylist as adversarial scenario, dedicated step-up middleware (vs. session cookie alone), explicit "≥ 3 credentials per user" recovery posture.

### WI-S03-007 (Audit events EVT-047)

**Score: 8.5/10**

Strongest of the four. CloudEvents 1.0 envelope is correctly specified (mandatory + extension fields), 23 event types are exhaustive across token/session/tenant/membership/WebAuthn/admin/anomaly axes, redaction macro architecture is sensible (compile-time enforcement via macro expansion + CI lint, not runtime check). Per-tenant retention hint as event field is the right pattern (lets S-11 worker be tenant-tier-aware without re-querying). Outbox-atomic-with-handler invariant is correctly placed as CRITICAL. Dual fan-out for SEV-1 anomaly events trades cost for incident response — well argued.

**Critical gaps / technical inaccuracies**:

1. **"Deterministic JSON canonicalization" via `serde_json` is not natively supported** (§6.1.5, §9.9). `serde_json` does not guarantee key ordering; it preserves struct field order at compile time but maps (HashMap/BTreeMap) and skipped-fields behave differently. The WI mentions "`canonical_json` crate" as alternative but that crate (RFC 8785 / JCS — JSON Canonicalization Scheme) has known issues with floating-point and Unicode normalization. Concrete recommendation: use `serde_jcs` (audited RFC 8785) and pin a specific Unicode normalization form (NFC); add a property test that round-trips known JCS test vectors. Currently §10.5.3 only tests "serialize twice → byte-equal" which doesn't prove canonical form, only deterministic-on-this-codebase.

2. **`#[non_exhaustive]` in Rust applies to *external* consumers, not internal**. §9.4 + §6.1.3 + §23: marking `AuthEventType` as `#[non_exhaustive]` lets you add variants without breaking *external crate consumers*' exhaustive matches, but inside the workspace (where S-09 chain processor lives) every match still needs a wildcard. Document that. Also, `#[non_exhaustive]` does **not** prevent serde from breaking when an unknown serialized variant arrives — that's a separate concern (`#[serde(other)]` or fallback variant).

3. **Chain hash semantics ambiguous re: `prev_hash` field inclusion**. §6.1.5: "`content_hash = sha256(serialized_event_without_chain_fields)`. `prev_hash` provided by chain consumer (S-09); este WI sets `None` em emit." But what does the consumer hash to produce the chain link — `sha256(content_hash || prev_hash)`, or `sha256(serialized_event_with_prev_hash_set)`? Without this spec'd here, S-09 will pick one and lock it in. Concrete: write the formula explicitly. Recommended: chain link = `sha256(prev_chain_hash || content_hash)`, never re-serialize the event for chain purposes (avoids the canonicalization problem twice).

4. **`principal_id_hash` truncated to 64 bits → birthday collision at ~4B distinct principals**. §9.3 says "collision birthday-bound 2^32 (4B); CoreLink scale (M users) negligible collision probability". Math: SHA-256 truncated to 64 bits has collisions at 2^32 ≈ 4B (birthday). For *distinct user space*, fine. But correlation across events for the *same* principal works fine; the concern is *cross-tenant* hash collision in audit forensics — distinct users in different tenants colliding to same hash → forensic confusion. At M users this is ~negligible but document explicitly: "P(collision per-event-pair) = 2^-64 ≈ 5×10^-20; P(any collision in 10^9 events) ≈ 3×10^-2". Or — the simpler fix — truncate to 96 or 128 bits (24 or 32 hex chars, 2x or 4x the storage), still pseudonymous, collision-resistant up to 2^48 / 2^64 events.

5. **23 event types is good but missing some**. Cross-WI check: WI-S03-006 references `auth.webauthn.new_device_used` (line 158) but this is **not** in the 23-type list. Also missing: `auth.account.deleted` (DSR erasure trigger event — needed for compliance trail), `auth.tenant.deleted` (same), `auth.pat.scope_escalated` (would catch the bug class in spec contract §10.s03.x). Add at minimum these 3-4. Update §6.1.3 to specify that the registry doc enumerates all 23+ types canonically.

6. **`auth.denied.invalid` event taxonomy granularity**. Currently one bucket. Pentester / SIEM analyst will want to distinguish: `denied.signature_invalid`, `denied.expired`, `denied.scope_insufficient`, `denied.not_found`, `denied.malformed`, `denied.revoked`. Each is a different forensic signal. Suggest splitting `auth.denied.invalid` into 5-6 sub-types.

7. **Volume math under-counts and over-promises** (§6.1.7 → §22). 10M req/dia × 1 audit event/req = 10M events/dia; at 200 B/event = 2 GB/dia = 730 GB/yr — correct. But "Storage cost (Neon retention 90d for team-tier average): ~50 GB × 12 × $0.000164 = ~$0.10/yr (trivial)" is wrong by ~3 orders of magnitude. 50 GB × 90 days × 24 hours × $0.000164/GB-hour = $17.71 / 90-day-window, repeated ~4x/yr = ~$71/yr — and that's per-tenant, per-tier. At 1000 tenants the storage line is $70k/yr, not "$0.10/yr". This entirely breaks the cost gate at §10.5.10 / §14.5.10. Re-derive from scratch.

8. **PII redaction CI lint is described as a Rust binary at `tools/audit_pii_lint/`** but Rust pattern-matching on source files for `format!("{}", principal_id)` patterns is hard (false positives on every variable named `principal_id_hash`). Real solution: a clippy lint plugin OR a `cargo-deny`-style tree-walking AST visitor. Document the implementation strategy or this becomes 2 weeks of "a Rust binary" yak-shaving. Alternative cheaper approach: `#[cfg(feature = "audit-strict")]` + a wrapper newtype `RedactedPrincipal` that derives `Debug` to print `[REDACTED]`, force all audit emit paths to take `RedactedPrincipal` not `String`. Compile-time enforcement, no lint needed.

9. **`auth.anomaly.cross_region_burst` detection logic in S-09 not S-03 — but emit event type defined here**. §6.1.9: "These detections em downstream (S-09 chain processor); este WI provê event types." Concern: how does this WI test that the type *is consumed correctly* by S-09 if S-09 doesn't exist yet? The §10.5.5 acceptance ("synthetic burst → emit fires") implies a stub S-09 detector. Make that stub an explicit deliverable here, or the test is unsetable.

10. **Outbox atomic-with-handler vs. CF Workers D1 batch limit**. D1 batch is the atomicity primitive but has a 100KB transaction size limit. A single mass-revoke could emit 10k audit events ~= 2 MB → exceeds D1 batch. Either chunk emit (loses atomicity per-event) or stream to outbox in a separate non-atomic write (loses the very INV §12.2 INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). Document the chunking semantics or shift to "exactly-once via idempotency key + retry" which is weaker but bounded.

**Strengths**: dual fan-out for SEV-1 only (cost-controlled), `#[non_exhaustive]` + 1yr deprecation policy, integration emit hooks listed per WI consumer, redaction macro at compile-time, anomaly events as first-class types not ad-hoc strings.

### WI-S03-008 (PRR ship gate)

**Score: 7.8/10**

Most ambitious of the four. Property test scope (4 properties, 100k iter, ≤10 min CI nightly) and pentest scope (1 week internal + tools list + tooling) are concrete. DSR PAT export integration test is well-specified (export response shape + erasure cascade + 30-day SLA). RB-FM-160 dry-run as Rust binary (not bash) reuses S-02 pattern. PRR-S03.md spec includes APPROVED/CONDITIONALLY_APPROVED/REJECTED gate with waiver expiry + ADR + review cadence — operationally mature.

**Critical gaps / technical inaccuracies**:

1. **Sprint contract drift: "100k iter" vs. "10k iter"**. Sprint contract §5 S03-D8 says "10k iter revocation race"; spec contract §6 / §7 mostly say "10k iter" then completeness §10.s03.2 says "property test 100k". Actual WI §1 + §6.1 commits to 100k. There's a real ambiguity in the contract. WI-008 should note this is a *deliberate raise* and which line of the contract supersedes. Right now the contract reads inconsistent with itself, and if a reviewer cites §5 they're correct.

2. **Sprint contract drift: "10–12 sign-offs" vs "13"**. Sprint contract §14 reads "12 mandatory + 1 optional Crypto SME = 13". Spec contract §6 reads "10–12 sign-offs". WI-008 §9.4 says "13 sign-offs mandatory; sprint contract S-03 §14 = 13". This is consistent with sprint.md but inconsistent with `_spec_contract.md` §6. Pick one canonically; amend the conflicting doc.

3. **100k iter × 50 tenants × 1000 ops in ≤10 min CI is optimistic**. Math: 100,000 iterations × ~6ms/iter (claimed) = 600s = 10 min, with zero overhead. Reality of property-based testing with `proptest` on a multi-region simulator (50 tenants, 5 regions, mixed ops including Argon2id verify which is calibrated to ~250ms server-side per WI-002) — 100k iter with even 1% Argon2 verify = 250 seconds of pure CPU just on hashing. The "≤10 min" target is unachievable as specified. Either:
   - Drop Argon2id verify out of the hot iter loop and into a sampled subset (~1% of iters).
   - Bump time budget to 30 min nightly (still acceptable).
   - Reduce to 10k iter for the full-stack property and run 100k for the simpler properties (5-layer defense, JWT alg=none).
   The current spec will fail on its own DoD timing on day 1.

4. **1-week internal pentest is undersized for 7-WI surface**. Pentester team = Security Lead + AppSec + 1 external advisor; 1 week = ~120 person-hours. Scope per §6.1.2: Clerk SSO + PAT (Argon2 timing oracle, scope spoofing, prefix manipulation, format injection) + Tower middleware (5-layer ordering, scope bypass, TenantCtx confusion) + Revocation (replay, mass abuse, queue poisoning) + WebAuthn (origin spoof, RP confusion, replay, attestation forge, AAGUID denylist) + Audit chain (tamper, insertion, gap, PII leak). That's ~30+ attack classes across 7 surfaces = ~4 hrs/class average. Realistic minimum for credible coverage is 2 weeks; industry norm for HIGH_RISK auth is 3-4 weeks with 2-3 pentesters. The 1-week box will produce a thin report. Either expand to 2 weeks, narrow scope to top-12 attack classes, or explicitly mark the report as "pre-GA scoping pentest, full external pentest defer to S-20".

5. **External advisor 12-week lead time conflicts with sprint timeline**. §9.3 says "external pentest cycle 12+ weeks lead time + ~$50-100k cost. S-03 internal: faster (1 week)." The advisor in §6.1.2 is described as "1 external advisor (rotated)" — rotated where, on what cycle, with what lead time? Sprint S-03 is a 4-week sprint + 7-day buffer; if external advisor wasn't booked at sprint kickoff (and there's no evidence in the spec contract that they were), they won't be available until S-04 or S-05 at earliest. Acknowledge this risk in §28; current §28 R-008 ("External pentester quality variance") doesn't capture the *availability* risk.

6. **PRR sign-off staffing reality**. Sign-off table §30 has 8 rows marked `_TBD_` and 1 row "_staffing-blocked_" (SRE Lead). Sprint memory and `_spec_contract.md` confirm no SRE Lead is currently retained. With 13 mandatory sign-offs and 9 unfilled roles, the gate is *structurally unmovable* — can't APPROVE without 13 signatures, can't get 13 signatures without 9 hires/contractors. §28 R-003 mentions "PRR sign-off staffing gap" but only with mitigation "2-week notice; alternate reviewers documented" — alternates aren't documented, this is a paper mitigation. Need: explicit "as of 2026-04-25, the following Tier-1 reviewers are confirmed: [list]" + "the following are pending: [list]" + escalation plan if any role is unfilled at PRR time. Without this, PRR gate cannot SEAL the sprint as designed.

7. **DSR 30-day SLA test is structural, not behavioral**. §6.1.3 Scenario C says "verify completion ≤ 30 days (S-11 worker timing assumed)". The actual S-11 worker doesn't exist yet, so the test only verifies *trigger* correctness, not *completion timing*. Reframe: "verify trigger emits within 1s; full 30-day SLA test deferred to S-11 ship gate."

8. **TLA+ specs (4) re-validation is not "free"**. §6.1.6 says "All 4 TLA+ specs re-validated com S-03 changes em test infrastructure." But which TLA+ specs are these? S-01/S-02 reviews mentioned `tenant_isolation.tla`. The "4 specs" claim is novel here and not corroborated upstream — what are `cas_integrity`, `gc_correctness`, `audit_immutability`? If they don't exist yet, the §11 DoD "All TLA+ specs validated" is unsatisfiable. List the 4 spec paths concretely, or drop to 1.

9. **Cost analysis is light**. §22: "Pentest engagement: internal (Security Lead + AppSec time) + external advisor ~$5k per engagement × 2/yr = $10k/yr." But §9.3 says external pentest is "$50-100k cost." A $5k advisor isn't a pentest, it's a review. This is the "external defer S-20 GA hardening" pattern, but the cost line in §22 contradicts §9.3. Reconcile.

10. **"Production parity validation em staging" (chaos exp 10) lacks acceptance criteria**. Real Clerk + Neon + R2 in staging is great, but what *passes* the chaos? Add specific SLOs sustained for N hours, error budget consumption thresholds, regression bench against PR baseline.

11. **Pentest waiver pathway has no expiry SLA**. §9.7 CONDITIONALLY_APPROVED: "Review cadence: 1 sprint." 1 sprint is 4 weeks; that's reasonable for low-severity P2 carry-over but for P1 waivers you want explicit expiry within sprint+1, with auto-rollback if not addressed. Make explicit: P1 waiver TTL 1 sprint + auto-escalate to REJECTED at expiry.

12. **`prop_argon2_calibration_stable` is unrunnable in CI**. §6.1.1: "1000 synthetic deploy environments (varying CPU)". CI runners do not provide varying CPU; you'd need a multi-platform matrix or a synthetic CPU-throttle harness. The actual property here is "calibration ∈ [200ms, 350ms] on a single CI runner over 1000 trials" — which tests calibration *stability under load*, not *across hardware*. Rephrase honestly.

**Strengths**: PRR gate semantics (APPROVED / CONDITIONALLY_APPROVED / REJECTED) with waivers + ADR + expiry, RB-FM-160 automation as Rust binary, internal pentest with explicit tools list (Burp + ZAP + Caido + libfido2 + cargo-fuzz), DSR test scoped to integration trigger correctness, adversarial test summary aggregation as deliverable.

## Cross-WI Consistency

**Dependency graph**:
- WI-006 hard-blocks on WI-005 (`webauthn_credentials` table) — declared correctly in §18.
- WI-006 hard-blocks on WI-003 (Tower middleware) for step-up integration — declared.
- WI-007 references emit hooks in WI-001..006 — declared.
- WI-008 hard-blocks on all 7 prior WIs SEALED — declared.
- WI-005 → WI-007 (tenant.tier lookup for retention_hint) — declared in WI-007 §18, **not** in WI-005 §18 outbound. Add to WI-005 §18.

**ADR whitelisting**:
- ADR-0031 (WI-005), ADR-0032 (WI-006), ADR-0033 (WI-007) all whitelisted in `scripts/validate_references.py:213-215`. Verified.
- ADR-0030 (WI-S03-004 in Part 1) also whitelisted at `scripts/validate_references.py:212`.
- WI-008 declines an ADR (§9.10 "No novel architecture decision") — defensible; no whitelist gap.

**Sign-off harmonization**:
- All 4 WIs use 13-row table format consistent with S-01/S-02 reviewer feedback.
- BUT: roles between WIs slightly diverge in *emphasis annotations*. WI-005 emphasizes Privacy + Architect; WI-006 emphasizes AppSec + Crypto SME; WI-007 emphasizes Compliance + Privacy; WI-008 emphasizes Security Lead + Compliance + Privacy + AppSec + Crypto SME. This is *appropriate* per-WI specialization, not drift. Good pattern.
- 9 of 13 rows are `_TBD_` across all WIs; SRE Lead specifically `_staffing-blocked_`. This is a sprint-level staffing risk, not a per-WI defect (raised in WI-008 §6).

**Forward INVs declared**:
- WI-005: 5 INVs (RLS-DEFAULT-ON, PII-ENCRYPTED, MIGRATION-ADDITIVE, CASCADE-DSR-COMPLETE, AUDIT-PSEUDONYMIZATION). Good.
- WI-006: 5 INVs (UV-REQUIRED-ADMIN, ATTESTATION-VERIFIED, SIGN-COUNT-MONOTONIC, ORIGIN-EXACT, RP-ID-CANONICAL). Good.
- WI-007: 5 INVs (NO-RAW-PII, EMIT-ATOMIC-WITH-HANDLER, CHAIN-HASH-DETERMINISTIC, EVENT-TYPE-EXHAUSTIVE, RETENTION-HINT-ACCURATE). Good.
- WI-008: aggregates existing INVs + INV-AUTH-REVOCATION-SLO-60S. Good.

**TLA+ spec references**: WI-005 mentions "data_integrity.tla", WI-006 mentions "webauthn_ceremony.tla", WI-007 mentions "audit_chain.tla", WI-008 mentions "auth_revocation.tla" + 4 existing specs. None of these specs exist yet (per S-01/S-02 audit). The "planned forward" hedging is acceptable but the WI-008 §6.1.6 / §11 DoD assertion that "all 4 TLA+ specs validated" is overconfident — see WI-008 gap #8.

**Consistent unmet expectations**: every WI assumes integration tests can hit "real Clerk staging" and "real Neon staging" — the actual staging tenancy / credentials / setup is not declared anywhere as an existing artifact. Either it exists and isn't documented here, or §6.1.X integration tests are not runnable until staging-setup tasks (zero of which appear in any WI's sub-task list) are complete. This is a sprint-level dependency not surfaced.

## Technical Accuracy Issues

Aggregated and ranked by severity:

**P0 (must fix before SEAL)**:
- **WI-005 #1**: `gen_random_uuid()` is v4, not v7. Either remove the v7 claim or remove the DEFAULT.
- **WI-005 #2**: `pg_strom` is not an HMAC extension — fix to `pgcrypto`'s native `hmac()`.
- **WI-005 #3**: `SET LOCAL` lifecycle vs. CF Workers connection pool is unspecified; RLS Layer-2 defense could be bypassed on first stale connection checkout. Specify transaction-wrapped binding + integration test.
- **WI-005 #7**: Multi-key decrypt requires `_key_id` columns on encrypted columns; current DDL has none.
- **WI-006 #6 / WI-007 cross-ref**: `auth.webauthn.new_device_used` referenced in WI-006 but not in WI-007's 23-type taxonomy.
- **WI-007 #1**: `serde_json` is not deterministic for maps; use `serde_jcs` (RFC 8785) or document canonicalization explicitly.
- **WI-007 #7**: Cost arithmetic on storage off by 3 orders of magnitude (storage isn't $0.10/yr; ~$70-70k/yr depending on tenant scale).
- **WI-008 #1, #2**: Sprint contract drift on iteration count (10k vs 100k) and sign-off count (10–12 vs 13). Reconcile docs.

**P1 (should fix this sprint)**:
- **WI-005 #4**: `pgp_sym_encrypt` returns text not bytea; use `pgp_sym_encrypt_bytea`.
- **WI-005 #5**: NOTIFY/LISTEN is unreliable; drop in favor of direct outbox INSERT.
- **WI-005 #6**: CF Workers pool of 25 needs Hyperdrive or serverless-driver context.
- **WI-005 #9**: BIGINT scope bitset caps at 63 bits with CHECK; document or use NUMERIC.
- **WI-006 #1**: `webauthn-rs 0.5` may not exist; pin actual version.
- **WI-006 #2**: AAGUID allowlist policy will go stale; flip to MDS-driven or commit to monthly review.
- **WI-006 #3**: Step-up TTL 5min vs. global 30min freshness — reconcile.
- **WI-006 #4**: Cross-browser CI matrix can't actually run Safari/WebKit in pure Playwright; need real macOS VM (BrowserStack/SauceLabs) — cost line item missing.
- **WI-006 #7**: sign_count regression policy (SEV-1 first vs. threshold > 3) — pick one.
- **WI-006 #9**: Recovery flow imports magic link anti-scope; rephrase as "email + verification code".
- **WI-007 #3**: chain hash formula not specified between WIs; lock to `sha256(prev_chain_hash || content_hash)`.
- **WI-007 #4**: `principal_id_hash` 64-bit truncation: document collision space or extend to 96-128 bits.
- **WI-007 #6**: split `auth.denied.invalid` into 5-6 sub-types for forensic granularity.
- **WI-007 #8**: PII redaction CI lint as Rust binary is yak-shave; use newtype `RedactedPrincipal` instead.
- **WI-007 #10**: D1 batch 100KB limit conflicts with 10k-event mass-revoke; spec chunking semantics.
- **WI-008 #3**: 100k iter × Argon2 verify won't fit in 10 min CI; reduce iter count or sample.
- **WI-008 #4**: 1-week internal pentest undersized for 7-WI surface; expand or scope-down.
- **WI-008 #5, #9**: External advisor lead time / cost reconcile.
- **WI-008 #6**: PRR staffing — 9 of 13 reviewers `_TBD_`; hard blocker for SEAL.
- **WI-008 #8**: TLA+ "4 specs" — list paths concretely or drop.

**P2 (next sprint or doc-only)**:
- **WI-005 #8, #10**: Cost arithmetic check; DSR test pseudonymization assertion mechanics.
- **WI-006 #5, #8, #10**: Origin matching algorithm specifics; FIDO compliance suite mischaracterization; AAGUID privacy LINDDUN row.
- **WI-007 #2, #5, #9**: `#[non_exhaustive]` semantics doc; missing event types (`auth.account.deleted`, `auth.tenant.deleted`, `auth.pat.scope_escalated`); S-09 stub for anomaly emit test.
- **WI-008 #7, #10, #11, #12**: DSR 30-day SLA test scope; chaos exp 10 acceptance criteria; waiver expiry SLA; argon2_calibration_stable property naming.

## Missing Gaps for Production

1. **Schema migration zero-downtime cross-region orchestration**. Sprint contract §6 / sprint.md lists 5 regions (wnam, enam, weur, eeur, apac). Neon multi-region is described but the migration roll-out *order* and *backout strategy across regions* is undocumented. Real production: rolling migration over 5 regions with replication lag (Neon ≤100ms p99 promised, but bursty up to 1+ min observed) means migration #002 applied in wnam may not be visible in apac for minutes, during which the app expects either schema-old or schema-new — breaks. Add: explicit rollout order (canary region → 4-region wave → audit), schema_version compat matrix per region, `IF NOT EXISTS` is necessary but not sufficient.

2. **WebAuthn iCloud Keychain account-level compromise edge case**. WI-006 §6.2 and §1 acknowledge this is "out-of-scope WebAuthn (account-level security; user responsibility)". For enterprise tier this is unacceptable hand-off; SOC 2 auditor will ask "what's your compensating control for stolen passkey via cloud sync?". Need: (a) explicit `attesteddCredentialData.flags.BE/BS` policy for enterprise tier (reject cloud-synced credentials for admin role), (b) anomaly emit on new-device-from-passkey-sync first use, (c) optional enterprise tier toggle "require platform-bound credentials only".

3. **Audit chain bloat — long-term TCO**. 700 GB/yr at single-tenant scale; multi-tenant 10k tenants × varied retention = projected ~50-500 TB depending on tier mix. Current §22 cost analysis assumes single-workload. Need: per-tier-per-event-type sampling rates, cold storage tiering (R2 → S3 Glacier equivalent), explicit budget gate with alerting at 80%/100% thresholds. WI-007 §6.1.7 mentions "sampling for non-CRITICAL events (admin opt-in)" but no concrete sampling rates.

4. **PRR sign-off staffing — Tier-1 reviewer pipeline**. 13 sign-offs requires:
   - Owner (Gustavo) ✅
   - Final Approver (Gustavo) ✅ (same person, double-counted is OK per HIGH_RISK rules?)
   - SRE Lead — `_staffing-blocked_`
   - Security Lead — `_TBD_`
   - 2 Engineer peers — `_TBD_`
   - QA — `_TBD_`
   - Product (Gustavo) ✅
   - Compliance — `_TBD_`
   - Privacy — `_TBD_`
   - Architect — `_TBD_`
   - AppSec — `_TBD_`
   - Crypto SME — `_TBD_`
   That's 9 unfilled roles. At sprint kickoff D+0, identify 9 contractors/staff or PRR cannot SEAL. Memory note suggests "user_constraints" doc — does it specify hiring constraints? If sprint S-03 is intended to ship in 4 weeks + 7d buffer, recruitment of 9 Tier-1 reviewers in <5 weeks is infeasible. Either: (a) reduce mandatory sign-off count (amend sprint contract), (b) accept that S-03 SEAL is staffing-blocked and slip schedule, (c) fractional/part-time reviewers via consultancies (LOA + scoped engagements). Pick one and write to `_spec_contract.md`.

5. **Pentest external advisor 12-week lead time**. Trail of Bits / NCC Group / IOActive / Latacora typical lead time 12-16 weeks; budget $50-100k per engagement. WI-008 §9.3 acknowledges this and defers external pentest to S-20. But §6.1.2 still says "1 external advisor (rotated)" in the *S-03* pentest team. Reconcile: either S-03 has internal-only pentest (correct per §9.3) and §6.1.2 is wrong, or S-03 has both internal + 1 advisor and that advisor is *already booked* (need name/firm/dates).

6. **DSR SLA testing doesn't test SLA**. LGPD Art. 19: 15-day response window (extendable to 30) for confirmation, 30-day for execution. Sprint contract §10.s03.6 / WI-008 §6.1.3 Scenario C says "verify completion ≤ 30 days". But the test runs in CI in <1 min — it cannot test 30-day timing. Add: separate compliance audit (quarterly?) that pulls real DSR request → completion timing from production audit chain and asserts against SLA.

7. **NIST SP 800-63B AAL3 alignment incomplete**. WebAuthn L3 + UV is necessary but not sufficient for AAL3; AAL3 requires hardware-bound *multi-factor cryptographic authenticator* — passkeys synced via iCloud are AAL2 max (cloud sync is "soft" by NIST definition). Sprint contract §16 claims "NIST SP 800-63B AAL3"; in practice S-03 ships AAL2 + AAL3-compatible-when-using-platform-bound-only. Document this distinction. Affects compliance positioning.

8. **OWASP ASVS V2/V3/V4/V6/V8 self-checklist** (WI-008 §6.1.5 / §10.6.10). The actual checklist file is listed as deliverable but not pre-populated; with 100+ requirements across 5 categories, this is a real 6-8h effort. Effort budget includes 6h for this in §17 ST-017 — that's plausibly enough for self-assessment but not for documented evidence per requirement. SOC 2 auditor will ask for evidence per requirement; budget 16-20h.

9. **Runbook RB-FM-160 doesn't currently exist**. WI-008 §6.1.4 says "Runbook: `specs/05_runbooks/RB-FM-160.md` — Auth Invalid Storm (reuse de existing runbook)". S-02 R4 review noted RB-FM-253 was the analog. Confirm RB-FM-160.md exists *or* add a sub-task to author it (~4h). Currently §17 ST-011 is "RB-FM-160 dry-run automation script (Rust binary) | 6h" — that's the automation, not the runbook itself.

10. **Property test memory budget 256 MB** (WI-008 §14.6.9). 50 tenants × 5 regions × 1000 ops × per-op state overhead — reasonable in steady state, but proptest shrinking on a failure replay can balloon to GB. Document the shrinking strategy, OOM fallback, and CI runner sizing.

11. **Audit chain DSR pseudonymization is one-way — but reverse-lookup capability is needed for legitimate forensics**. WI-007 §3 Persona 2 + WI-005 §6.1.7 mention "privileged query via PII-access role logs reverse lookup". The mechanism for reverse-lookup is not specified — separate side-channel mapping table? K-anonymity lookup? This is the place where the hash-truncation issue (#4) bites: if you ever need to reverse-lookup a `principal_id_hash` to a real principal, you need a stored mapping (separate table, separate access control, separate audit). This stored mapping is itself the vulnerable PII concentration point. WI-005 doesn't include it; either declare out-of-scope (no reverse-lookup) or scope it explicitly.

12. **Cost regression gates are scattered and non-aggregating**. WI-005 §14.5.10, WI-006 §14.5.10, WI-007 §14.5.10, WI-008 §14.6.10 each define a per-WI cost gate. There's no aggregate "total auth domain ≤ $X/yr per Y req/dia" gate. Result: each WI passes its gate while the sum exceeds budget. Add aggregate gate.

## Comparison vs Sprint Contract S-03

Deliverables coverage:

| Sprint Contract | WI | Coverage |
|---|---|---|
| **S03-D5** Neon schema (`migrations/002_auth_tables.sql`) | WI-S03-005 | ✅ DDL specified, idempotent, RLS, pgcrypto, 7 tables |
| **S03-D6** WebAuthn flows (Level 3 + cross-browser) | WI-S03-006 | ⚠️ Cross-browser CI matrix incomplete (Safari/WebKit unrunnable in Playwright); commit to BrowserStack |
| **S03-D7** Audit events EVT-047 (4 event types, chain hash S-09) | WI-S03-007 | ✅ Goes beyond contract — 23 event types vs 4. Fine, but contract should be amended to reflect. |
| **S03-D8** Property + pentest + RB + PRR (10k iter, 11 sign-offs) | WI-S03-008 | ⚠️ Drift: 10k → 100k iter, 11 → 13 sign-offs. Reconcile. |

Sprint contract §1 + §10 + §13 alignment: largely good. Risks:
- Sprint contract §7 DoD says "PRR HIGH_RISK 10–12 sign-offs"; WI-008 says "13 mandatory". This is exactly the kind of 1-row drift that S-01/S-02 review flagged. Fix.
- Sprint contract §10.s03.5 "WebAuthn cross-browser tested Chrome/Firefox/Safari/Edge latest 2 versions": WI-006 claims this but unrunnable specifications.
- Sprint contract §15 R-008 "JWKS cache poisoning" — covered in WI-001 (Part 1), not WI-005-008 scope.
- Sprint contract §17 References — list is correct, ADRs whitelisted.
- Sprint contract §19 Waiver policy — WI-008 §9.7 implements the CONDITIONALLY_APPROVED gate but doesn't enumerate the §19 waivable items (MFA freshness 30→60min, Argon2 m_cost 65536→32768, revocation 60→90s). Add explicit cross-link.

Sprint contract Timeline §13: D+5 (WI-001 + WI-005 SEALED), D+18 (WI-006), D+22 (WI-007), D+26 (WI-008), D+27 review. WI-005 effort PERT 48h ≈ 6 work-days, fits D+5. WI-006 PERT 71h ≈ 9 work-days from D+5 → D+14, beats D+18 mark. WI-007 PERT 50h ≈ 6.25 work-days from D+14 → D+20, beats D+22. WI-008 PERT 80h ≈ 10 work-days from D+20 → D+30 — overshoots D+26 SEAL by 4 days *and* D+27 review by 3 days. Single-engineer plan is unachievable as scheduled. Either (a) parallelize WI-008 sub-tasks across pentester + owner (already noted, but pentester is 1-week not 10-day), (b) start WI-008 setup in D+20 alongside WI-007 final, (c) accept slip to D+30/D+31. Document.

Spec contract `_spec_contract.md` §6 Definition of Done: 13 checklist items. WI-008 covers 11 of them; missing explicit traceability to "10.s03.4 Argon2id params calibration" (only WI-002 / Part 1) and "OWASP ASVS V2/V3 100% checklist" (WI-008 §10.6.10 covers V2/V3/V4/V6/V8 — superset, fine).

## Recommendations

### P0 (block SEAL until fixed)

1. **Fix WI-005 SQL accuracy**: drop `gen_random_uuid()` DEFAULT (v4 not v7), drop `pg_strom` (not HMAC), specify `SET LOCAL` transaction wrapping for RLS, add `_key_id` columns to encrypted fields, switch to `pgp_sym_encrypt_bytea`, drop NOTIFY/LISTEN in favor of direct outbox INSERT, document Hyperdrive/serverless-driver for CF Workers.

2. **Reconcile sprint contract drift in WI-008**: 10k vs 100k iter, 10–12 vs 13 sign-offs. Pick canonical. Amend `_spec_contract.md` and/or `sprint.md` accordingly; do not let WIs unilaterally raise commitments.

3. **Address PRR staffing gap**: explicit retention plan for 9 unfilled Tier-1 reviewer roles before D+0 sprint kickoff, OR amend sign-off count, OR accept staffing-blocked SEAL slip.

4. **WebAuthn cross-WI consistency**: add `auth.webauthn.new_device_used` (and `auth.account.deleted`, `auth.tenant.deleted`) to WI-007's event taxonomy; reconcile WI-006 references.

5. **WI-007 deterministic JSON canonicalization**: switch from "deterministic JSON" hand-wave to `serde_jcs` (RFC 8785) or equivalent with explicit Unicode normalization. Add property test against JCS test vectors.

6. **WI-008 100k iter timing budget**: either reduce iter count (revert to 10k for full-stack), expand CI budget to 30 min nightly, or sample Argon2 verify in <1% of iters. Current spec is unachievable.

### P1 (fix this sprint, before PRR)

7. **WebAuthn AAGUID policy**: switch to FIDO MDS-driven status policy with denylist override; commit to monthly review (not quarterly).

8. **WebAuthn step-up TTL reconciliation**: document 5min as per-op binding window vs. 30min as session freshness; add test for chained admin ops crossing 5min boundary.

9. **Cross-browser CI realism**: commit BrowserStack/SauceLabs line item to §22 cost analysis (~$1.5-3k/yr) for real Safari/WebKit coverage; otherwise scope down to Chrome + Firefox + Edge in Playwright.

10. **WI-006 recovery flow**: rephrase "magic link" → "email + verification code" to avoid sprint contract §10 anti-scope import.

11. **WI-007 audit cost arithmetic redo**: storage at 50 GB × 90d × $0.000164/GB-h = ~$17.7/window; per-tier-per-tenant aggregation for realistic TCO; aggregate auth-domain cost gate.

12. **WI-007 chain hash formula**: lock to `chain_hash = sha256(prev_chain_hash || content_hash)`; document explicitly so S-09 doesn't pick something incompatible.

13. **WI-007 PII redaction approach**: replace "Rust binary lint" with `RedactedPrincipal` newtype + compile-time enforcement. Cheaper, more robust.

14. **WI-008 pentest scope**: expand to 2 weeks OR scope down to top-12 attack classes; clarify external advisor name/firm/dates or remove from §6.1.2.

15. **WI-008 TLA+ "4 specs"**: enumerate concrete spec paths or drop the count claim.

16. **DSR test improvement**: separate "trigger correctness" (CI test, today) from "30-day SLA evidence" (quarterly compliance audit, defer to S-11 ship gate).

### P2 (next sprint or doc-only)

17. WI-005 §22 cost arithmetic re-derive; add `principal_id_hash` collision analysis sentence to WI-007 §26 LINDDUN.

18. WI-006 origin matching algorithm spec (port, scheme, exact string compare); FIDO compliance "open-source" claim correction; iCloud Keychain enterprise toggle for AAL3.

19. WI-007 split `auth.denied.invalid` into 5-6 sub-types for forensic granularity.

20. WI-008 waiver expiry SLA (P1 waiver TTL 1 sprint + auto-rollback to REJECTED); RB-FM-160 runbook authorship sub-task if not already present.

21. Cross-WI: declare staging environment ownership and credentials as explicit sprint dependency (not a per-WI assumption).

## Final Verdict

**GO-WITH-FIXES** (average 7.95/10).

The four WIs hit the SOTA bar in volume, ambition, and cross-cutting integration; they reflect genuine engineering depth on RLS + pgcrypto + WebAuthn + CloudEvents + ship-gate semantics. The defects are real but tractable: ~6 P0 items (mostly SQL accuracy, sprint contract drift, and sign-off staffing) plus ~10 P1 items can all be remediated inside the 4-week sprint window if addressed at kickoff. The two highest-risk items are (1) the staffing gap on 9 Tier-1 reviewers (which is a sprint-level risk, not a WI-defect, and gates the whole SEAL), and (2) the implementation-time discovery of the `SET LOCAL` RLS binding bug, which would silently break tenant isolation in production. Both deserve explicit owner attention before D+0 of the sprint, not at PRR review. The audit-events WI is the strongest of the four; the schema WI has the most P0-class technical accuracy issues; the WebAuthn WI is good but optimistic about cross-browser CI feasibility; the ship-gate WI is the most ambitious and most exposed to staffing/timeline reality. Recommend Owner work through P0 list with an SRE/DBA pair-review (even if contract) in week 0 before kicking off implementation, and amend `_spec_contract.md` + `sprint.md` to reconcile the iter count and sign-off count drift.

---

**Reviewer**: Agent R4 (Claude Opus 4.7, 1M context)
**File**: `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/_audits/sealed/2026-04-25-agent-r4-s03-part2-wi-review.md`
**Status**: COMPLETE
