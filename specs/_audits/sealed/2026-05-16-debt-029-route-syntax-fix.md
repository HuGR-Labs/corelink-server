# DEBT-029 — axum 0.7 / matchit 0.7 route-syntax fix on `ac.rs` + `admin.rs` — Audit Doc

> **Doc kind:** wave-30 stream-1 R-prep audit / engineering-side DEBT-029 full closure
> (no canonical front matter required — `_audits/` excluded from
> `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-30 R-prep DEBT-029 route-syntax agent (Claude Opus 4.7) on
> branch `wt/r-prep-debt-029-route-syntax`.
> **Base:** `main` @ `04f2dff` ("merge wt/r-prep-perf-baseline-ga-freeze into main (wave-29)").
> **Scope:** Close the latent route-syntax bug surfaced as follow-up #1 in
> `specs/_audits/2026-05-16-signup-corelink-dev-backend.md §13`. The path
> constants in `apps/server/src/routes/ac.rs` (`AC_LOOKUP_ROUTE`,
> `AC_UPDATE_ROUTE`) and `apps/server/src/routes/admin.rs`
> (`ADMIN_READ_ROUTE`) declared the `{name}` placeholder form used by
> matchit 0.8+, but the workspace is pinned at `axum = "0.7"` which uses
> matchit 0.7.3. matchit 0.7 only recognises `:name` (param) and `*name`
> (catch-all) wildcards — `{name}` segments parse as **literal path
> bytes**, so `Router::new().route(ROUTE, …)` accepted the templates
> without panic but actual `GET /v1/ac/<tenant>/<digest>` requests
> would have produced a router-level `404 Not Found` (handler never
> fired). The bug was silent because no `oneshot`-based integration
> test exercised either route prior to wave-30.
>
> **Cross-ref:** `specs/_audits/2026-05-15-debt-register.md` (DEBT-029 row
> appended below), `specs/_audits/2026-05-16-signup-corelink-dev-backend.md §13`
> (latent-bug discovery + capture), `apps/server/Cargo.toml` (axum 0.7 pin).

---

## 1. Charter

DEBT-029 is a P1 GA-blocker because the affected routes back two GA
SLOs:

* `SLO-AVAIL-AC` + `SLO-LAT-AC-HIT-P99` (AC lookup / update) — both
  named in `apps/server/src/routes/ac.rs §SLO emit` doc-comment and
  cross-linked from the audit closure list at
  `specs/_audits/2026-05-15-debt-register.md`.
* `SLO-AVAIL-CP` (admin control plane) — named in
  `apps/server/src/routes/admin.rs §SLO emit`.

If the broken route templates landed at GA the SLO emit sites would
record availability *under simulated load*, but every real-world
request would have flat-lined the metric at 100% router-404 — i.e.
the SLO would *look* healthy while the system was, in fact, unable
to serve a single AC or admin-read request. This is a classic
"silent-fail" that the wave-29 signup agent surfaced as a latent
bug and that this wave closes empirically.

The fix is mechanical: replace `{name}` with `:name` in the three
route constants + their corresponding doc-comments + the in-module
`#[test]` `route_constants_match_canonical_path*` assertions. The
test net is an integration smoke suite in
`apps/server/tests/admin_ac_route_smoke.rs` that drives real
`tower::ServiceExt::oneshot` requests through both routers and pins
the *handler-emitted* status + body — so any future regression
where someone re-introduces `{name}` will surface as a router-miss
404 with an empty body (which the asserts reject).

## 2. Bug surface — root-cause walkthrough

### 2.1 matchit 0.7 wildcard grammar

`apps/server/Cargo.toml:333` pins `axum = { version = "0.7", … }`.
The transitive `matchit` resolves to `0.7.3` in `Cargo.lock`. The
parser at `matchit-0.7.3/src/tree.rs:653-661` is explicit:

```rust
// a wildcard starts with ':' (param) or '*' (catch-all)
if c != b':' && c != b'*' {
    // … treated as literal byte …
}
```

A path template like `/v1/ac/{tenant}/{action_digest}` therefore
matches **only** the literal characters `{tenant}` (curly brace,
the word `tenant`, closing brace) — not the substring `tenant-a` a
client would send.

### 2.2 Why `Router::new().route(...)` did not panic

`Router::insert_at` (matchit 0.7) only rejects two things: two
wildcards in a single segment, and an empty wildcard name. Literal
curly braces are valid path-bytes per the URI ABNF, so the route
template *succeeded* at insert time. The bug therefore manifested
**only at request time** and **only against a real client** — both
of which were absent from the pre-wave-30 test surface.

### 2.3 Why the in-module unit tests didn't catch it

`ac.rs::tests::route_constants_match_canonical_path` and
`admin.rs::tests::route_constants_match_canonical_paths` only assert
the string value of the const, then call `router(st)` to verify the
router *constructs* — neither test dispatches a real request through
the router. They are smoke-only.

### 2.4 Why the wave-29 signup audit flagged it

The wave-29 stream-1 signup agent (which landed `signup.rs` with the
correct `:token` form per `SIGNUP_PILOT_ROUTE: &str =
"/v1/signup/pilot/:token"`) noticed the inconsistency while
cross-referencing the route conventions and surfaced it as
follow-up #1 in `specs/_audits/2026-05-16-signup-corelink-dev-backend.md §13`,
captured as **DEBT-029-engineering** to be added in wave-30.

## 3. Out-of-scope: `cas.rs`

`apps/server/src/routes/cas.rs` has the same bug (`CAS_READ_ROUTE: &str = "/v1/cas/{tenant}/{hash}"`).
The wave-30 stream-1 task scope is explicitly `ac.rs + admin*.rs`
per the dispatch charter, so `cas.rs` is **not** closed here. A
follow-up row is appended to §6 below and a matching DEBT-029-cas
entry will be opened in the DEBT register at wave-30 stream-2
dispatch. The bug surface is identical (silent router-miss);
priority remains P1 GA-blocker for the same SLO-binding reason
(`SLO-AVAIL-CAS`, `SLO-LAT-CAS-HIT-P99`).

## 4. Deliverables landed this wave

### 4.1 Route-constant fix — `ac.rs`

`apps/server/src/routes/ac.rs`:

* `AC_LOOKUP_ROUTE: &str = "/v1/ac/:tenant/:action_digest"` (was
  `"/v1/ac/{tenant}/{action_digest}"`).
* `AC_UPDATE_ROUTE: &str = "/v1/ac/:tenant/:action_digest"` (same
  rationale; the update path reuses the template, axum disambiguates
  by HTTP method).
* Doc-comments updated on both constants + on the
  `handle_lookup` / `handle_update` rustdoc to reflect the new
  syntax; module-level `//!` summary line updated.
* In-module unit test `route_constants_match_canonical_path` now
  asserts the `:name` form (was `{name}`).
* A doc-comment paragraph on `AC_LOOKUP_ROUTE` cites DEBT-029
  inline so a future reader sees the rationale next to the
  constant.

### 4.2 Route-constant fix — `admin.rs`

`apps/server/src/routes/admin.rs`:

* `ADMIN_READ_ROUTE: &str = "/v1/admin/read/:resource"` (was
  `"/v1/admin/read/{resource}"`).
* `ADMIN_MUTATE_ROUTE` unchanged (`/v1/admin/mutate` is wildcard-free).
* Module-level `//!` summary line + `handle_read` rustdoc updated.
* In-module unit test `route_constants_match_canonical_paths` now
  asserts the `:name` form (was `{name}`).
* DEBT-029 rationale paragraph added to the `ADMIN_READ_ROUTE`
  doc-comment.

### 4.3 Integration smoke test — `apps/server/tests/admin_ac_route_smoke.rs`

Five `tokio::test` cases, all driven through
`tower::ServiceExt::oneshot`:

| # | Test | Asserts |
|---|------|---------|
| 1 | `ac_lookup_route_reaches_handler_and_returns_handler_miss_404` | `GET /v1/ac/tenant-a/digest-xyz` returns 404 with body `"ac miss"` (handler-emitted; router-miss would yield empty body). |
| 2 | `ac_update_route_reaches_handler_and_returns_handler_created_201` | `PUT /v1/ac/tenant-a/digest-xyz` returns 201 with body `"digest-xyz"` (echoes the captured `:action_digest`). |
| 3 | `admin_read_route_reaches_handler_and_returns_handler_not_found_404` | `GET /v1/admin/read/tenant:unknown` returns 404 with body `"admin not found"`. |
| 4 | `admin_read_route_does_not_match_literal_braces_uri` | `GET /v1/admin/read/%7Bresource%7D` reaches the handler with literal `{resource}` as the resource name; pins all three route constants free of `{` characters. |
| 5 | `route_state_constructs_without_panic_on_native` | Both `router(state)` constructions succeed against the native target. |

The body-string assertions are load-bearing: a router-miss
emits an empty body, so the equality checks (e.g. `body == "ac miss"`)
distinguish "route reached the handler" from "matchit treated
`:tenant` as a literal segment".

### 4.4 DEBT register row update

`specs/_audits/2026-05-15-debt-register.md` — DEBT-029 added as
CLOSED in the §2 P1 table with the closure-commit reference; row
24 → 25 in the totals; v1.2.8 change-log entry appended to §7
mirroring the wave-29 v1.2.7 format.

## 5. Quality gates — green this wave

| Gate | Command | Result |
|------|---------|--------|
| Build | `cargo build -p corelink-server` | `Finished dev profile target(s) in 2m 16s` |
| Smoke tests | `cargo test -p corelink-server --test admin_ac_route_smoke` | `5 passed; 0 failed` |
| Lib regression | `cargo test -p corelink-server --lib` | `90 passed; 0 failed` |
| Clippy (crate) | `cargo clippy -p corelink-server --all-targets -- -D warnings` | clean |
| Clippy (workspace) | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| Spec validator | `python3 scripts/validate_specs.py` | `448 com schema + 9 YAML-only (457 total)` |
| Reference validator | `python3 scripts/validate_references.py` | `Nenhuma dangling reference detectada` |

## 6. Follow-ups

1. **`cas.rs` route-syntax** — same bug surface in
   `CAS_READ_ROUTE: &str = "/v1/cas/{tenant}/{hash}"`. Out of
   wave-30 stream-1 scope per dispatch charter. Open as
   **DEBT-029-cas** at the next dispatch with the same fix template
   (constant + doc-comment + in-module test + oneshot smoke).
   Priority P1 (same SLO-binding rationale as the AC/admin half:
   binds `SLO-AVAIL-CAS` + `SLO-LAT-CAS-HIT-P99`).
2. **Cross-route convention guard** — consider a `validate_specs.py`-
   adjacent grep-rule that rejects `pub const .*_ROUTE: &str = ".*\\{.*\\}"`
   so future regressions are caught at the spec-gate before they
   reach `cargo build`. Captured as wave-30 stream-2 stretch goal
   (orchestrator-bound, no DEBT row needed).
3. **axum 0.8 upgrade** — when the workspace bumps to axum 0.8 the
   route constants will need to revert to the `{name}` form (matchit
   0.8 grammar). Track under the existing post-GA dependency-bump
   schedule; no new DEBT row needed (covered by the standard
   workspace-upgrade ceremony).

## 7. Charter compliance pin

* `#![forbid(unsafe_code)]` — preserved on `apps/server/src/lib.rs`;
  the new test file inherits `#![forbid(unsafe_code)]` at its head
  (line 38) per the wave-29 convention from `admin_pilot.rs`.
* No `unwrap` / `expect` / `panic` in `src/` — only the existing
  test fixtures use these primitives (allowed by per-module
  `#[allow(clippy::unwrap_used, …)]` attributes).
* DCO sign-off + Co-Authored-By — applied at commit time.
* Synchronous bash only — confirmed (no `&` / `nohup` / background
  job invocations in this wave's worktree).
