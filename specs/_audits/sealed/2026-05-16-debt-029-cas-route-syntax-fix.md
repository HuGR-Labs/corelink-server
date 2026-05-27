# DEBT-029-cas — `cas.rs::CAS_READ_ROUTE` axum-0.7 / matchit-0.7 route-syntax fix — 2026-05-16

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** DEBT-029 closure (wave-30 stream-1) left `cas.rs::CAS_READ_ROUTE` open as the follow-up surface; this is the matching closure for the CAS read path.
>
> **Related controls:** SLO-AVAIL-CAS-GET, SLO-LAT-CAS-GET-P99, CTRL-CAS-001 (CAS read path integrity).
>
> **Related WIs:** DEBT-029 (wave-30 stream-1 `wt/r-prep-debt-029-route-syntax`) — same surface on `ac.rs` + `admin.rs`.

## 1. Scope

Replace the matchit-0.8 `{name}` route placeholders with the matchit-0.7 (= axum-0.7) `:name` form on `apps/server/src/routes/cas.rs::CAS_READ_ROUTE` + matching doc-comments + in-module test assertion. Add an integration test net pinning handler-reach with body-string asserts. Identical pattern to the wave-30 stream-1 DEBT-029 closure on `ac.rs` + `admin.rs`.

Out of scope:
- Any other route surface (none remain — `grep -rn 'pub const.*ROUTE.*=.*"/.*{.*}'` returns 0 hits across `apps/server/src/` + `crates/` post-fix).
- Production wasm32 CF-Worker CAS handler (deferred per `trait-abstraction-defer`; the wasm32 cfg-gate in `cas.rs::build_handler` is unchanged).

## 2. Latent bug surface (pre-fix)

```rust
// apps/server/src/routes/cas.rs:45 (PRE-FIX)
pub const CAS_READ_ROUTE: &str = "/v1/cas/{tenant}/{hash}";
```

`Cargo.lock` confirms `matchit = "0.7.3"` transitively via `axum = "0.7"` (workspace pin). matchit 0.7.3 `tree.rs:653-661` only rejects `:` or `*` at wildcard-start position — `{` is a literal byte, so `Router::new().route("/v1/cas/{tenant}/{hash}", get(handle_read))` accepts the template without panic.

At request time, however, the literal-byte segment `{tenant}` will only match a URI segment that is *literally* the four characters `%7Btenant%7D` (or `{tenant}` post-percent-decode). Every real production request (e.g. `GET /v1/cas/acme-corp/blake3-abc123`) would produce a router-level `404 Not Found` with **the handler never firing**.

SLO impact: `SLO-AVAIL-CAS-GET` + `SLO-LAT-CAS-GET-P99` would have flat-lined at 100% miss while `Sli::AvailCasGet` + `Sli::LatencyCasGetP99` recorded nothing — same silent-fail GA-blocker class as the wave-30 stream-1 `ac.rs`/`admin.rs` surface.

Latent because no `oneshot` integration test exercised the CAS route pre-fix (the in-module unit test only asserted the *string value* of the constant, not that it composed into a working router). Discovery source: wave-30 stream-1 DEBT-029 closure follow-up.

## 3. Fix

```rust
// apps/server/src/routes/cas.rs:45 (POST-FIX)
/// Canonical CAS read route path (matchit-0.7 / axum-0.7 `:name` captures).
///
/// MUST use the `:name` form — matchit 0.7.3 (the version transitively
/// pinned via `axum = "0.7"`) parses `{name}` as **literal path bytes**
/// rather than a capture, which would silently route every real request
/// to a router-level 404. This was DEBT-029 (closed wave-30 stream-1)
/// on `ac.rs` + `admin.rs`; DEBT-029-cas closes the same surface here.
pub const CAS_READ_ROUTE: &str = "/v1/cas/:tenant/:hash";
```

Matching changes:
- Module-level rustdoc `//! GET /v1/cas/{tenant}/{hash}` → `:tenant/:hash`.
- Handler rustdoc `/// GET /v1/cas/{tenant}/{hash}` → `:tenant/:hash`.
- In-module unit test `route_constant_matches_canonical_path`: assertion updated to `/v1/cas/:tenant/:hash`.
- In-module unit test `route_constant_uses_matchit_0_7_colon_syntax_not_curly_braces` (NEW): pins no-curly-braces + presence of `:tenant` + `:hash`.

## 4. Test net (net-new)

`apps/server/tests/cas_route_smoke.rs` (NEW):

| Test | What it pins |
|---|---|
| `cas_read_route_reaches_handler_and_returns_handler_not_found_404` | Real `GET /v1/cas/tenant-a/abc123` reaches the handler; status = 404, body = `"not found"` (the handler-emitted string, NOT an empty router-miss body). |
| `cas_read_route_does_not_match_literal_braces_uri` | Percent-encoded `%7Btenant%7D/%7Bhash%7D` URI still reaches the handler with the literals as path params (post-fix the constant has no `{` so a curly URI never collides with a "matching" literal route). Also pins `!CAS_READ_ROUTE.contains('{')` + presence of `:tenant`/`:hash`. |
| `cas_route_state_constructs_without_panic_on_native` | `cas::router(state)` composes under the workspace-pinned matchit grammar (the same construction-time smoke wave-30 stream-1 used). |

Plus the in-module test `route_constant_uses_matchit_0_7_colon_syntax_not_curly_braces` (NEW, +1 in-module).

Total wave-31 net-new: 3 integration + 1 in-module = 4 tests; matches the wave-30 stream-1 DEBT-029 closure pattern (5 integration tests across 3 route constants).

## 5. Gates (all green)

| Gate | Result |
|---|---|
| `cargo build -p corelink-server` | green (32.95s) |
| `cargo test -p corelink-server --test cas_route_smoke` | 3/3 green |
| `cargo test -p corelink-server --lib` | 91/91 green (was 90; +1 in-module assertion) |
| `cargo clippy -p corelink-server --all-targets -- -D warnings` | clean |
| `validate_specs.py` | 449+9 OK |
| `validate_references.py` | 0 dangling |
| `check_migrations_additive.py` | 59 migrations all additive |

## 6. Charter constraints (preserved)

- `#![forbid(unsafe_code)]` — unchanged.
- No unwrap/expect/panic in `src/` — unchanged (test files use the test-allow attrs per existing convention).
- Fail-CLOSED on audit — unchanged (the handler's `CasHandlerError::AuditFailed` → 503 mapping at `cas.rs:140-144` is untouched).
- `#[non_exhaustive]` on public types — unchanged.

## 7. Residual route-syntax surface (audit)

`grep -rn 'pub const.*ROUTE.*=.*"/.*{.*}' apps/server/src/ crates/` post-fix returns **0 hits**. Combined with the wave-30 stream-1 closure (ac.rs + admin.rs), the entire `pub const *_ROUTE` constant surface across the workspace is now on matchit-0.7 `:name` syntax. No further DEBT-029-* follow-ups required.

## 8. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of DEBT-029-cas closure audit.**
