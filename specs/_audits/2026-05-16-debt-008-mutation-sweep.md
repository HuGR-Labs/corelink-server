---
id: "AUDIT-DEBT-008-MUTATION-SWEEP-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "Wave-21 (post-2026-05-15 DEBT-008 expansion) + wave-22 addendum (tenant-path)"
parent_wi: "WI-DEBT-008-MUTATION-FULL-SWEEP"
parent_audit: "specs/_audits/2026-05-15-mutation-full-sweep.md"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "debt-008", "corelink-hash", "corelink-tenant-path", "wave-21", "wave-22"]
---

# Wave-21 DEBT-008 mutation sweep — `corelink-hash` empirical baseline + 7 targeted kills

> **doc_status:** REVIEW · **scope:** expand DEBT-008 closure by landing
> the first empirical `cargo mutants` sweep on `corelink-hash` (43
> mutants; canonical content-address surface for the CAS layer), classify
> every surviving mutant, add targeted unit tests, and verify the
> post-additions kill rate via a second `cargo mutants` re-sweep.
>
> **Anchor:** `2026-05-15-mutation-full-sweep.md` (wave-13/14) recorded
> the 84.24 % empirical kill rate for `corelink-audit-chain` and shipped
> the CI-nightly path for the four slower crates (`corelink-pat`,
> `corelink-clerk`, `corelink-dual-approval`, `corelink-ratelimit`).
> `corelink-hash` was not in the CI-nightly matrix at wave-15. This
> audit adds it as the **2nd developer-laptop-feasible empirical sweep**
> in the DEBT-008 corpus.

## 1. Headline result — before / after table

| Crate | Mutants | Caught (pre) | Missed (pre) | Unviable | Viable | Kill rate (pre) | Targeted tests added | Caught (post) | Missed (post) | Kill rate (post) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `corelink-hash` | 43 | 28 | 8 | 7 | 36 | **77.78 %** | 7 | 35 | 1 (equivalent) | **97.22 %** |
| `corelink-tenant-path` (wave-22) | 21 | 17 | 0 (1 timeout) | 3 | 18 | **94.44 %** | 1 | 18 | 0 | **100.00 %** |

- **Pre-additions:** 77.78 % kill rate (28/36 viable mutants caught).
- **Post-additions:** 97.22 % kill rate (35/36; the single remaining
  miss is the mathematically equivalent `(hi << 4) | lo → (hi << 4) ^ lo`
  mutation classified in §4 below — see *Equivalent mutants*).
- **Crate lifted above the 80 % bar:** YES (97.22 % >> 80 %).

## 2. Invocation

```bash
# Initial empirical sweep (pre-additions)
cargo mutants -p corelink-hash --no-shuffle --jobs 4 --timeout 120 \
  --output ./target/mutants/corelink-hash.out

# Re-sweep after landing crates/corelink-hash/tests/mutation_kills.rs
cargo mutants -p corelink-hash --no-shuffle --jobs 4 --timeout 120 \
  --output ./target/mutants/corelink-hash-verify.out
```

- `--jobs 4`: 4-wide parallel scenario execution (deterministic across runs).
- `--timeout 120`: per-mutant test ceiling (2× the BLAKE3-throughput +
  constant-time variance gate budget).
- `--no-shuffle`: stable diff for CI integration.

**Wall-clock:** pre-additions 6m 16s; verification re-sweep 6m 54s on
the dev laptop (rustc 1.91.1, 8-core). Both runs reached terminal state
on 100 % of the 43-mutant population.

## 3. Surviving-mutant table (pre-additions, 8 total)

| # | File | Line | Mutation | Genre | Classification |
|---|---|---:|---|---|---|
| 1 | `digest.rs` | 57 | `replace * with /` in `from_hex` | arith | real test gap |
| 2 | `digest.rs` | 57 | `replace * with +` in `from_hex` | arith | real test gap |
| 3 | `digest.rs` | 58 | `replace * with +` in `from_hex` | arith | real test gap |
| 4 | `digest.rs` | 59 | `replace \| with ^` in `from_hex` | bitwise | **equivalent** |
| 5 | `digest.rs` | 94 | `Digest::as_bytes → &[0; DIGEST_LEN]` | const-return | real test gap |
| 6 | `digest.rs` | 94 | `Digest::as_bytes → &[1; DIGEST_LEN]` | const-return | real test gap |
| 7 | `digest.rs` | 100 | `Display::fmt → Ok(Default::default())` | side-effect | real test gap |
| 8 | `digest.rs` | 106 | `Debug::fmt → Ok(Default::default())` | side-effect | real test gap |

### 3.1 Equivalent mutation — `(hi << 4) | lo → (hi << 4) ^ lo`

The hex-decode loop combines a hi nibble and a lo nibble into a byte:

```rust
*slot = (hi << 4) | lo;
```

Because `hi` and `lo` are each constrained to the range `0..=15` (one
nibble), `(hi << 4)` always has its **low four bits zero** and `lo`
occupies **only those low four bits**. The two operands therefore have
**no overlapping set bits**, and the truth table for `|` and `^` on
non-overlapping operands is bit-identical:

| hi << 4 (bits 4..7) | lo (bits 0..3) | OR | XOR |
|---:|---:|---:|---:|
| any | any | (hi<<4)+lo | (hi<<4)+lo |

The mutant is therefore observationally indistinguishable from the
original across the entire input domain. We classify it as **equivalent
mutation** (not a real test gap), document it in
`crates/corelink-hash/tests/mutation_kills.rs` header comment, and
exclude it from the kill-rate fix list.

## 4. Targeted tests added

All 7 tests landed in
`crates/corelink-hash/tests/mutation_kills.rs` (net-new file). Each
test maps 1:1 to a surviving mutant per §3 above:

| Test name | Kills mutant |
|---|---|
| `invalid_hex_byte_position_kills_mul_to_div_at_hi_nibble` | `digest.rs:57 *→/` |
| `invalid_hex_byte_position_kills_mul_to_add_at_hi_nibble` | `digest.rs:57 *→+` |
| `invalid_hex_byte_position_kills_mul_to_add_at_lo_nibble` | `digest.rs:58 *→+` |
| `as_bytes_returns_real_blake3_not_all_zero` | `digest.rs:94 → &[0;32]` |
| `as_bytes_returns_real_blake3_not_all_one` | `digest.rs:94 → &[1;32]` |
| `display_renders_full_hex_not_default` | `digest.rs:100 Display::fmt → Ok(default())` |
| `debug_renders_wrapped_hex_not_default` | `digest.rs:106 Debug::fmt → Ok(default())` |

### 4.1 Key design notes

- **Error-position arithmetic** (mutants 1–3): the existing
  `parse_non_hex_byte` test used i=2 (chunk-2, position 5). At i=2 the
  three arithmetic forms `i*2`, `i+2`, and `i*2+1` collide partially
  (2*2 == 2+2 == 4; 2*2+1 == 5; 2+2+1 == 5). The new tests poison the
  hi nibble at i=3 (pos 6) and the lo nibble at i=3 (pos 7) where
  `i*2 != i+2` (6 vs 5) and `i*2+1 != i+2+1` (7 vs 6). These two
  positions plus the chunk-3 inversion in the hi-nibble test
  collectively kill the mul-to-add and mul-to-div families.
- **`as_bytes` constant-return**: we pin the exact 32-byte
  `BLAKE3("hello world")` vector
  (`d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24`),
  so `[0; 32]` and `[1; 32]` mutants are killed by direct byte
  comparison. A second test (`as_bytes_returns_real_blake3_not_all_one`)
  performs the hex round-trip to confirm `as_bytes` is the inverse of
  `from_hex(to_hex(...))`.
- **`Display` / `Debug` short-circuit**: the
  `Ok(Default::default())` mutant returns success without writing to
  the formatter (empty string output). The tests assert (a) exact
  length (64 for Display; 64+8 for Debug), (b) exact textual content
  matches `to_hex()`, and (c) the `Digest(...)` wrapper for Debug.

## 5. Equivalent-mutation register (this audit)

| Crate | File:Line | Mutation | Reason |
|---|---|---|---|
| `corelink-hash` | `digest.rs:59` | `(hi << 4) \| lo → (hi << 4) ^ lo` | OR ≡ XOR when operands have no overlapping bits — see §3.1 truth table. |

This is the **second** equivalent mutation registered in the DEBT-008
corpus (the first was `audit.rs:178` `FailingAdminOpAuditSink::captured
→ vec![]` recorded in `2026-05-15-mutation-full-sweep.md §7.4`).

## 6. Crates considered but skipped (60-min budget triage)

| Crate | Test-cycle time | Mutant count | Projected sweep | Disposition |
|---|---:|---:|---:|---|
| `corelink-tenant-path` | n/a (test compile failure: `Uuid::now_v7` not in scope) | 21 | n/a | **BLOCKED on compile fix** (separate ticket; see Caveats §8) |
| `corelink-multipart-schema` | 31 s | 133 | ~25 min × 4-wide | deferred — exceeds 60-min lane |
| `corelink-r2-multipart` | not measured | not enumerated | ~30+ min projected | deferred to wave-22 |
| `corelink-handler-cas` | not measured | not enumerated | ~30+ min projected | deferred to wave-22 |
| `corelink-quota-cas` | not measured | not enumerated | ~40+ min projected | deferred to wave-22 |
| `corelink-dedup` | not measured | not enumerated | ~25+ min projected | deferred to wave-22 |
| `corelink-chunker` | not measured | not enumerated | ~25+ min projected | deferred to wave-22 |
| `corelink-auth-schema` | not measured | not enumerated | ~25+ min projected | deferred to wave-22 |
| `corelink-webauthn` | not measured | not enumerated | ~50+ min projected | deferred to wave-22 |

The 60-min wave-21 budget accommodated **one** empirical sweep + its
verification re-sweep on the smallest critical-surface crate
(`corelink-hash`, 344 LOC). The remaining critical crates remain on the
CI-nightly lane (`.github/workflows/mutation-nightly.yml`) per the
wave-14 plumbing — or queue for wave-22 follow-on dispatch.

## 7. DEBT-008 status delta

| Crate | Pre-wave-21 status | Post-wave-21 status | Empirical kill rate |
|---|---|---|---:|
| `corelink-audit-chain` | CLOSED (wave-13) | unchanged | 84.24 % → ≥ 95 % projected post-additions |
| `corelink-pat` | PARTIAL (CI-nightly) | unchanged | pending first CI run |
| `corelink-clerk` | PARTIAL (CI-nightly) | unchanged | pending first CI run |
| `corelink-dual-approval` | PARTIAL (CI-nightly) | unchanged | pending first CI run |
| `corelink-ratelimit` | PARTIAL (CI-nightly) | unchanged | pending first CI run |
| **`corelink-hash`** | not in matrix | **CLOSED** (this audit) | **97.22 %** |
| **`corelink-tenant-path`** | BLOCKED (compile error) | **CLOSED** (wave-22 addendum §11) | **100.00 %** |

DEBT-008 remains **PARTIAL** at the register level (4 crates still on
CI-nightly + N un-swept critical crates), but the empirically-closed
subset grows from `{audit-chain}` to `{audit-chain, hash,
tenant-path}`. The `corelink-tenant-path` empirical 100.00 % rate is the
new highest measured kill rate in the corpus, surpassing
`corelink-hash`'s 97.22 % (which retains the equivalent-mutant `|→^`
floor).

## 8. Caveats

1. ~~**`corelink-tenant-path` compile error:** `cargo test -p
   corelink-tenant-path` fails to compile because the bench/test code
   references `Uuid::now_v7()` which is not in scope under the current
   `uuid` feature set. This blocks mutation testing for the canonical
   tenant-isolation crate; tracked as a follow-on ticket (not in
   wave-21 scope).~~ **RESOLVED in wave-22 addendum §11** — `uuid`
   feature set lifted to `["v7", "rng-getrandom"]` on top of the
   workspace pin; empirical sweep landed at 100.00 % kill rate.
2. **The equivalent `|→^` mutant** is documented in §3.1 and in the
   `mutation_kills.rs` header comment. If a future refactor changes
   the nibble-domain invariant (e.g., accepts wider input), the mutant
   may cease to be equivalent and the OR-to-XOR assertion would need
   a real test.
3. **Wave-21 budget was 60 min:** the audit covers a single crate
   (`corelink-hash`). The remaining critical-surface crates documented
   in §6 are queued for wave-22 dispatch under the same per-crate
   sweep-then-fix protocol.

## 9. Decisions log

- **2026-05-16** — `corelink-hash` empirical pre-additions kill rate:
  77.78 % (28 caught / 36 viable / 8 missed). 7 of 8 misses
  classified as real test gaps; 1 classified as equivalent (`|→^`
  on non-overlapping nibbles).
- **2026-05-16** — 7 targeted tests added in
  `crates/corelink-hash/tests/mutation_kills.rs`; all pass under
  `cargo test -p corelink-hash --test mutation_kills`.
- **2026-05-16** — Empirical verification re-sweep kill rate:
  97.22 % (35 caught / 36 viable / 1 missed equivalent). Crate
  promoted from "not-in-matrix" to **CLOSED** at the >80 % bar.
- **2026-05-16** — DEBT-008 register entry updated: empirically-closed
  subset is now `{corelink-audit-chain, corelink-hash}`.

## 10. Test count summary

| Crate | Pre-additions | New (this audit) | Total |
|---|---:|---:|---:|
| `corelink-hash` | 20 (prop_hash + blob_store_contract) | 7 (`mutation_kills.rs`) | 27 |
| `corelink-tenant-path` (wave-22) | 21 (lib + integration; was failing compile) | 1 (`cache_is_not_empty_after_insertion`) | 22 |

## 11. Wave-22 addendum — `corelink-tenant-path` (2026-05-16)

> **Context:** caveat §8.1 (compile error blocking the sweep) was the
> only inflight blocker preventing `corelink-tenant-path` — the
> canonical tenant-isolation surface (CTRL-AUTH-004 / INV-TENANT-ISOLATION
> layer 5) — from being empirically swept under DEBT-008. This addendum
> lifts the block, lands the sweep, and promotes the crate to the
> empirically-closed subset.

### 11.1 Root cause + fix

- **Root cause:** `crates/tenant-path/Cargo.toml` declared
  `uuid = { workspace = true }`, which pulled the workspace
  feature set `["v4", "serde", "js"]`. The `cache.rs` test module
  and the `derive_prefix_cached` bench reference `Uuid::now_v7()`,
  which requires the `v7` feature. Compile failure: `function or
  associated item not found in uuid::Uuid` (6 occurrences across
  `cache.rs` test mod + `benches/derive_prefix_cached.rs`).
- **Fix:** lifted the dep to
  `uuid = { workspace = true, features = ["v7", "rng-getrandom"] }`.
  Pattern matches the established `corelink-audit` and
  `corelink-auth-schema` precedent (both add `v7` per-crate on top of
  the workspace pin). `rng-getrandom` is required because `now_v7()`
  needs a host RNG (the workspace pin's `js` feature only covers
  wasm32).

### 11.2 Empirical sweep results

**Pre-additions** (immediately after the compile fix):

```text
21 mutants tested in 12m 32s: 17 caught, 3 unviable, 1 timeouts
```

- Viable population: 18 (21 − 3 unviable).
- Caught: 17. Missed: 0. Timeouts: 1.
- Pre-additions kill rate: **94.44 %** (17/18).
- Timeout mutant: `cache.rs:180:9: replace TenantPrefixCache::is_empty
  -> bool with true`. The existing `cache_is_empty_initially` test
  asserts `cache.is_empty() == true` on a fresh cache, which the
  mutation also satisfies (it returns `true` unconditionally). No
  existing test exercises the **false** branch, so the test runtime
  cycles up to the 120 s ceiling without divergence.

### 11.3 Targeted test added

A single test in `crates/tenant-path/src/cache.rs`:

```rust
#[test]
fn cache_is_not_empty_after_insertion() {
    // Mutation guard (DEBT-008 wave-22): kills the
    // `is_empty -> bool with true` survivor by asserting the
    // false branch is reachable after a single populated entry.
    let tdk = fresh_tdk(0x66);
    let cache = TenantPrefixCache::new();
    let v = TdkVersion(1);
    let tenant = Uuid::now_v7();
    let _ = cache.get_or_derive(&tdk, v, tenant);
    assert!(!cache.is_empty());
    assert_eq!(cache.len(), 1);
}
```

This exercises the **false** branch of `is_empty()` after a single
insertion, immediately failing the `replace -> true` mutation by
inverting the assertion that the original would satisfy.

### 11.4 Verification re-sweep

```text
21 mutants tested in 9m 24s: 18 caught, 3 unviable
```

- Viable population: 18.
- Caught: 18. Missed: 0. Timeouts: 0.
- Post-additions kill rate: **100.00 %** (18/18).
- This is the highest measured kill rate in the DEBT-008 corpus,
  surpassing `corelink-hash`'s 97.22 % (which retains the
  equivalent-mutant `|→^` floor at one missed mutant).

### 11.5 Surviving-mutant table — none

Post-additions, **zero** viable mutants survive. The 3 unviable
mutants are documented for completeness:

| File:Line | Mutation | Why unviable |
|---|---|---|
| `cache.rs:121` | `get_or_derive -> TenantPrefix with Default::default()` | `Default` not implemented for `TenantPrefix` — won't compile under mutation. |
| `prefix.rs:63` | `TenantDerivationKey::from_bytes -> Self with Default::default()` | `Default` not implemented for `TenantDerivationKey` (zeroize discipline forbids it). |
| `prefix.rs:149` | `derive_prefix -> TenantPrefix with Default::default()` | Same — `Default` is intentionally absent on `TenantPrefix`. |

The absence of `Default` on the three key types is itself a
correctness invariant (a zero-value tenant prefix or all-zero TDK
would be a security footgun); the mutation-tool's failure to
synthesize these mutants is a *positive* signal, not a gap.

### 11.6 Invocation

```bash
# Pre-additions sweep (after Cargo.toml fix, before targeted test)
cargo mutants -p corelink-tenant-path --no-shuffle --jobs 4 \
  --timeout 120 \
  --output ./target/mutants/corelink-tenant-path.out

# Verification re-sweep (after targeted test landed)
cargo mutants -p corelink-tenant-path --no-shuffle --jobs 4 \
  --timeout 120 \
  --output ./target/mutants/corelink-tenant-path-verify.out
```

### 11.7 Wave-22 decisions log

- **2026-05-16** — `corelink-tenant-path` Cargo.toml lifted to
  `uuid` features `["v7", "rng-getrandom"]`; `cargo build -p
  corelink-tenant-path` and `cargo test -p corelink-tenant-path`
  both green.
- **2026-05-16** — empirical pre-additions sweep: 17/18 caught
  (94.44 %); 1 timeout on `is_empty -> bool with true`.
- **2026-05-16** — targeted test `cache_is_not_empty_after_insertion`
  added (1 net-new test in `crates/tenant-path/src/cache.rs`).
- **2026-05-16** — verification re-sweep: 18/18 caught (100.00 %);
  zero misses, zero timeouts.
- **2026-05-16** — DEBT-008 empirically-closed subset extended to
  `{corelink-audit-chain, corelink-hash, corelink-tenant-path}`.
  Caveat §8.1 resolved.
