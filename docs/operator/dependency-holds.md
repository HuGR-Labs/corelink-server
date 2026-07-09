# Rust dependency holds

Honest, evidenced record of Rust dependency majors that are **intentionally held
back** — each one was actually attempted (bump → resolve → compile), broke for a
specific, captured reason, and was reverted rather than force-adopted with a hack
(no `#[allow]`, no `[patch]`, no pin-around). This file is **documentation, not a
gate**: it converts vague "we're behind on deps" into a tracked, reproducible hold
with the exact upstream condition that unblocks each one.

Toolchain of record for these attempts: **rustc/cargo 1.91.1**
(`1.91.1-x86_64-apple-darwin`).

---

## rusqlite 0.32 → 0.40  — HELD (2026-07-09)

**Owners:** `corelink-container` (pkg `corelink-server`), `corelink-ops`
(both `features = ["bundled"]`).

**What happens:** resolves fine (`rusqlite 0.40.1` pulls
`libsqlite3-sys 0.38.1`), then the `libsqlite3-sys` **build script** fails to
compile:

```
error[E0658]: use of unstable library feature `cfg_select`
   --> libsqlite3-sys-0.38.1/build.rs:110:9
    |
110 |         cfg_select! {
    = note: see issue #115585 <https://github.com/rust-lang/rust/issues/115585>
error: could not compile `libsqlite3-sys` (build script) due to 1 previous error
```

**Root cause:** `rusqlite 0.40` requires `libsqlite3-sys ^0.38`, and
`libsqlite3-sys 0.38`'s `build.rs` uses the `cfg_select!` macro, which is an
**unstable** library feature (`cfg_select`, rust-lang/rust#115585) not stabilized
on the pinned toolchain (1.91.1). Cannot be worked around without a nightly/newer
stable, which would be a toolchain change, not a dependency change.

**Unblock condition:** bump the pinned toolchain to a rustc that has stabilized
`cfg_select` (or a future `libsqlite3-sys` release that drops the unstable macro
from its build script). Re-attempt the bump then.

---

## password-hash 0.5 → 0.6  — HELD (2026-07-09)

**Owners:** `corelink-auth`, `corelink-pat` (both
`features = ["alloc", "rand_core"]`); `corelink-worker` (dev-dependency,
`features = ["alloc"]`).

**What happens:** resolves by **adding** `password-hash 0.6.1` *alongside* the
existing `password-hash 0.5.0` (the latter is still pulled by `argon2 0.5.3`),
then fails to compile:

```
error[E0432]: unresolved imports `password_hash::Salt`, `password_hash::SaltString`
  --> crates/corelink-pat/src/argon.rs:34:21
   | use password_hash::{Salt, SaltString};
   |                     ^^^^  ^^^^^^^^^^ no `SaltString` in the root, no `Salt` in the root
error[E0412]: cannot find type `Salt` in crate `password_hash`
  --> crates/corelink-pat/src/mint.rs:157:30
```

**Root cause:** `password-hash 0.6` reorganized its public API — the `Salt` /
`SaltString` types are no longer re-exported from the crate root — AND `argon2 0.5`
still pins `password-hash ^0.5`, so the two majors coexist and `argon2 0.5`'s API
surfaces the *0.5* salt types, not the 0.6 ones. password-hash cannot be bumped in
isolation: it is a **coordinated bump with `argon2` (0.5 → 0.6)** plus source
changes to the moved import paths. That is out of scope for a single-dep bump and
out of scope for a Cargo-only change.

**Unblock condition:** bump `argon2 0.5 → 0.6` in lockstep (argon2 0.6 pairs with
password-hash 0.6 and re-exports the salt types) and update the `use
password_hash::{Salt, SaltString}` import sites in `corelink-pat` (and the argon2
call sites in `corelink-auth`). Do it as one `argon2 + password-hash` PR.

---

## rand_chacha 0.9 → 0.10  — HELD (2026-07-09)

**Owners:** 22 crates (`corelink-ac`, `corelink-cas`, `corelink-gc`,
`corelink-worker`, the billing family, the privacy family, `corelink-ops`,
`corelink-telemetry`, `corelink-tracing`, …).

**What happens:** resolves cleanly (`rand_chacha 0.10.0` pulling
`rand_core 0.10.1`, which is already in the graph) and **21 of the 22 owners
compile**. Only `corelink-worker`'s timing-padding middleware (behind the
`tower-middleware` feature) breaks:

```
error[E0599]: no function or associated item named `seed_from_u64` found for struct `ChaCha20Rng`
   --> crates/corelink-worker/src/middleware/timing_padding/padding.rs:42:40
    | 42 | let mut rng = ChaCha20Rng::seed_from_u64(request_id_seed);
note: there are multiple different versions of crate `rand_core` in the dependency graph
    | use rand::{rngs::OsRng, Rng, SeedableRng, TryRngCore};
    |                              ----------- `SeedableRng` imported here doesn't correspond
    |                                          to the right version of crate `rand_core`
```

(same failure at `stats.rs:174`).

**Root cause:** `rand_chacha 0.10` implements `rand_core 0.10`'s `SeedableRng`,
but `corelink-worker` imports `SeedableRng` from `rand` 0.9 (which re-exports
`rand_core 0.9`'s trait) and calls `ChaCha20Rng::seed_from_u64(...)`. Because
`rand` is still on `rand_core 0.9`, the trait the code has in scope does not match
the trait the 0.10 RNG type implements, so `seed_from_u64` is not found. This
exactly confirms the pre-bump hypothesis (mixed `rand_core` majors). Splitting the
dep (worker pinned to 0.9 while the other 21 owners go 0.10) would leave the SAME
dependency at two majors across the tree — a split-version inconsistency the
zero-debt mandate forbids — so the whole bump is held rather than partially
adopted.

**Unblock condition:** either (a) migrate `corelink-worker`'s `timing_padding`
module to seed `ChaCha20Rng` via `rand_chacha`'s own `SeedableRng`
(`use rand_chacha::rand_core::SeedableRng`) at both call sites, or (b) bump `rand`
to a release that rides `rand_core 0.10` and do the whole `rand` + `rand_chacha`
family in one PR. Either is a source change beyond a Cargo-only bump.

---

_Last reviewed: 2026-07-09 (branch `deps/rust-majors-frontier`)._
