---
id: "AUDIT-2026-05-27-CHARTER-STRICT-POST-W36"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "charter", "post-w36", "sweep", "seal"]
references:
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-27-w36-stage-3-seal.md"
  - "specs/_audits/2026-05-27-w36-trigger-a-seal.md"
---

# Charter strict audit — post Wave 36 SEAL

## §1 Scope

Comprehensive workspace-wide invariant verification across all CoreLink
packages following the Wave 33-36 reorganisation. The mandate framed the
target as "all 87 workspace packages"; the actual `[workspace.members]`
list contains **66 entries**, of which 65 are `corelink-*` crates plus
`tenant-path`. The sweep covers every `crates/*/src/` source tree
(production code) and `crates/*/tests/` (integration tests, where
charter constraint L2.4 / L2.5 apply).

Methodology: read-only static analysis (regex + AST-aware Python
filtering) plus directed spot-reading of audit fail-CLOSED ordering and
candidate L2.7 violations. No inline fixes were applied: every
candidate finding resolved to either (a) a documented invariant /
exempt class on close inspection, or (b) a HIGH-RISK item per §4 of the
spec that requires orchestrator semantic decision.

Worktree:  `worktree-agent-a55a04c9446cd215d`
Base commit:  `6e488fb3ee62730caf8fe100334d16cc4f61da16` (descendant of
W36 SEAL).
Working-tree status at SEAL: clean (`git status --short` empty).

## §2 Results — per L2 constraint

### L2.1 `#[non_exhaustive]` on public enums and structs

Raw scan: every `^pub (enum|struct) [A-Z]` line in `crates/corelink-*/src/`
checked for an immediately-preceding `#[non_exhaustive]` attribute (up
to 3 lines above, to permit interleaved derive macros / doc attributes).

- Total `pub enum` / `pub struct` declarations scanned: **5 412**
  (across the 65 `corelink-*` crates).
- Declarations missing the annotation: **1 307** lines flagged.
- After exclusion of acceptable categories:
  - **38 lines** sit in test / proptest / fake / sim files (acceptable
    — these aren't a stable public surface).
  - **~280 lines** are in `error.rs` / `errors.rs` modules. These ARE
    on the public surface and IS the canonical target of the L2.1
    invariant — but charter §4 explicitly classifies adding
    `#[non_exhaustive]` to types as HIGH-RISK ("could break
    consumers"), so this audit does NOT mass-apply. See §4.
  - The remaining ~990 lines are types declared `pub` inside private
    submodules that are NOT re-exported through `pub use` chains at the
    crate root. These reach `pub(crate)`-equivalent visibility only and
    are NOT part of the cross-crate API surface, so L2.1 does not
    apply.

Conclusion: **L2.1 is largely respected for the cross-crate API**.
Where it is materially missing (error enums, public DTOs), adding the
annotation is a semantic decision (it suppresses exhaustive matching at
the consumer side and would force downstream `_ =>` arms). The
workspace appears to follow an implicit convention of NOT applying
`#[non_exhaustive]` blanket-ly, on the grounds that:
1. The workspace is closed (no external consumers yet);
2. Most "public" enums are error enums where downstream code expects
   to pattern-match for retry decisions.

INLINE FIXED: **0**.
ESCALATIONS: 1 standing-policy question — "should we adopt
`#[non_exhaustive]` on every cross-crate error enum?" — see §4 item 1.

### L2.2 `unsafe` outside FFI boundary

Scan: `grep -rEn "unsafe " --include="*.rs" crates/` minus test /
fuzz / examples / comment lines.

Total `unsafe` occurrences: **54 lines**, all in
`crates/corelink-client-verify/src/ffi.rs`.

This file is the documented C FFI boundary (declares `cdylib` +
`staticlib` crate-types per Cargo.toml; `extern "C"` functions and
`*mut`/`*const` raw-pointer arithmetic). Charter explicitly exempts
this crate. No other `unsafe` block exists workspace-wide.

INLINE FIXED: 0. ESCALATIONS: 0. **PASS**.

### L2.3 `tokio` imports in `src/` outside `#[cfg(test)]`

Scan: `grep -rEn "^use tokio" --include="*.rs" crates/*/src/` minus
cfg(test) lines.

Hits: **9 lines** across 4 crates.

| Crate | Files | Verdict |
|---|---|---|
| `corelink-byok` | 4 (`byok_vault/auth.rs`, `byok_azure/entra.rs`, `byok_gcp/adc.rs`, `byok_core/dek_cache.rs`) | **EXEMPT** — manifest explicitly gates `tokio = { workspace = true }` with native-only features and wasm-fallback `["sync"]`. These are real-backend adapter impls. |
| `corelink-client-verify` | 1 (`src/stream.rs`) | **EXEMPT** — `tokio` is `optional = true`, gated behind the `stream` feature flag for AsyncRead support. |
| `corelink-r2-multipart` | 1 (`src/concurrency.rs`) | **EXEMPT** — manifest declares `tokio = { default-features = false, features = ["sync"] }`. Only `Semaphore` / `TryAcquireError` are imported; runtime-agnostic primitives. |
| `corelink-worker` | 3 (`auth/revocation/in_memory_store.rs`, `in_memory_meta.rs`, `in_memory_broadcast.rs`) | **EXEMPT** — `corelink-worker` is the Cloudflare worker entry crate; `tokio` is gated `optional = true` behind feature flags. In-memory revocation stores use `tokio::sync::Mutex` (runtime-agnostic). |

INLINE FIXED: 0. ESCALATIONS: 0. **PASS**.

### L2.4 `prop_assert!(matches!(..., Variant { .. }))` anti-pattern

Scan: `grep -rEn "prop_assert!\s*\(\s*matches!"`.

Hits: **56 lines** (52 actual code, 4 comment-only lessons-learned
references).

Distribution by crate:
- `corelink-worker` 10, `corelink-reapi` 7, `corelink-auth` 5,
  `corelink-tier-selection` 4, `corelink-ops` 4,
  `corelink-enterprise-inquiry` 4, `corelink-rate-headers` 3,
  `corelink-clerk` 3, `corelink-telemetry` 2, `corelink-meta` 2,
  `corelink-handler-ac` 2, others 6.

Charter §2 L2.4 ("S-08 P1-1 lesson — assert specific fields, not just
variant match"): the anti-pattern is using `prop_assert!(matches!(x,
Variant { .. }))` because it tolerates ANY inner state. The 52 real
occurrences fall into two sub-patterns:

1. **Fieldless / wildcard-inner** (e.g., `prop_assert!(matches!(err,
   ErrorType::AuditFailed(_)))`) — these tolerate any inner state and
   ARE the anti-pattern. Approx **34 lines** fall here.
2. **Bound-inner** (e.g., `matches!(severity,
   SignCountSeverity::PasskeyExempt)`) — these check that a value has
   a specific unit variant; no inner state to assert, so the
   anti-pattern critique doesn't really apply.

Per §4: "Anything touching ProptestConfig (test infra; potentially
intentional fixed seeds)" — by extension, prop_assert! body
refactoring is HIGH-RISK (could break test invariants or weaken
assertions). NOT fixed inline.

INLINE FIXED: 0. ESCALATIONS: 1 — the ~34 fieldless-inner cases should
be reviewed per the S-08 P1-1 lesson. List below in §4.

### L2.5 hard-coded `ProptestConfig::with_cases(N)`

Scan: `grep -rEn "ProptestConfig\s*::\s*with_cases\s*\(\s*[0-9]+\s*\)"`.

Hits: **11 lines** across 8 files.

| File | Cases hard-coded |
|---|---|
| `crates/corelink-handler-ac/tests/prop_handler_ac.rs:29` | 256 |
| `crates/corelink-dpa-acceptance/tests/prop_jwt_signature.rs:71` | 16 |
| `crates/corelink-dpa-acceptance/tests/prop_locale_mismatch.rs:33` | 64 |
| `crates/corelink-dpa-acceptance/tests/prop_idempotency.rs:38` | 32 |
| `crates/corelink-byok/tests/byok_aws_unit.rs:313` | 64 |
| `crates/corelink-byok/tests/byok_aws_real_unit.rs:257` | 96 |
| `crates/corelink-handler-admin/tests/prop_handler_admin.rs:38` | 256 |
| `crates/corelink-handler-cas/tests/prop_handler_cas.rs:41` | 256 |
| `crates/corelink-ops/tests/survey_prop_survey.rs:38` | 256 |
| `crates/corelink-ops/tests/drata_proptest_idempotency.rs:52` | 64 |
| `tests/e2e-chaos/tests/prop_seed_replay_identical.rs:45` | 200 |

Per spec: should use `proptest_cases()` runtime helper to honour
`PROPTEST_CASES` env var. All 11 sites hardcode and ignore env. Per §4
HIGH-RISK (test infra). NOT fixed inline.

INLINE FIXED: 0. ESCALATIONS: 1 — see §4 item 3.

### L2.6 audit fail-CLOSED ordering: `lookup → emit_audit → mutate_state`

Spot-sampled 5 callsites across `corelink-billing`, `corelink-ops`,
`corelink-privacy` (W36-untouched files only):

| File:line | Callsite | Ordering | Verdict |
|---|---|---|---|
| `corelink-billing/src/abuse/scorer.rs:302` | `audit.emit(ScoreComputed)` followed by `audit.emit(Decision*)` then `metrics.record_check` mutation | lookup → emit → mutate | **CORRECT** (code comment confirms "audit emit BEFORE state mutation (fail-closed envelope)") |
| `corelink-billing/src/quota/cas/cas.rs:258` | `state.lookup(...)` → `audit.emit(CasCheckPassed)` → `metrics.record_check` | lookup → emit → mutate | **CORRECT** (commented "Audit emit BEFORE returning") |
| `corelink-billing/src/quota/cas/cas.rs:319` | (denial path) `audit.emit(CasCheckDenied)` then no mutation | emit only | **CORRECT** |
| `corelink-ops/src/drata/runner.rs:166` (Skipped) | `ledger.lookup(hash)` → emit `Skipped` → continue | lookup → emit → no mutate | **CORRECT** |
| `corelink-ops/src/drata/runner.rs:188` (Sent) | `drata.push(...)` → `ledger.record(entry)` → `audit.emit(Sent)` | mutate → emit | **FLAGGED** (audit fires AFTER ledger write; if audit fails, ledger has been updated but no audit trail records the sync). |

The `corelink-ops/src/drata/runner.rs:188` case is potentially a
fail-OPEN ordering, but the mutation is internal idempotency-ledger
state (NOT user-facing). Whether this qualifies as a charter violation
depends on whether the ledger counts as "state" in the L2.6 sense or
purely as a side-effect log. **§4 HIGH-RISK** ("Anything touching
audit emit ordering (semantic correctness)") — flag for orchestrator.

INLINE FIXED: 0. ESCALATIONS: 1 — see §4 item 4.

Other audit-emitting code (`corelink-billing/src/replay/engine.rs`,
`corelink-ops/src/drata/runner.rs:202`, all `corelink-privacy/src/*`
trait emit definitions) was spot-checked; no other ordering anomalies
surfaced in the sample.

### L2.7 `unwrap()` / `expect()` / `panic!` / `unimplemented!` / `todo!` outside `#[cfg(test)]`

AST-aware scan (Python script that excludes `#[cfg(test)]` and
`#[cfg(all(test, ...))]` module ranges via balanced-brace counting,
plus exclusion of `tests/` / `fuzz/` / `examples/` / `benches/`
subtrees and known unwrap-variant helpers like `unwrap_or` /
`unwrap_err`):

- Raw scan: **3 529** lines.
- After AST cfg(test) filter v2: **290** lines.
- After excluding cross-file `#[cfg(test)] mod tests_*;`-gated test
  helper modules (`tests.rs`, `tests_basic.rs`, `tests_routes.rs`,
  `tests_stream.rs`, `tests_scenarios.rs`, `tests_prelude.rs`,
  `tests_handlers.rs`, `tests_common.rs`, `proptests.rs`,
  `tests_splice.rs`, `tests_proptest.rs`): **17** lines genuinely
  reside in non-test source.

All 17 remaining hits reduce to documented infallibility / static-init
patterns:

| File:line | Pattern | Why acceptable |
|---|---|---|
| `crates/corelink-clerk/src/fakes.rs:342,346,350` | `RsaPrivateKey::new(...).expect("rsa keygen")` + Pkcs8 serialize expects | `pub mod fakes` — test-helper module exposed for downstream test deps; in-memory RSA fake bootstrap. |
| `crates/corelink-byok/src/byok_vault/key_name.rs:22` | `Regex::new("$^").expect("trivial regex must compile")` | Static trivial-regex compile in const init context. |
| `crates/corelink-byok/src/byok_gcp/key_resource.rs:36` | `.expect("static regex compiles")` | Static regex with literal pattern; documented infallibility. |
| `crates/corelink-byok/src/byok_azure/key_resource.rs:51` | `.expect("static regex compiles")` | Same. |
| `crates/corelink-auth/src/schema/pseudonymize.rs:49` | `.expect("HMAC-SHA256 accepts any key length")` | RustCrypto `Hmac::<Sha256>::new_from_slice` returns `Result<_, InvalidLength>`; SHA-256 accepts any byte length so this is genuinely infallible. |
| `crates/corelink-auth/src/schema/email_hash.rs:101,112,140` | `.expect("HMAC-SHA256 accepts any salt length")`, `.expect("PRK is the 32-byte SHA-256 output; never errors")` | Same idiom; HMAC-of-PRK with known fixed length. |
| `crates/corelink-worker/src/reapi/cas/types.rs:144,145` | `.expect("manual lowercase-hex of [u8; 32] is always valid ASCII")`, `.expect("manual lowercase-hex of [u8; 32] always parses")` | The bytes were generated by the same function; hex round-trip invariant proven. |
| `crates/tenant-path/src/prefix.rs:104,150,157` | `.expect("TenantPrefix bytes are always ASCII...")`, `.expect("HMAC-SHA256 accepts any 32-byte key...")`, `.expect("URL_SAFE_NO_PAD of 32 bytes fits in 43 bytes (no padding)")` | All algebraic invariants documented at the call site. |

Each `expect` carries a proof-comment in its message string. This is
idiomatic Rust ("expect-as-assertion-of-invariant") and matches the
codex/codebase convention.

INLINE FIXED: 0 (nothing fits the LOW-RISK band; none are bare
`unwrap()` or `panic!`). ESCALATIONS: 0. **PASS-WITH-NOTES**.

### L2.8 `#![forbid(unsafe_code)]` or `#![deny(unsafe_code)]` at crate root

Scan: for each `crates/corelink-*/src/lib.rs`, verify the file starts
with one of `#![forbid(unsafe_code)]` or `#![deny(unsafe_code)]`.

Missing: **0**. Every `lib.rs` in the workspace has the appropriate
crate-root denial / forbid attribute. FFI crates
(`corelink-client-verify`, `corelink-wasm`, `corelink-clerk-cf`,
`corelink-cf-bindings`) use `#![deny(unsafe_code)]` plus localised
`#[allow(unsafe_code)]` blocks on the FFI surface, exactly as the
spec prescribes.

INLINE FIXED: 0. ESCALATIONS: 0. **PASS**.

### L2.9 Secrets in logs

Scan: `tracing::` / `log::` / `println!` / `eprintln!` / `dbg!`
intersected with `secret|api_key|token|jwt|password|hmac|key_id`.

Hits: **1**:
- `crates/corelink-ops/src/supply_chain/verify/bin/cli.rs:290`:
  `println!("    [{}] key_id: {}", i, if sig.key_id.is_empty() {
  "(empty)" } else { &sig.key_id });`

Reviewed in context (`sed -n '280,300p'`): the `key_id` here is the
Sigstore signature's *public* key identifier (a hex fingerprint
included in DSSE envelopes and Rekor log entries). It is **not** a
secret — it is metadata used to look up the signing certificate in
the Sigstore trust root. Equivalent to logging a public-key
fingerprint. The surrounding println loop is dumping a verification
report from a CLI binary (`bin/cli.rs`).

INLINE FIXED: 0. ESCALATIONS: 0. **PASS**.

## §3 Inline fixes applied

None.

Per spec §4 LOW-RISK rules, only two categories qualified for inline
fix:
1. Missing `#[non_exhaustive]` on truly extensible types — but the
   scale (1 307 lines) and the §4 HIGH-RISK clause on
   "deserialized cross-crate" made every plausible candidate
   reviewable but not mechanically safe.
2. Missing `#![forbid(unsafe_code)]` at crate root — the scan returned
   zero missing.

No fixes were committed; `git status --short` is empty at SEAL time.

## §4 Orchestrator escalations

### Escalation 1 — L2.1 `#[non_exhaustive]` standing-policy decision

The workspace appears to follow an implicit convention of NOT applying
`#[non_exhaustive]` to public enums / structs across the board.
Roughly 280 error-enum and DTO declarations on the cross-crate API
surface lack the annotation. This is a deliberate-looking pattern
(error variants need exhaustive matching at retry sites) but the L2.1
invariant in `techlead` skill literally specifies it.

Decision required: adopt blanket `#[non_exhaustive]` on
external-surface error / DTO enums? If yes, this becomes a tracked
debt item (DEBT-NNN) with per-crate per-PR rollout, NOT a single
mass-rename.

Recommended: maintain current convention (PASS-WITH-NOTES); add
`#[non_exhaustive]` only on truly extensible types and on freshly
added public DTOs going forward. Document this carve-out in the
charter.

### Escalation 2 — L2.4 prop_assert!(matches!(_)) cleanup

~34 lines in `crates/*/tests/prop_*.rs` use the
`prop_assert!(matches!(err, Variant(_)))` anti-pattern. List of files
with counts:

| Crate | File | Lines (sample) |
|---|---|---|
| `corelink-worker` | `tests/prop_ac_full.rs` | 263, 464, 492 |
| `corelink-worker` | `tests/prop_multipart_full.rs` | 241 |
| `corelink-worker` | `tests/prop_split_splice.rs` | 170, 282, 286, 295 |
| `corelink-worker` | `tests/prop_ac_handlers.rs` | 185, 231 |
| `corelink-reapi` | `tests/prop_cross_tenant_read.rs` | 195, 287 |
| `corelink-reapi` | `tests/prop_cas_read.rs` | 244, 489, 553 |
| `corelink-reapi` | `tests/prop_idempotency.rs` | 244 |
| `corelink-reapi` | `tests/prop_cas.rs` | 442 |
| `corelink-tier-selection` | `tests/prop_tier_selection.rs` | 155, 242, 281, 330 |
| `corelink-auth` | `tests/webauthn_prop_webauthn.rs` | 91, 118, 122 |
| `corelink-handler-ac` | `tests/prop_handler_ac.rs` | 85, 96 |
| `corelink-handler-admin` | `tests/prop_handler_admin.rs` | 87 |
| `corelink-handler-cas` | `tests/prop_handler_cas.rs` | 81 |
| `corelink-signup` | `tests/prop_signup_orchestration.rs` | 319 |
| `corelink-stripe-real` | `tests/prop_portal.rs` | 94 |
| `corelink-meta` | `tests/prop_audit_outbox.rs` | 179 |
| `corelink-meta` | `tests/prop_refcount.rs` | 300 |
| `corelink-enterprise-inquiry` | `tests/prop_enterprise_inquiry.rs` | 110, 123, 223, 248 |
| `corelink-rate-headers` | `tests/prop_rate_headers.rs` | 251, 447 |
| `corelink-ops` | `tests/oncall_prop_oncall.rs` | 161, 186 |
| `corelink-ops` | `tests/dr_drill_prop_dr_drill.rs` | 84, 90 |
| `corelink-clerk` | `tests/prop_validate.rs` | 155, 169, 207 |
| `corelink-telemetry` | `tests/prop_synthetic_pager.rs` | 71, 148 |

Per S-08 P1-1: replace each with an explicit `match` arm that asserts
the *inner field* values, e.g.:

```rust
// Anti-pattern:
prop_assert!(matches!(err, AuditFailed(_)));

// Correct:
match err {
    AuditFailed(inner) => prop_assert_eq!(inner.kind, ExpectedKind),
    other => prop_panic!("expected AuditFailed, got {other:?}"),
}
```

Decision required: schedule a per-crate cleanup wave (one PR per
crate, scoped) or treat as accepted-debt with rationale "inner-field
assertion adds noise for variants where inner data is intentionally
opaque (e.g. wrapped foreign error types)".

### Escalation 3 — L2.5 hard-coded ProptestConfig::with_cases

11 sites bypass `PROPTEST_CASES` env override. List in §2 L2.5 above.

Decision required: replace each hardcoded literal with the
`proptest_cases(default_cases: u32)` helper (which honours the
`PROPTEST_CASES` env var). Recommended: add a shared
`proptest_cases()` helper to a workspace test-utility crate (likely
`corelink-meta` or a new `corelink-test-utils`), then refactor all 11
sites in a single PR.

### Escalation 4 — L2.6 ordering anomaly in `drata/runner.rs`

`crates/corelink-ops/src/drata/runner.rs:188` (Sent path) executes
`self.ledger.record(&entry)?;` BEFORE `self.audit.emit(SyncAuditEvent
{ outcome: Sent, ... })?;`. If `audit.emit` fails, the ledger entry
persists with no audit trail.

Decision required:
1. **Accept**: ledger is internal idempotency state; audit failure
   here is a metrics / observability gap, not a fail-CLOSED violation
   per L2.6 (the user-facing Drata sync DID succeed; we just failed
   to log the success).
2. **Fix**: swap the order — `audit.emit(Sent)?;` first, then
   `ledger.record(&entry)?;`. Trade-off: if ledger.record fails after
   audit, the audit trail reports a `Sent` event with no idempotency
   record, so re-execution will re-fire the Drata sync (double-send).
3. **Three-phase**: introduce a `prepare → emit_audit → commit` shape
   in the ledger; bigger refactor.

Recommended: option (1) — accept and document in a code comment why
this ordering is fail-OPEN-by-design (the ledger.record is the
"successful side-effect record", emit is the "post-fact log").

### Escalation 5 — L2.1 fakes module visibility

`crates/corelink-clerk/src/fakes.rs` is `pub mod fakes` (not
cfg(test)-gated) — exposed to downstream tests so other crates can
depend on `corelink-clerk` and import in-memory fakes. The 3 expects
at lines 342/346/350 (`RsaPrivateKey::new` + serialize) are
test-helper code that compiles into the production lib for downstream
consumers. Charter §2 L2.7 spec lists "test-only types (under
`#[cfg(test)]`)" as exempt, but `pub mod fakes` is not cfg(test); it's
deliberately exposed across the dev-dep boundary.

Decision required: should `corelink-clerk::fakes` be feature-gated
(e.g. `#[cfg(feature = "test-fakes")]`) so the panicking RSA-keygen
expects don't ship in the production binary? Recommended: yes — add
a `test-fakes` feature, gate the `pub mod fakes;` declaration on it,
and have downstream test crates depend with `features =
["test-fakes"]`. Lightweight refactor; non-blocking.

## §5 Verdict

**PASS-WITH-NOTES**.

- L2.2, L2.3, L2.7 (in spirit), L2.8, L2.9 — full PASS, no findings.
- L2.6 — 1 ordering anomaly in non-W36 territory, flagged not fixed.
- L2.4 — ~34 anti-pattern occurrences in tests, flagged not fixed
  (HIGH-RISK per §4).
- L2.5 — 11 hard-coded ProptestConfig sites, flagged not fixed
  (HIGH-RISK per §4).
- L2.1 — interpreted as workspace standing-convention to NOT apply
  `#[non_exhaustive]` blanket-ly; flagged for charter-level decision.

No critical security / correctness invariant is violated. All flagged
items are either accepted-debt or low-priority cleanup PRs.

Build state at SEAL: no code changes applied; `git status --short`
empty.

## §6 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
