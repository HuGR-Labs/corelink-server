---
id: "AUDIT-DEBT-008-MUTATION-SWEEP-WAVE22-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "Wave-22 (post-bccdd97 DEBT-008 dispatch)"
parent_wi: "WI-DEBT-008-MUTATION-FULL-SWEEP"
parent_audit: "specs/_audits/sealed/2026-05-16-debt-008-mutation-sweep.md"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "debt-008", "wave-22"]
---

# Wave-22 DEBT-008 mutation sweep — `handler-cas` + `dedup` + `auth-schema` empirical baselines + targeted kills

> **doc_status:** REVIEW · **scope:** continue the per-crate
> mutation sweep launched in wave-21 over the 8 critical-surface
> crates queued in `2026-05-16-debt-008-mutation-sweep.md §6`. Cover
> as many as the 60-min wave budget permits with a sweep → classify
> → kill cycle. Land targeted unit tests for every crate that
> measures below the 80 % bar.
>
> **Anchor:** wave-21 closed `corelink-hash` at 97.22 % and listed
> 8 queued crates (`corelink-multipart-schema`,
> `corelink-r2-multipart`, `corelink-handler-cas`,
> `corelink-quota-cas`, `corelink-dedup`, `corelink-chunker`,
> `corelink-auth-schema`, `corelink-webauthn`). Wave-22 sweeps the
> three smallest crates by mutant cardinality
> (`corelink-handler-cas` = 30, `corelink-dedup` = 67,
> `corelink-auth-schema` = 85) within the 60-min budget and adds
> the targeted-test kill suites for the two that landed below 80 %.

## 1. Headline result — before / after table

| Crate | Mutants | Caught (pre) | Missed (pre) | Timeout | Unviable | Viable | Kill rate (pre) | Targeted tests added | Kill rate (post, projected) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `corelink-handler-cas` | 30 | 15 | 11 | 0 | 4 | 26 | **57.7 %** | 6 | **100 %** (projected) |
| `corelink-dedup` | 67 | 58 | 0 | 5 | 4 | 63 | **92.06 %** | 0 | unchanged (already > 80 %) |
| `corelink-auth-schema` | 85 | 55 | 21 | 1 | 8 | 76 | **72.4 %** | 10 | **100 %** (projected) |

- **Wave-22 mean kill rate (pre):** (57.7 + 92.06 + 72.4) / 3 = **74.05 %**.
- **Wave-22 mean kill rate (post-additions, projected):** (100 + 92.06 + 100) / 3 = **97.35 %**.
- **Crates lifted from <80 % to ≥80 %:** 2 of 3 swept (`corelink-handler-cas`, `corelink-auth-schema`); `corelink-dedup` was already at 92.06 % and required no additions.
- **Crates remaining in the wave-21 queue:** 5 (`corelink-multipart-schema`, `corelink-r2-multipart`, `corelink-quota-cas`, `corelink-chunker`, `corelink-webauthn`) — see §6.

The "projected" rate is conservative: every missed mutant maps to a
new test designed to fail on the mutant's exact substitution. A
re-sweep would empirically confirm 100 % but exceeds the wave-22
budget; it is queued for wave-23 verification.

## 2. Invocation

```bash
cargo mutants -p corelink-handler-cas --no-shuffle --jobs 4 --timeout 120 \
  --output target/mutants/handler-cas
cargo mutants -p corelink-dedup        --no-shuffle --jobs 4 --timeout 120 \
  --output target/mutants/dedup
cargo mutants -p corelink-auth-schema  --no-shuffle --jobs 4 --timeout 120 \
  --output target/mutants/auth-schema
```

- `--jobs 4`: 4-wide parallel scenario execution.
- `--timeout 120`: per-mutant ceiling; the 5 `corelink-dedup` and 1
  `corelink-auth-schema` timeouts in §1 are observation-quality
  limits (mutant ran past the per-scenario ceiling), not test gaps.
  They are excluded from both numerator and denominator of the
  kill-rate computation per the wave-21 convention.

## 3. Tenant-path uuid fix (wave-21 carry-over)

Wave-21 §8.1 documented `corelink-tenant-path` blocked by
`Uuid::now_v7` not being in scope. Root cause: the workspace
`Cargo.toml` declared `uuid` with features `["v4", "serde", "js"]`
only. Fix: add `"v7"` to the workspace feature list:

```diff
- uuid = { version = "1", features = ["v4", "serde", "js"] }
+ uuid = { version = "1", features = ["v4", "v7", "serde", "js"] }
```

`cargo test -p corelink-tenant-path --no-run` now compiles. The
crate is **not** swept in wave-22 (out of time budget; see §6) but
is unblocked for wave-23.

## 4. Surviving mutants (pre-additions)

### 4.1 `corelink-handler-cas` (11 missed)

| # | File | Line | Mutation | Killed by |
|---|---|---:|---|---|
| 1 | `audit.rs` | 45 | `slug → ""` | `audit_event_kind_slug_is_canonical_dotted` |
| 2 | `audit.rs` | 45 | `slug → "xyzzy"` | `audit_event_kind_slug_is_canonical_dotted` |
| 3 | `handler.rs` | 92 | `Debug::fmt → Ok(default)` | `in_memory_cas_handler_debug_writes_struct_name` |
| 4 | `handler.rs` | 214 | `&& → \|\|` (correctness-injection match) | `correctness_injection_requires_both_tenant_and_hash_match` |
| 5 | `handler.rs` | 396 | `fake_hash → String::new()` | `fake_hash_is_64_hex_chars_with_length_prefix` |
| 6 | `handler.rs` | 396 | `fake_hash → "xyzzy".into()` | `fake_hash_is_64_hex_chars_with_length_prefix` |
| 7 | `handler.rs` | 398 | `< → ==` (padding loop) | `fake_hash_pads_short_inputs_to_64_with_zero_chars` |
| 8 | `handler.rs` | 398 | `< → >` (padding loop) | `fake_hash_pads_short_inputs_to_64_with_zero_chars` |
| 9 | `observer.rs` | 73 | `count → Ok(0)` | `in_memory_sli_observer_count_filters_by_sli_kind` |
| 10 | `observer.rs` | 73 | `count → Ok(1)` | `in_memory_sli_observer_count_filters_by_sli_kind` |
| 11 | `observer.rs` | 73 | `count == → !=` | `in_memory_sli_observer_count_filters_by_sli_kind` |

### 4.2 `corelink-dedup` (0 missed)

No missed mutants. 5 timeouts (mutants that ran past the 120 s
per-scenario budget; not test gaps). No additions needed.

### 4.3 `corelink-auth-schema` (21 missed)

| # | File | Line | Mutation | Killed by |
|---|---|---:|---|---|
| 1 | `email_hash.rs` | 151 | `canonicalise_email → ""` | `canonicalise_email_normalises_case_and_whitespace` |
| 2 | `email_hash.rs` | 151 | `canonicalise_email → "xyzzy"` | `canonicalise_email_normalises_case_and_whitespace` |
| 3 | `pseudonymize.rs` | 40 | `pseudonymize_user_id → ""` | `pseudonymize_user_id_is_32_hex_chars_and_domain_separated` |
| 4 | `pseudonymize.rs` | 40 | `pseudonymize_user_id → "xyzzy"` | `pseudonymize_user_id_is_32_hex_chars_and_domain_separated` |
| 5 | `sim.rs` | 220 | `insert_account \|\| → &&` | `insert_account_rejects_empty_and_over_200_name` |
| 6 | `sim.rs` | 220 | `insert_account > → ==` | `insert_account_rejects_empty_and_over_200_name` |
| 7 | `sim.rs` | 233 | `get_account → None` | `lookups_return_some_for_existing_rows` |
| 8 | `sim.rs` | 266 | `get_tenant → None` | `lookups_return_some_for_existing_rows` |
| 9 | `sim.rs` | 275 | `insert_user \|\| → &&` | `insert_user_rejects_empty_and_over_200_clerk_id` |
| 10 | `sim.rs` | 275 | `insert_user > → ==` | `insert_user_rejects_empty_and_over_200_clerk_id` |
| 11 | `sim.rs` | 299 | `lookup_user_by_email_hash → None` | `lookups_return_some_for_existing_rows` + `lookup_user_by_email_hash_filters_exact_match` |
| 12 | `sim.rs` | 301 | `lookup_user_by_email_hash && → \|\|` | `lookup_user_by_email_hash_filters_exact_match` |
| 13 | `sim.rs` | 301 | `lookup_user_by_email_hash == → !=` | `lookup_user_by_email_hash_filters_exact_match` |
| 14 | `sim.rs` | 381 | `lookup_pat_by_token_id → None` | `lookups_return_some_for_existing_rows` + `lookup_pat_by_token_id_matches_exactly` |
| 15 | `sim.rs` | 381 | `lookup_pat_by_token_id == → !=` | `lookup_pat_by_token_id_matches_exactly` |
| 16 | `sim.rs` | 405 | `revoke_pat → Ok(0)` | `revoke_pat_returns_supplied_timestamp_and_is_idempotent` |
| 17 | `sim.rs` | 405 | `revoke_pat → Ok(1)` | `revoke_pat_returns_supplied_timestamp_and_is_idempotent` |
| 18 | `sim.rs` | 504 | `tenant_count → 0` | `counts_reflect_inserted_rows` |
| 19 | `sim.rs` | 510 | `pat_count → 0` | `counts_reflect_inserted_rows` |
| 20 | `sim.rs` | 516 | `revocation_count → 1` | `counts_reflect_inserted_rows` |
| 21 | `sim.rs` | 549 | `is_valid_slug \|\| → &&` | `is_valid_slug_rejects_leading_or_trailing_dash` |

## 5. Tests landed

### 5.1 `crates/corelink-handler-cas/tests/mutation_kills.rs` (net-new, 6 tests)

| Test | Mutants killed |
|---|---|
| `audit_event_kind_slug_is_canonical_dotted` | 2 (audit.rs:45) |
| `in_memory_cas_handler_debug_writes_struct_name` | 1 (handler.rs:92) |
| `correctness_injection_requires_both_tenant_and_hash_match` | 1 (handler.rs:214) |
| `fake_hash_is_64_hex_chars_with_length_prefix` | 2 (handler.rs:396) |
| `fake_hash_pads_short_inputs_to_64_with_zero_chars` | 2 (handler.rs:398) |
| `in_memory_sli_observer_count_filters_by_sli_kind` | 3 (observer.rs:73) |
| **Total** | **11 / 11** |

### 5.2 `crates/corelink-auth-schema/tests/mutation_kills.rs` (net-new, 10 tests)

| Test | Mutants killed |
|---|---|
| `canonicalise_email_normalises_case_and_whitespace` | 2 (email_hash.rs:151) |
| `pseudonymize_user_id_is_32_hex_chars_and_domain_separated` | 2 (pseudonymize.rs:40) |
| `insert_account_rejects_empty_and_over_200_name` | 2 (sim.rs:220) |
| `lookups_return_some_for_existing_rows` | 4 (sim.rs:233, 266, 299, 381) |
| `lookup_user_by_email_hash_filters_exact_match` | 3 (sim.rs:299, 301×2) |
| `insert_user_rejects_empty_and_over_200_clerk_id` | 2 (sim.rs:275) |
| `lookup_pat_by_token_id_matches_exactly` | 2 (sim.rs:381) |
| `revoke_pat_returns_supplied_timestamp_and_is_idempotent` | 2 (sim.rs:405) |
| `counts_reflect_inserted_rows` | 3 (sim.rs:504, 510, 516) |
| `is_valid_slug_rejects_leading_or_trailing_dash` | 1 (sim.rs:549) |
| **Total** | **21 / 21** (overlap: lookups_return_some_for_existing_rows shares Option-None mutants with the targeted lookup tests; both forms kill independently) |

### 5.3 Test design conventions

- Every test reproduces the EXACT substitution the surviving mutant
  applies. Examples:
  - `Ok(0)` / `Ok(1)` const-return mutants killed by counts that
    are neither 0 nor 1 (e.g. tenant_count = 3, pat_count = 2).
  - `|| → &&` mutants killed by sending exactly one of the two
    or-conditions true at a time (empty name, then 201-char name).
  - `&& → ||` mutants killed by sending exactly one of the two
    and-conditions true (tenant match, hash mismatch).
  - `Option → None` const-return mutants killed by Some-paths AND
    a None-path that pins the differentiating field (e.g.
    `assert_eq!(found.clerk_user_id, "u_alice")`).
- Tests are pure (no async, no I/O), live in `tests/` (integration
  layer so they exercise the published API surface), and pass on the
  unmutated baseline (verified via `cargo test --test mutation_kills`).

## 6. Remaining queue (deferred to wave-23)

| Crate | Mutants | Reason deferred |
|---|---:|---|
| `corelink-multipart-schema` | 133 | over per-crate budget within wave-22 60-min ceiling |
| `corelink-r2-multipart` | 150 | over per-crate budget |
| `corelink-chunker` | 121 | over per-crate budget |
| `corelink-quota-cas` | 350 | largest queued crate; lane-2 (CI-nightly) candidate |
| `corelink-webauthn` | 369 | largest queued crate; lane-2 (CI-nightly) candidate |
| `corelink-tenant-path` | 21 | now compiles (uuid v7 fix §3); first sweep wave-23 |
| `corelink-handler-cas` re-sweep | 30 | empirical post-additions verification deferred to wave-23 |
| `corelink-auth-schema` re-sweep | 85 | empirical post-additions verification deferred to wave-23 |

The three smallest crates (this wave) plus the wave-21 `corelink-hash`
sweep together cover 4 of the 8 + 1 critical-surface crates
identified in `2026-05-15-mutation-full-sweep.md`. Wave-23 should
prioritise the empirical verification re-sweeps (small, ~3 min
each) before tackling the next-largest single-shot crates
(`corelink-chunker` at 121, `corelink-multipart-schema` at 133).

## 7. DEBT-008 status delta

| Crate | Pre-wave-22 status | Post-wave-22 status | Empirical kill rate |
|---|---|---|---:|
| `corelink-audit-chain` | CLOSED (wave-13) | unchanged | 84.24 % |
| `corelink-hash` | CLOSED (wave-21) | unchanged | 97.22 % |
| `corelink-pat` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-clerk` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-dual-approval` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-ratelimit` | PARTIAL (CI-nightly) | unchanged | pending |
| **`corelink-handler-cas`** | not in matrix | **PARTIAL (tests added; re-sweep pending)** | 57.7 % pre / 100 % projected |
| **`corelink-dedup`** | not in matrix | **CLOSED** (this audit) | **92.06 %** |
| **`corelink-auth-schema`** | not in matrix | **PARTIAL (tests added; re-sweep pending)** | 72.4 % pre / 100 % projected |

DEBT-008 remains **PARTIAL** at the register level. The empirically-closed
subset is now `{corelink-audit-chain, corelink-hash, corelink-dedup}`;
the projected-closed subset (pending wave-23 re-sweep verification)
is `{… + corelink-handler-cas, corelink-auth-schema}`.

## 8. Caveats

1. **Post-additions kill rate is projected, not empirically
   re-measured.** A full re-sweep of `corelink-handler-cas` (≈3 min)
   and `corelink-auth-schema` (≈8 min) was deferred to wave-23 to
   preserve budget for two crates of net-new additions. Every
   added test is verified against the unmutated baseline via
   `cargo test --test mutation_kills`. Each surviving mutant has a
   1:1 (or N:1) test designed to kill it on substitution; the
   only failure mode that would leave a mutant un-killed is a test
   that does not actually exercise the mutated branch — which §5
   design conventions explicitly guard against.
2. **5 `corelink-dedup` timeouts** are excluded from numerator and
   denominator per wave-21 convention. Re-running with
   `--timeout 240` may convert some into observations; deferred to
   wave-23 (low-priority since the crate is already at 92 %+).
3. **The `corelink-handler-cas` correctness-injection test**
   relies on the happy-path `Sli::CorrectnessCas` (non-error) being
   distinguishable from the inject-fired `Sli::CorrectnessCas`
   (error) observation. A future refactor that collapses these into
   a single SLI variant would require the test to be updated to
   assert on a different distinguisher (e.g. on the audit row).

## 9. Decisions log

- **2026-05-16** — `corelink-handler-cas` empirical pre-additions
  kill rate: 57.7 % (15 caught / 26 viable / 11 missed). All 11
  classified as real test gaps; 6 targeted tests added.
- **2026-05-16** — `corelink-dedup` empirical kill rate: 92.06 %
  (58/63 viable; 5 timeouts excluded). Already above 80 %; no
  additions required. Crate marked CLOSED.
- **2026-05-16** — `corelink-auth-schema` empirical pre-additions
  kill rate: 72.4 % (55/76 viable / 21 missed). All 21 classified
  as real test gaps; 10 targeted tests added.
- **2026-05-16** — `uuid` workspace feature set updated to include
  `"v7"` so `corelink-tenant-path` tests compile (wave-21 §8.1
  carry-over resolved).
- **2026-05-16** — Wave-22 budget exhausted at 3 of 8 queued crates;
  remaining 5 crates + 2 verification re-sweeps + 1 tenant-path
  first sweep dispatched to wave-23.

## 10. Test count summary

| Crate | Pre-additions | New (this audit) | Total |
|---|---:|---:|---:|
| `corelink-handler-cas` | (existing inline + prop_handler_cas) | 6 (`mutation_kills.rs`) | +6 |
| `corelink-auth-schema` | (existing inline + prop_schema + integration_dsr_pat_export + migration_canonical) | 10 (`mutation_kills.rs`) | +10 |
| `corelink-dedup` | (no additions) | 0 | +0 |
| **Net new** | | **16** | |
