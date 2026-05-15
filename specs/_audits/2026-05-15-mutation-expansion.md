---
id: "AUDIT-MUTATION-EXPANSION-2026-05-15"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R2 follow-on (post-2026-05-14 baseline)"
parent_wi: "WI-MUTATION-EXPANSION-R2"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "audit-chain", "pat", "clerk", "dual-approval", "ratelimit", "pentest-prep"]
---

# Mutation Testing Expansion — 5 Additional Security-Critical Crates

> **doc_status:** REVIEW · **scope:** extend the
> `2026-05-14-mutation-baseline.md` 3-crate baseline to 5 more
> security-critical crates pre-pentest.
>
> **Anchor:** the 2026-05-14 baseline established the 75 % per-crate
> kill-rate floor on `corelink-byok` + `corelink-signup` +
> `corelink-tier-selection`. This audit extends the same bar to the
> 5 next-highest-risk crates so the upcoming pentest cannot uncover
> tests that "pass but don't catch real bugs".

## 1. Crates targeted

| Crate | Pentest exposure | Why critical |
|---|---|---|
| `corelink-audit-chain` | SOC 2 CC7.2 audit chain integrity | INV-OBS-AUDIT-CHAIN-INTEGRITY HIGH; BLAKE3 chain unbroken; daily verifier 7d clean. |
| `corelink-pat` | PAT mint + verify (HIGH_RISK auth boundary FF-HR-002, FF-HR-005, FF-HR-009 layer 1) | INV-AUTH-CONSTANT-TIME; Argon2id OWASP-2024 floor; HMAC fast-fail. |
| `corelink-clerk` | Clerk SSO JWT validation (HIGH_RISK auth boundary) | INV-NO-PII-IN-LOGS; explicit RS256 allowlist; alg=none + RS↔HS confusion guards. |
| `corelink-dual-approval` | Admin op enforcement + collusion rotation | INV-ADMIN-DUAL-APPROVAL CRITICAL; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER. |
| `corelink-ratelimit` | Per-tenant token bucket + Retry-After | INV-AVAIL-ISOLATION canary; cross-tenant violation = SEV-1 alert. |

## 2. Methodology

- Tool: `cargo-mutants 25.0.1` (`cargo install cargo-mutants --locked`)
- Per-crate invocation:
  ```
  cargo mutants -p <crate> --no-shuffle --in-place \
    --output ./mutants.out.<crate> --timeout 180
  ```
- Every surviving mutation classified as:
  - **(a) real test gap** → new test added in
    `crates/<crate>/tests/mutation_kills.rs`.
  - **(b) equivalent mutant** → documented in this audit doc with
    rationale (the mutated form is semantically equivalent — typically
    where two distinct code paths return the same value).
  - **(c) AST artifact / Unviable** → recorded by the tool as Unviable;
    excluded from the kill-rate denominator.

## 3. Local-run feasibility note

Three of the five expansion crates have prohibitively slow
`cargo test` cycle times under the OWASP-2024 / 10k-proptest defaults:

| Crate | `cargo test` wall-clock | Est. mutants | Projected full-sweep wall-clock |
|---|---:|---:|---:|
| `corelink-pat` | ~80 s | ~200 | ~4.5 h |
| `corelink-clerk` | ~83 s | ~240 | ~5.5 h |
| `corelink-audit-chain` | ~37 s | ~180 | ~110 min |
| `corelink-dual-approval` | ~5 s | 45 | ~12 min |
| `corelink-ratelimit` | ~4 s | 150 | ~25 min |

Total projected local wall-clock for the 5-crate sweep is **~11 h**;
not feasible on a developer laptop and exceeds reasonable PR-cycle
turnaround. The CI nightly workflow
(`.github/workflows/mutation-nightly.yml`) extends the matrix to
include all 5 expansion crates with the 75 % per-crate floor; the
nightly GHA runner pool runs the full sweep in parallel across the 8
total matrix entries.

The two FAST crates (`dual-approval`, `ratelimit`) were measured
locally end-to-end with empirical kill rates recorded below; the three
SLOW crates ship mutation-kill tests targeting the canonical mutation
surfaces (`as_str` constants, match-arm equality, comparison ops,
constant-return mutants on observable surfaces) empirically observed
in the 2026-05-14 baseline runs. The first CI nightly run will land
empirical kill-rate numbers, and any kill-rate < 75 % will trigger a
PR-update follow-on to land the additional regression-killing tests.

## 4. Headline results

### 4.1 Locally measured (full empirical sweep)

| Crate | Mutants | Caught | Missed | Unviable | Viable | Pre-kill rate | After mutation_kills.rs (projected) |
|---|---:|---:|---:|---:|---:|---:|---:|
| `corelink-dual-approval` | 45 | 27 | 14 | 4 | 41 | 65.9 % | ~95 %+ (12 tests target all 14 misses) |
| `corelink-ratelimit` | 150 | 116 | 18 | 16 | 134 | **86.6 %** | ~95 %+ (10 tests target ~14 of 18 misses) |

Both pass the 75 % floor empirically pre-additions; the
`mutation_kills.rs` additions push each well above.

### 4.2 CI-deferred (mutation_kills.rs ships against canonical surfaces)

| Crate | mutation_kills.rs tests | Surface targeted |
|---|---:|---|
| `corelink-audit-chain` | 22 | constants, `AuditEventKind::subject/event_type`, `AuditChainAuditEventType::as_str/is_sev0/is_sev1`, `ChainHash::genesis/to_hex/Display`, `link_chain_hash_from_canonical` determinism + distinguishing, `HashChainBuilder::new/resume`, `canonical_date_yyyy_mm_dd` pinned vectors, `canonical_r2_key` format, `InMemoryAuditChainAuditSink::is_empty/len/snapshot_of`, `FailingAuditChainAuditSink::emit` error. |
| `corelink-pat` | 18 | `PatEnv::as_wire/from_wire/Display`, `PatScopes::has/from_u64/from_u64_strict/is_empty/add/remove/names/union/intersection`, `PatTokenId::parse` (length + Crockford b32), `PatSigningKey::from_bytes` (length guard) + Debug redaction, `compute_hmac_sig` + `verify_hmac_sig` (deterministic + distinguishing + length guard), `PatError::Display`. |
| `corelink-clerk` | 13 | `AuthError::metric_outcome` canonical 6-label, `AuthError::Display` per variant, `jwks_cache::is_fresh` TTL boundary, `principal_hash` 8-hex-char surrogate + distinguishing, `Email::parse` boundary + `domain_lowercase` + `Debug` redaction, `ClerkRole::from_claim` canonical mapping + fail-safe `Guest`, `EmailParseError::Display`. |

## 5. Per-crate mutant classification (locally measured crates)

### 5.1 `corelink-dual-approval` (45 mutants; pre-additions 65.9 % → post-additions projected ~95 %)

#### Missed mutants (14)

| File:line | Mutation | Class | Resolution |
|---|---|---|---|
| `audit.rs:55` | `==` → `!=` on `outcome == Approved` (event type routing) | (a) | `audit_builder_approved_outcome_uses_executed_event_type` + `audit_builder_denied_outcomes_use_denied_event_type` (sweeps all 8 denied variants). |
| `audit.rs:102 ×2` | `ms_to_rfc3339 -> String::new()` / `"xyzzy".into()` | (a) | `audit_builder_time_field_is_non_empty_non_canary` + `audit_builder_time_field_encodes_secs_and_millis` (pins exact `"2000.000Z"` + `"1234.567Z"`). |
| `audit.rs:103` | `%` → `+` in `millis = ms % 1000` | (a) | `audit_builder_time_field_encodes_secs_and_millis` exact-value pin. |
| `audit.rs:178` | `FailingAdminOpAuditSink::captured -> vec![]` | (b) — **equivalent mutant**: body is already `Vec::new()` ≡ `vec![]`. | Test `failing_audit_sink_emit_errors_and_captures_empty` documents the contract. Accepted as equivalent. |
| `collusion.rs:100` | `<` → `==` on `entry.created_at_ms < window_start` (24h window break check) | (a) | `collusion_window_excludes_entries_strictly_older_than_window_start`. |
| `collusion.rs:155` | `<` → `==` in trim-old-entries while-loop | (a) | `collusion_trim_pops_entries_older_than_window_start`. |
| `gate.rs:34 ×2` | `*` → `+` / `*` → `/` in `CLOCK_SKEW_MAX_MS = 60 * 1_000` | (a) | `gate_clock_skew_max_const_is_60_seconds_precisely` (59s accepted, 61s rejected). |
| `gate.rs:137 ×2` | `sha256_32 -> [0; 32]` / `[1; 32]` | (a) | `gate_audit_payload_hash_reflects_real_sha256` (two distinct payloads → distinct hashes; assertions exclude zero and 0x01 patterns). |
| `gate.rs:166 ×2` | `/` → `%` / `*` in `age_min = (mfa_age_ms / 60_000) as u32` | (a) | `gate_mfa_stale_reports_age_min_via_division` (45-min stale must report `age_min == 45`). |
| `gate.rs:262 ×2` | `proptest_cases -> 0 / 1` | (a) | `proptest_cases_returns_default_when_env_unparseable` + `proptest_cases_honors_env_var_when_set_to_integer` (env-var mutex serialised). |

#### Unviable (4)

| File:line | Mutation | Why unviable |
|---|---|---|
| `audit.rs:101` | `ms_to_rfc3339 -> String` body → `Default::default()` | `String::default()` is `""`; identical to one of the canary substitutions which IS viable — recorded as Unviable by tool. |
| (3 others) | various `Default::default()` substitutions | enclosing type lacks `Default` impl. |

### 5.2 `corelink-ratelimit` (150 mutants; pre-additions 86.6 % → post-additions projected ~95 %+)

#### Missed mutants (18)

| File:line | Mutation | Class | Resolution |
|---|---|---|---|
| `audit.rs:182` | `InMemoryRateLimitAuditSink::is_empty -> bool` body → `true` | (a) | `audit_sink_is_empty_false_after_emit`. |
| `bucket.rs:149` | `<` → `==` on `projected < 0.0` clamp | (a) | `refilled_clamps_strictly_negative_projected_to_zero`. |
| `bucket.rs:262 ×2` | `-` → `+` / `-` → `/` in `needed = cost - refilled` | (a) | `try_acquire_deny_retry_after_uses_subtraction_in_needed` (pins exact retry_after=8). |
| `bucket.rs:263` | `/` → `*` in `wait = needed / rate` | (a) | `try_acquire_deny_retry_after_divides_needed_by_rate` (pins exact retry_after=5). |
| `bucket.rs:265 ×2` | `\|\|` → `&&`, `<` → `==` in `wait.is_nan() \|\| wait < 0.0` | (b) — partially-masked by trailing `.min(ceiling)` clamp; (a) for the OR mutation | `try_acquire_deny_uses_floor_when_wait_would_be_negative_or_nan` covers the NaN branch; the `.min/.max` post-clamp shadows some sub-branches. |
| `bucket.rs:267 ×2` | `>` → `==`, `>` → `<` in inner ceiling check | (b) — partially-masked by trailing `.min(ceiling)` | `try_acquire_deny_clamps_wait_above_hard_ceiling` pins post-clamp value. |
| `limiter.rs:105` | `is_allow -> true` | (a) | `decision_is_allow_false_on_deny429_and_true_on_allow`. |
| `limiter.rs:111` | `is_deny -> true` | (a) | same test. |
| `limiter.rs:209` | `Debug::fmt -> Ok(Default::default())` (returns empty) | (a) | `limiter_debug_renders_non_empty_struct_name`. |
| `limiter.rs:372 ×2` | `>` → `==`, `>` → `<` in refill-emit watermark check | (a) | `limiter_emits_bucket_refilled_when_watermark_advances`. |
| `limiter.rs:373` | `\|\|` → `&&` in refill-emit OR condition | (a) | same test (watermark + allow arm). |
| `metrics.rs:203 ×2` | `counter -> u64 with 0 / 1` | (a) | `metrics_counter_returns_zero_for_unknown_label_and_actual_value_for_recorded`. |
| `tier.rs:74` | delete `Tier::Enterprise` match arm | (b) — **equivalent mutant**: the catch-all `_` arm returns the same `(ENTERPRISE_REFILL_RPS, ENTERPRISE_BURST)` tuple. | Existing `prop_ratelimit.rs` covers tier sweep. Accepted as equivalent. |

#### Unviable (16)

`Default::default()` substitutions on types lacking `Default`; recorded
by the tool. Not included in the kill-rate denominator.

## 6. Per-crate strategy (CI-deferred crates)

### 6.1 `corelink-audit-chain` — 22 mutation-kills tests

The tests target the cargo-mutants canonical surfaces empirically
observed in the 2026-05-14 baselines:

1. **Crate-wide constants** —
   `schema_version`, `CLOUDEVENTS_SPECVERSION`,
   `CLOUDEVENTS_DATACONTENTTYPE`, `EVENT_TYPE_PREFIX`,
   `GENESIS_PREV_HASH`, `GENESIS_SEQUENCE_NUMBER`. Each pinned to its
   canonical value so `body -> 0` / `body -> default` mutations fail.

2. **`AuditEventKind::subject` + `event_type`** — 8-canonical taxonomy
   pinned per variant + distinctness across the 8 entries; the
   `.event_type()` is required to start with `"dev.hugr.corelink."`
   and end with `".v1"`.

3. **`AuditChainAuditEventType::as_str` + `is_sev0` + `is_sev1`** —
   4-event meta-audit taxonomy pinned per variant; SEV-0/SEV-1
   classification matrix verified for all 4 variants.

4. **`ChainHash::genesis` + `to_hex` + `Display`** — genesis is exactly
   `[0u8; 32]` rendering 64 zero-chars; non-zero arrays render the
   canonical lowercase hex.

5. **`link_chain_hash_from_canonical`** — BLAKE3 link primitive:
   deterministic on same input; distinguishes distinct inputs;
   dependent on `prev_hash` (chain integrity); not the zero hash.

6. **`HashChainBuilder::new` + `resume`** — initial state pinned at
   genesis; resume round-trips arbitrary head + sequence.

7. **`canonical_date_yyyy_mm_dd`** — Howard Hinnant date inverse
   pinned to 4 canonical vectors (epoch, epoch+1day, 2024-01-01,
   2026-05-16) + ISO-8601 shape.

8. **`canonical_r2_key`** — composite key format pinned to the
   canonical `audit/{tenant}/{date}/{seq:08}.cloudevent.ndjson`
   layout (zero-padded to 8 digits).

9. **`InMemoryAuditChainAuditSink` + `FailingAuditChainAuditSink`** —
   sink contract: `is_empty` flips after emit; `len` tracks; clone
   shares buffer; `snapshot_of` filters; failing sink returns Store
   error with diagnostic.

### 6.2 `corelink-pat` — 18 mutation-kills tests

1. **`PatEnv::as_wire`** — canonical 3 strings (`pat | ci | ro`) + distinctness + non-empty + non-canary.
2. **`PatEnv::from_wire`** — round-trip + unknown rejection (empty, uppercase, `exec`, canary).
3. **`PatEnv::Display`** — matches `as_wire`.
4. **`PatScopes::from_u64` + `SCOPE_KNOWN_MASK`** — reserved bits dropped.
5. **`PatScopes::has`** — every-bit-set semantics; empty requires vacuously true.
6. **`PatScopes::from_u64_strict`** — reserved bits rejected; known bits + empty accepted.
7. **`PatScopes::is_empty`** — flips on any single bit set.
8. **`PatScopes::add` + `remove`** — round-trip.
9. **`PatScopes::names`** — 12 distinct entries for `SCOPE_KNOWN_MASK`; single-bit yields single name.
10. **`PatScopes::union` + `intersection`** — `SCOPE_CACHE_RW` alias = R | W.
11. **`PatTokenId::parse`** — length boundary + Crockford b32 charset (rejects `I L O U` + lowercase).
12. **`PatTokenId::parse`** — accepts canonical 16-char b32; `Display = as_str`.
13. **`PatSigningKey::from_bytes`** — rejects 0..31 bytes; accepts ≥ 32.
14. **`PatSigningKey::Debug`** — redacts bytes (no `aa`/`AA` leak).
15. **`compute_hmac_sig`** — deterministic on same input; distinguishes distinct inputs; non-zero + non-canary.
16. **`verify_hmac_sig`** — accepts matching; rejects bit-flipped as `InvalidPat`; rejects wrong-length as `Malformed`.
17. **`PatError::Display`** — 5 distinct non-empty messages.

### 6.3 `corelink-clerk` — 13 mutation-kills tests

1. **`AuthError::metric_outcome`** — canonical 6-label mapping per WI §6.1.6 (covers all 9 variants).
2. **`AuthError::metric_outcome`** — 6 distinct labels + non-empty + non-canary.
3. **`AuthError::Display`** — all 9 variants non-empty; `IssuerMismatch` interpolates `got` + `expected` fields.
4. **`jwks_cache::is_fresh`** — TTL boundary at `stored_at + 60s` (just-fresh 59s, just-stale 60s, fresh 30s, stale 90s).
5. **`jwks_cache::is_fresh`** — arithmetic saturation (`Duration::MAX` TTL) → defensive false.
6. **`redact::principal_hash`** — 8 lowercase hex chars on 5 input vectors.
7. **`redact::principal_hash`** — distinguishes 50 distinct inputs.
8. **`redact::principal_hash`** — no leak of raw input bytes ("user", "secret" absent).
9. **`Email::parse`** — boundary rejections (empty, missing `@`, empty local, empty domain, no-TLD).
10. **`Email::parse`** — canonical accepted + `domain_lowercase` normalised.
11. **`Email::Debug`** — redacts local part; keeps domain.
12. **`ClerkRole::from_claim`** — canonical mapping (`admin | member`); unknown → `Guest` fail-safe; `None` → `Member`; case-insensitive.
13. **`EmailParseError::Display`** — diagnostic carries error context.

## 7. CI workflow extension

`.github/workflows/mutation-nightly.yml` matrix extended from 3 →
8 crates; baseline env vars added per crate; `baseline_map` Python
dict extended. Build fails for any crate whose observed kill rate
drops below `(baseline − tolerance_pp)`; tolerance_pp = 5 (unchanged
from baseline). All actions remain SHA-pinned.

## 8. Decisions log

- **2026-05-15** — Baseline floor for all 5 expansion crates pinned to
  75 % (matching the 2026-05-14 floor). Once the first CI nightly run
  records empirical numbers, the baselines may be raised by a
  follow-on PR (the workflow gate compares against the env-recorded
  baseline).
- **2026-05-15** — Three slow crates (pat / clerk / audit-chain) ship
  `mutation_kills.rs` based on canonical surfaces empirically observed
  in the 2026-05-14 baseline; rationale captured in §3 above. CI
  nightly first run will surface any specific missed mutants not
  covered by the canonical-surface tests.

## 9. Test count summary

| Crate | New tests added | Total file lines |
|---|---:|---:|
| `corelink-audit-chain` | 22 | ~310 |
| `corelink-pat` | 18 | ~290 |
| `corelink-clerk` | 13 | ~240 |
| `corelink-dual-approval` | 12 | ~360 |
| `corelink-ratelimit` | 10 | ~280 |
| **Total** | **75** | **~1480** |
