# Current `main` drift readback — 2026-09-22 (`50a5`)

`origin/main` now resolves to
`50a5ab3a0e3e90bff56beb18fade52f50e3ff005` (`fix(ci): enforce run-scoped
endurance teardown (#2162)`). The delta from the reviewed server pin
`47f4db7f32bba5ee346e6157203cd480a3abe78b` includes six relevant paths:

- `byte_accounting/b126_m2_impl_01_part_02.rs`
- `byte_accounting/b126_m2_impl_02.rs`
- `byte_accounting/b126_m2_test_3_1.rs`
- `byte_accounting/b126_m2_test_4_1.rs`
- `routes/cas/single_setup.rs`
- `tools/cli/fuzz/Cargo.lock`

The Rust changes alter BYOK operation pinning/context propagation and add CAS
route behavior. Impact is selective: server relations for accounting, BYOK,
CAS and the shared hash/CAS limit surfaces need a current-main reconciliation;
billing, CF bindings and E2E are unaffected. The `corelink-hash` implementation
crate itself did not change. The immutable artifact set remains approved for
its declared source pin `47f4`; only claims that attempt to promote those bytes
to `main=50a5` are pending. This is targeted drift, not a campaign restart.
