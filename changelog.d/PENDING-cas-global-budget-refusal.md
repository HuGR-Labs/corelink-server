### Changed

- **B-056 (process-wide CAS byte budget) is NOT implemented — a written refusal,
  with the measurement behind it.** The item says copying
  `GLOBAL_TURBO_GET_BUDGET` makes this "not a design question". Verifying that
  before applying it: the exemplar sizes itself against *"a standard-1 instance
  (~4 GiB)"* (`turbo_v8.rs:192`, and its PUT twin at `:168`). That box no longer
  exists — #1066 moved all seven `wrangler.toml` blocks to `instance_type =
  "basic"`, and `cas.rs` recorded the measurement as
  `CONTAINER_MEMORY_BYTES = 1024 MiB`. The pattern to copy is calibrated to 4x
  the real machine.

  Nominal ceilings the binary's own comments declare, against 1024 MiB physical:
  cas per-tenant 512 (sized against the measured value) + turbo PUT 1600 + turbo
  GET 1600 + argon2 1024 = **4736 MiB, 4.6x the box**.

  Adding a `GLOBAL_CAS_READ_BUDGET` now would turn the item's `verify` green — it
  greps for the singleton's name — while the property the item asserts ("with N
  tenants the process is still unbounded") stayed **false**, since Turbo alone
  claims 3.1x the box. That is a decorative gate over the exact defect the item
  exists to name. Any honest CAS number requires deciding Turbo's in the same
  move, and changing Turbo's permits changes the throughput ceiling of a live
  cache surface — a product decision, not a transcription.

- **The finding propagates to B-077, and widens it.** B-077 names `adapter_pat.rs`
  and counts "~1.5 GiB documented over 1 GiB physical". It does not name
  `turbo_v8.rs`, which is the single largest orphan of the downsize (3200 MiB on
  its own). Measured with `grep -rn "standard-1" crates --include "*.rs"`: four
  sites, three of them sizing budget. `CONTAINER_MEMORY_BYTES` is still
  referenced by exactly one file.

  Correct sequence recorded in both items: size cas, turbo and argon2 against
  `CONTAINER_MEMORY_BYTES` **together**, and pin the set with a compile-time
  assert like `cas.rs:183` — today the only thing preventing the next drift.
