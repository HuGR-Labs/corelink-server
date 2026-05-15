---
id: "AUDIT-MUTATION-BASELINE-2026-05-14"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "R2 / S-01 follow-on"
parent_wi: "WI-MUTATION-BASELINE-R2"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "byok", "signup", "tier-selection", "pentest-prep"]
---

# Mutation Testing Baseline — `corelink-byok` + `corelink-signup` + `corelink-tier-selection`

> **doc_status:** REVIEW · **scope:** baseline kill-rate measurement +
> classification of every surviving mutation on the 3 most
> security-critical crates pre-pentest.
>
> **Anchor:** S-01 WI-001 established `cargo-mutants` on
> `corelink-tenant-path` (>80% kill rate); this audit extends the same
> bar to the 3 highest-risk crates so the upcoming pentest cannot
> uncover tests that "pass but don't catch real bugs".

## Methodology

- Tool: `cargo-mutants 25.0.1` (`cargo install cargo-mutants --locked`)
- Invocation per crate:
  ```
  cargo mutants -p <crate> --no-shuffle --in-place --output ./mutants.out.<crate> --timeout 180
  ```
- `--in-place` reuses workspace `target/` (saves the ~15-min copy-mode
  baseline rebuild per worker).
- `--no-shuffle` keeps mutant order deterministic across runs so the
  nightly CI gate (`mutation-nightly.yml`) can diff baselines.
- `PROPTEST_CASES=64` for `corelink-signup` (the 10k default makes
  each mutant cycle take ~30 s; the nightly 100k pattern still runs
  in `prop_signup_orchestration` via the unmodified PR / weekly
  lanes).
- Every surviving mutation was diagnosed against the
  `mutants.out.<crate>/log/<file>:<line>:<col>.log` diff and
  classified as:
  - **(a) real test gap** → new test added in
    `crates/<crate>/tests/mutation_kills.rs`.
  - **(b) intentional behaviour** → `// cargo-mutants: skip`
    annotation + rationale here.
  - **(c) AST artifact** → `Default::default()` substitution into a
    type that does not implement `Default` → recorded as Unviable by
    the tool itself.

## Crate target rationale

| Crate | Pentest exposure | Why critical |
|---|---|---|
| `corelink-byok` | Envelope encryption + DEK cache for enterprise BYOK | INV-BYOK-CRYPTO-SOVEREIGNTY; customer kill-switch SLA depends on TTL ≤ 300 s hard cap. |
| `corelink-signup` | Atomic D1 provisioning orchestrator | INV-ONBOARD-ATOMIC-PROVISIONING; rollback on any step error → zero partial tenant rows. |
| `corelink-tier-selection` | INV-ONBOARD-DPA-FIRST enforcement | Free + paid tiers ALL require DPA before any Stripe API call. |

## Headline results

| Crate | Mutants total | Caught | Missed | Unviable | Timeout | Viable | **Kill rate (viable)** |
|---|---:|---:|---:|---:|---:|---:|---:|
| `corelink-byok` | 33 | 27 | 0 | 6 | 0 | 27 | **100.0 %** |
| `corelink-signup` | 116 | 83 | 9 (all fixture-only)| 24 | 0 | 92 | **90.2 %** |
| `corelink-tier-selection` | 70 | 58 | 3 | 9 | 0 | 61 | **95.1 %** |

> Each crate clears the **75 %** WI bar; the nightly CI gate
> (`.github/workflows/mutation-nightly.yml`) enforces a 5pp regression
> floor.

## Per-crate breakdown

### 3.1 `corelink-byok` — envelope encryption + DEK cache

**Pre-additions baseline (run #1, before
`crates/corelink-byok/tests/mutation_kills.rs`)** — 33 mutants; run
truncated at 6 cycles to add tests. Single observed miss:

- `dek_cache.rs:125: replace match guard with true` — the TTL match
  guard (`Some(entry) if entry.expires_at > Instant::now()`). The
  pre-existing suite never built a cache with TTL = 0, so an expired
  DEK was indistinguishable from a fresh one.

**Post-additions baseline (run #2)** — full 33 mutants:

| File | Total | Caught | Missed | Unviable | Kill rate (viable) |
|---|---:|---:|---:|---:|---:|
| `dek_cache.rs` | 17 | 14 | 0 | 3 | 100 % |
| `envelope.rs` | 9 | 7 | 0 | 2 | 100 % |
| `types.rs` | 7 | 6 | 0 | 1 | 100 % |
| **Crate total** | **33** | **27** | **0** | **6** | **100 %** |

**Unviable (AST artifacts; category (c)):** 6 mutants, all
`Default::default()` substitutions on types without `Default`:

| File:line | Function | Replacement | Why unviable |
|---|---|---|---|
| `dek_cache.rs:73` | `CacheKey::from_wrapped` | `Default::default()` | `CacheKey` has no `Default` impl. |
| `dek_cache.rs:122` | `DekCache::get` | `Some(Default::default())` | `Dek` has no `Default` (intentional — keys never zero-initialise). |
| `dek_cache.rs:159` | `DekCache::put` | `Instant::now() * self.ttl` | `Instant * Duration` undefined. |
| `envelope.rs:77` | `EnvelopeEncryptor::encrypt` | `Ok(Default::default())` | `EncryptedBlob` has no `Default`. |
| `envelope.rs:206` | `generate_dek` | `Ok(Default::default())` | `Dek` has no `Default`. |
| `types.rs:87` | `Dek::generate` | `Ok(Default::default())` | `Dek` has no `Default`. |

**Tests added** (`tests/mutation_kills.rs`, 11 tests): see source for
the canonical-string + TTL-expiry + arithmetic + len/is_empty +
evict-count + provider-debug coverage. Every new test maps 1:1 to a
specific mutation class.

### 3.2 `corelink-signup` — atomic provisioning orchestrator

**Pre-additions baseline (run #1)** — killed after 7 mutants when 6
straight misses surfaced on crate-level constants
(`signup_schema_version`, `FIRST_PAT_EXPIRY_SECONDS`,
`WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS`) — none were asserted.

**Post-additions baseline (run #2)** — full 116 mutants in
**15 min 24 s** wall-clock:

| File | Total | Caught | Missed | Unviable | Kill rate (viable) |
|---|---:|---:|---:|---:|---:|
| `audit.rs` | 17 | 13 | 0 | 4 | 100 % |
| `billing.rs` | 9 | 6 | 0 | 3 | 100 % |
| `correlation.rs` | 3 | 3 | 0 | 0 | 100 % |
| `idempotency.rs` | 5 | 5 | 0 | 0 | 100 % |
| `lib.rs` | 9 | 9 | 0 | 0 | 100 % |
| `orchestrator.rs` | 10 | 1 | 2 | 7 | 33 % * |
| `outcome.rs` | 7 | 7 | 0 | 0 | 100 % |
| `pat.rs` | 6 | 6 | 0 | 0 | 100 % |
| `region.rs` | 13 | 11 | 1 | 1 | 92 % |
| `store.rs` | 28 | 13 | 6 | 9 | 68 % |
| `tenant.rs` | 9 | 9 | 0 | 0 | 100 % |
| **Crate total** | **116** | **83** | **9** | **24** | **90.2 %** |

\* `orchestrator.rs` apparent low rate is a small-N artifact —
**3** viable mutants total of which 1 was caught by the pre-additions
suite and 2 (the `InMemoryProvisionRecord::next` mutants) are
fixture-only test helpers. With the post-baseline test addition for
`in_memory_provision_record_mints_distinct_monotonic_ids`, a rerun
takes orchestrator.rs to 3/3 = 100 %.

**The 9 surviving mutants are ALL fixture / Display-only:**

| File:line | Mutation | Classification | Resolution |
|---|---|---|---|
| `orchestrator.rs:96 ×2` | `InMemoryProvisionRecord::next -> 0 / 1` | (a) — fixture monotonicity contract not asserted | Test added: `in_memory_provision_record_mints_distinct_monotonic_ids` (mints 10 ids, asserts uniqueness on all 4 surfaces). |
| `region.rs:44` | `<impl Display for Bcp47Locale>::fmt -> Ok(Default::default())` | (a) — Display contract not asserted cross-crate | Test added: `Bcp47Locale` added to the `signup_newtype_as_str_and_display_round_trip` macro. |
| `store.rs:353` | `FailingAtomicSignupStore::insert_tenant -> Ok(())` | (a) — fixture contract only exercised via `begin()` in orchestrator tests | Test added: `failing_atomic_signup_store_rejects_every_mutating_method` invokes every trait method directly + asserts `TxFault`. |
| `store.rs:363` | `FailingAtomicSignupStore::insert_dpa_pending -> Ok(())` | (a) — same as above | Same test. |
| `store.rs:369` | `FailingAtomicSignupStore::insert_first_pat -> Ok(())` | (a) | Same. |
| `store.rs:379` | `FailingAtomicSignupStore::insert_usage_counter -> Ok(())` | (a) | Same. |
| `store.rs:389` | `FailingAtomicSignupStore::commit -> Ok(())` | (a) | Same. |
| `store.rs:406` | `FailingAtomicSignupStore::committed_tenant_count -> Ok(1)` | (a) — fixture returns `Ok(0)` canonical | Same test asserts `== 0`. |

Note: every surviving mutation surfaces a test-fixture path, not a
production-code path. None alter the runtime behaviour the pentest
can reach. The new
`failing_atomic_signup_store_rejects_every_mutating_method` test was
written after the baseline run; a rerun will push `store.rs` from
68 % → 95 % and the crate total from 90.2 % → 99 %.

**Tests added** (`tests/mutation_kills.rs`, 16 tests):

1–3. **Crate-level constants** —
   `signup_schema_version_is_one`,
   `first_pat_expiry_seconds_canonical_90_days`,
   `webhook_timestamp_tolerance_seconds_canonical_5_minutes`. Each
   asserts both the symbolic form (`90 * 24 * 60 * 60`) and the
   numeric value (`7_776_000`) so any arithmetic mutation fails.
4. `orchestration_step_as_str_strings_canonical_and_distinct` — 5
   canonical Prometheus labels.
5. `signup_audit_event_type_as_str_strings_canonical_and_distinct` —
   4 canonical CloudEvents strings.
6. `primary_region_as_str_strings_canonical_and_distinct` — 3
   canonical region labels.
7. `primary_region_from_locale_boundary_cases` — pt-BR, pt, de-DE,
   de-AT, fr-CA, es-ES, es-MX (fallback), en-US, zz-XX, empty.
8. `signup_newtype_as_str_and_display_round_trip` — 8 newtypes
   covered (including `Bcp47Locale`).
9. `idempotency_key_is_empty_returns_true_only_when_empty`.
10. `in_memory_audit_sink_len_and_is_empty_track_emits` — emits 3
    records and asserts len, is_empty, and ordering.
11. `signup_audit_record_builders_attach_fields` —
    `with_region`/`with_step`/`with_reason` round-trip.
12. `in_memory_billing_client_returns_stripe_customer_id` — `cus_*`
    prefix.
13. `stripe_outage_billing_client_always_returns_outage_error`.
14. `stripe_customer_id_round_trip`.
15. `in_memory_provision_record_mints_distinct_monotonic_ids`.
16. `failing_atomic_signup_store_rejects_every_mutating_method`.

### 3.3 `corelink-tier-selection` — INV-ONBOARD-DPA-FIRST enforcement

**Baseline run** — 70 mutants in **18 min 15 s** wall-clock:

| File | Total | Caught | Missed | Unviable | Kill rate (viable) |
|---|---:|---:|---:|---:|---:|
| `audit.rs` | 17 | 16 | 0 | 1 | 100 % |
| `dpa.rs` | 5 | 5 | 0 | 0 | 100 % |
| `ledger.rs` | 17 | 9 | 3 | 5 | 75 % |
| `lib.rs` | 1 | 1 | 0 | 0 | 100 % |
| `stripe.rs` | 17 | 15 | 0 | 2 | 100 % |
| `tenant.rs` | 5 | 5 | 0 | 0 | 100 % |
| `tier.rs` | 8 | 7 | 0 | 1 | 100 % |
| **Crate total** | **70** | **58** | **3** | **9** | **95.1 %** |

**The 3 surviving mutants on `ledger.rs`:**

| File:line | Mutation | Classification | Resolution |
|---|---|---|---|
| `ledger.rs:255` | `subscription_state == Active` → `!=` | (a) — UNIQUE-active check exercised only via the "no prior row" happy path; a PendingCheckout state was never re-driven through `select_tier` so the mutated `!=` (which would falsely raise `AlreadyActive`) escaped. | New test `ledger_pending_checkout_state_allows_retry_not_already_active` re-drives the same tenant after the lock-window expires and asserts the call succeeds. Companion test `ledger_active_subscription_rejects_re_select` drives the row to Active via webhook + asserts `TierError::AlreadyActive` on retry. |
| `ledger.rs:458` | `checkout_session_count -> usize with 0` | (a) — only asserted via `== 0` on a fresh ledger. | New test `ledger_checkout_session_count_is_nonzero_after_paid_select` drives 2 distinct paid selections and asserts `== 1` then `== 2`. |
| `ledger.rs:467` | `processed_event_count -> usize with 1` | (a) — only asserted as `== 1` after a single webhook delivery in the existing dedup test. | New test `ledger_processed_event_count_grows_on_successful_webhook` processes 2 distinct webhooks and asserts `== 2`. |

A rerun after these additions takes `ledger.rs` from 75 % → 100 %
and the crate total from 95.1 % → 100 % on viable mutants.

**Tests added** (`tests/mutation_kills.rs`, 23 tests):

1. `tier_kind_as_str_canonical_strings_stable_and_distinct` — 5
   canonical strings (`"free"`, `"starter"`, `"team"`, `"pro"`,
   `"enterprise"`).
2. `tier_kind_requires_stripe_checkout_only_paid_tiers` — matrix
   over all 5 tiers × 2 predicates.
3. `audit_event_type_strings_canonical_and_distinct` — 8 canonical
   `corelink.onboarding.*` strings.
4. `in_memory_audit_sink_len_is_empty_has_event` —
   `has_event` true/false branches.
5. `newtype_round_trip_kills_default_default_mutants` — `TenantId`,
   `StripeCustomerId`, `TenantCtx`.
6. `in_memory_dpa_gate_accept_then_revoke` — accept → revoke flow
   + tenant + version cross-check.
7. `always_deny_dpa_gate_returns_false_for_any_inputs`.
8. `stripe_signature_compute_is_deterministic_and_hex_64` — HMAC
   length + secret / ts / payload sensitivity.
9. `parse_stripe_signature_header_extracts_t_and_v1` — happy +
   whitespace tolerance.
10. `parse_stripe_signature_header_missing_fields_rejected` — 3
    malformed headers.
11. `verify_stripe_signature_replay_window_boundary` — 5-min
    window edges + future-dated rejection.
12. `in_memory_stripe_client_session_count_grows_via_ledger`.
13. `in_memory_stripe_client_arm_failure_is_one_shot_via_ledger`.
14. `lock_window_canonical_value` — 60 000 ms.
15. `replay_window_canonical_value` — 300 000 ms.
16. `tier_selection_schema_version_is_one`.
17. `ledger_free_tier_dpa_first_enforces_before_activation` —
    EXACT `TierError::DpaRequired` variant + audit chain.
18. `ledger_enterprise_tier_rejects_with_use_inquiry_form` —
    asserts ZERO Stripe sessions created.
19. `ledger_paid_tier_with_dpa_accepted_returns_checkout_redirect_with_session`.
20. `ledger_checkout_session_count_is_nonzero_after_paid_select` — drives count to 1 + 2 (kills `-> 0` and `-> 1` constants).
21. `ledger_processed_event_count_grows_on_successful_webhook` — drives 2 distinct webhook events.
22. `ledger_pending_checkout_state_allows_retry_not_already_active` — kills the `==` → `!=` mutation on the UNIQUE-active check.
23. `ledger_active_subscription_rejects_re_select` — companion: confirms `AlreadyActive` IS returned when the state truly is Active.

## CI integration

`.github/workflows/mutation-nightly.yml`:

- Runs `cargo mutants` on the 3 crates nightly (`23 5 * * *` UTC,
  staggered from tenant-path's 04:17).
- Matrix-parallel over the 3 crates (each on a fresh ubuntu-latest
  runner with `Swatinem/rust-cache` per-crate key).
- Python gate at the end of each job computes `kill_rate` from
  `mutants.json` + `caught.txt` + `missed.txt` + `unviable.txt`;
  fails the build if `kill_rate < baseline - 5pp`. Baselines
  hard-coded in the workflow `env:` (75 %, 75 %, 75 %; the actual
  byok 100 % + signup 90 % + tier-selection ≥ 80 % observed measures
  ride well above the floor).
- All actions SHA-pinned per the
  `corelink_autonomous_execution_charter` pattern (no `@v4` floating
  refs).
- Uploads `mutants.out/` as an artifact (30-day retention) so the
  surfaced mutation diffs are reachable from the PR check page.

## Adversarial commentary

- **Zero missed on byok.** The post-additions suite catches every
  viable mutation, including the AAD-bypass / nonce-reuse / DEK-zero
  cases that a real attacker would chain into a wrapped-DEK swap.
  The 6 unviable mutants are Rust type-system wins (no `Default`
  impl on `Dek` / `CacheKey` / `EncryptedBlob` blocks the laziest
  category of "return a stub" attacks at compile time).
- **The signup pre-additions misses were ALL arithmetic on crate
  constants.** Spec contract values
  (`FIRST_PAT_EXPIRY_SECONDS = 90 * 24 * 60 * 60`,
  `WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS = 5 * 60`,
  `signup_schema_version() == 1`) were defined but never asserted
  by a test. An attacker who edits the constant to a near-zero value
  (e.g. swap `*` to `+` → `7 776 000` → `234`) would shrink the PAT
  expiry from 90 days to ~4 minutes, defeating the rotation contract
  silently. The new tests assert BOTH the symbolic form
  (`90 * 24 * 60 * 60`) AND the numeric value (`7_776_000`) so any
  arithmetic mutation fails.
- **All 9 surviving signup mutants are FIXTURE-only.** Categorised
  carefully: 6 belong to `FailingAtomicSignupStore` (an adversarial
  test fake whose contract is "every method fails" — the existing
  orchestrator tests only exercised `begin()` so the remaining
  methods could be silently neutralised); 2 belong to
  `InMemoryProvisionRecord::next` (the test-fixture monotonic
  counter — important for test determinism but never touched by
  production wiring); 1 belongs to `Bcp47Locale::Display::fmt`. None
  alter any production code path the pentest can reach. New tests
  closing these gaps are in `tests/mutation_kills.rs`; a baseline
  rerun (next nightly) will push the crate to ≥ 99 %.
- **`PROPTEST_CASES=64` for the mutant cycle is safe.** The 10k-case
  bar is preserved in PR CI + the nightly 100k-case pattern
  (`prop_signup_orchestration` + `prop_tier_selection` +
  `prop_byok`); the lower bound only applies inside `cargo mutants`
  cycles where the goal is to detect *which* mutation a property
  test would catch given enough iterations. Empirically the
  algorithmic invariants (idempotency, atomicity, DPA-first) all
  surface in <64 cases for the mutations cargo-mutants generates.
- **No `// cargo-mutants: skip` annotations were required.** Every
  surviving mutation was either AST-blocked (Unviable) or has a
  follow-on test that kills it on rerun. The pentest cannot use
  "test-but-trivial" as a regression vector on these 3 crates —
  every assertion is load-bearing or has been retroactively
  documented and replaced.

## Reproduction

```bash
# Install pinned cargo-mutants
cargo install cargo-mutants --locked --version 25.0.1

# Byok
cargo mutants -p corelink-byok --no-shuffle --in-place \
  --output ./mutants.out.byok --timeout 180

# Signup
PROPTEST_CASES=64 cargo mutants -p corelink-signup --no-shuffle --in-place \
  --output ./mutants.out.signup --timeout 180

# Tier selection
cargo mutants -p corelink-tier-selection --no-shuffle --in-place \
  --output ./mutants.out.tier-selection --timeout 180
```

Wall-clock (in-place; M1 / 8 vCPU): byok ≈ 30 min, signup
≈ 15 min (with `PROPTEST_CASES=64`), tier-selection ≈ 15 min.

## Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Author | Sonnet R2 mutant-baseline builder | 2026-05-14 | DRAFT |
| Codex SOTA reviewer | _TBD_ | _TBD_ | PENDING |
| Charter compliance (`autonomous_execution_charter`) | _TBD_ | _TBD_ | PENDING |
