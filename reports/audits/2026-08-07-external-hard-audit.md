# CoreLink — external hard audit (2026-08-07)

**Scope:** 73 crates · 1,555 Rust files · ~478K LOC (`crates/` + container) · 3,469 commits
(Apr 23 → Aug 7 2026) · 114 CI workflows · 608 distinct crates in `Cargo.lock` · 18 GB on disk.
**Method:** first-hand file reads + gate runs + dependency-graph scans. Four read-only sub-audits
(architecture, security, code quality, testing/CI) then every load-bearing claim re-verified
manually in a deep-dive pass. All file:line references are to the tree at HEAD (`0a5e3349`).

> **Honest caveats.** (1) Some early greps used `rg -r` (which is `--replace`, not "recursive"),
> mangling output; every finding cited here was re-verified with clean commands.
> (2) GitHub API for branch protection returned 403 (private repo, no GHAS/Pro) — that claim rests
> on the repo's own `scripts/pre-merge-gate-check.sh:18` + `CLAUDE.md`.
> (3) This repo is a moving target: the working tree carries uncommitted in-flight remediation work
> (`crates/corelink-container/src/gc_worker/`, pricing pages, admin-ui tests).

---

## Verdict table

| Dimension | Grade | Weight | Weighted |
|---|---|---|---|
| Security | 85 | 20% | 17.0 |
| Architecture | 75 | 15% | 11.3 |
| Code quality | 74 | 15% | 11.1 |
| Testing | 73 | 12% | 8.8 |
| Git / process hygiene | 82 | 5% | 4.1 |
| Docs / knowledge | 74 | 5% | 3.7 |
| Performance & economics | 70 | 7% | 4.9 |
| Product | 62 | 10% | 6.2 |
| Pricing | 55 | 8% | 4.4 |
| CI/CD | 40 | 3% | 1.2 |
| **Overall** | | | **~72** |

---

## Security — 85

### Strengths
- **Structural header-strip trust boundary.** `worker/src/index.ts:526-595` delete-then-sets ~14
  client-forgeable trust headers on every forward arm; the DO pins the tenant into lifecycle state
  (`worker/src/durable_object.ts:333-360`); the container rejects reserved namespaces
  (`crates/corelink-container/src/auth_tenant.rs:35-40`).
- **JWT hardening.** `alg=none`/RS↔HS confusion rejected (`crates/corelink-clerk/src/adapter.rs:286-301`),
  constant-time audience match, exact issuer allowlist, KV-cached JWKS with bounded retry.
- **PAT two-layer.** Worker HMAC-SHA256 fast-fail + container re-runs Argon2id Option-B verify at
  the top of every billable handler (`routes/cas.rs:161-167`).
- **Cryptography.** HMAC-derived tenant namespaces from a zeroizing TDK (`tenant-path/src/prefix.rs:148-165`);
  BLAKE3 content verification gates every CAS read (`storage/r2_s3.rs:985`); `timingSafeEqual` with a
  per-isolate random key (F22 fix); Ed25519 erasure attestation.
- **SQL parameterized everywhere** sampled (Worker `.prepare().bind()` and container `?1` binds).
  Path traversal rejected in the sccache WebDAV key normalizer (`adapter-host/src/cargo/translate.rs:42-66`).
- **RLS is real** (not documentation-only): `migrations/002_auth_tables.sql:314-360` enables ROW LEVEL
  SECURITY on 7 tables with `tenant_isolation_*` policies keyed on `current_setting('app.current_tenant')`.
  What is missing is only a shared Rust helper (ergonomics), not enforcement.
- **Fail-closed defaults** everywhere sampled: missing `native_pat_gate`/`EMAIL_HASH_SALT` in prod is
  `exit(1)` (`main.rs:77-96`); missing internal-auth keys mean routes are NOT mounted; sub-floor
  consumer keys fail CLOSED (`lib/internal_auth.ts:111-150`).
- **Self-audit loop is real.** `reports/audits/2026-07-03-caa360-pre-hn-launch.md` documented a
  CRITICAL (EU erasure falsely signed `VerifiedComplete`) and a HIGH (RBAC role dead). Both are closed
  in the current tree:
  - Erasure CRITICAL → fixed by per-region fan-out with fail-closed semantics
    (`worker/src/index.ts:1970-2030`): every jurisdiction (lhr/sam/nrt/syd) must confirm erase or the
    origin returns 502 → queue retries → no false attestation can be signed. Each regional container
    erases its own jurisdiction's buckets (`wrangler.toml:845-882`), so the single-bucket adapter is
    correct by construction.
  - RBAC HIGH → fixed (`worker/src/lib/clerk_auth.ts:279-326` resolves `role` from `team_member`;
    `x-corelink-role` structurally stripped at `index.ts:588-594`; `e161359f`).
  This is evidence of a leak history that is *acted upon* (F-012, F22, CAA-360), not ignored.

### Residual risks (not systemic holes)
1. Header-strip discipline is not structurally enforced by a test that walks every forward arm — the
   F-012 smuggling incident happened once and could recur. **Highest-value hardening.**
2. `/_health/container` is public + unauthenticated and exposes the `storage` backend (fingerprinting;
   documented intentional).
3. No CSP / `X-Content-Type-Options` on API responses (JSON API, low risk).
4. Secrets are single-copy in `.env.local` (CF secrets are write-only) — acknowledged SPOF.
5. RSA Marvin (`RUSTSEC-2023-0071`) remains an ADR-gated, operator-mitigated residual.
6. The container receives the full secrets envelope at `container.start()` — documented blast radius.

**Why not higher:** the strip discipline is convention-enforced, soft markers (`x-corelink-mfa-verified`,
`x-corelink-storage-quota-bytes`) are trusted by header discipline rather than cryptography, and a
real pentest would pressure-test the forward-arm inventory.

---

## Architecture — 75

### Strengths
- Cryptographically-derived tenant namespace, unforgeable, never customer-controlled.
- Single enforcement point for content-addressing; Bazel shares the verified CAS trait objects
  (`routes/bazel_v2.rs:247`).
- Atomic D1 metadata + audit (batch, `INSERT OR IGNORE`, `RETURNING refcount` pattern)
  (`crates/corelink-meta/src/store.rs`).
- Genuinely wired GC trait surface (`corelink-gc` compiled into container) + boot-time fail-fast.

### Weaknesses / debt
- **BYOK:** real providers ARE implemented and feature-gated (`byok_orchestrator.rs:223-272`); since
  the 2026-08-07 commit the Dockerfile builds `--features byok-aws-real`, so AWS is compiled into the
  prod image. GCP/Azure/Vault exist as mutually-exclusive features but are NOT in the shipping Dockerfile
  (image rebuild required). AWS credentials are not present in `.env.local` (only `AWS_REGION`); their
  presence in prod secrets is unverified. **Two stale docs contradict the build** (`REMEDIATION_PLAN.md:27`
  and `main.rs:927-931` both say the feature is off).
- **Dual-language boundary duplication:** the deployed edge is TypeScript (`worker/src/index.ts`), and
  a parallel Rust model (`crates/corelink-worker`, wasm32-test-only) re-implements tenant derivation +
  auth timing. Two implementations must stay in sync.
- **Canonical facades half-adopted:** the container imports leaf crates directly (`corelink-pat` in 17
  files, `corelink-hash` in 3) and does not depend on the `corelink-auth`/`corelink-cas`/`corelink-crypto`
  umbrellas — the advertised canonical import surface is not the surface the shipped binary uses.
- **Orphaned packages (0 Rust dependents, precise scan):** `corelink-adapters-cloud`, `corelink-adapters-vault`,
  `corelink-signup`, `corelink-transparency-log`, `corelink-wasm`, and the `corelink-privacy` umbrella
  (the container uses the privacy *leaf* crates directly). `corelink-ops` has exactly 1 dependent
  (`corelink-dsr-statuspage-scheduler`), which is itself not in the container — the Ops/chaos/rotation
  plane is effectively disconnected from the shipping binary.
- **Dead code in the tree:** `crates/corelink-container/src/gc_worker/` (11 files) is UNTRACKED and
  undeclared in `lib.rs`/`main.rs` — it never compiles. Only `InMemoryGcWorker`/`InMemoryEvictionPhase`
  are compiled (`REMEDIATION_PLAN.md:9-16` agrees). Violates the repo's own zero-loose-ends mandate.
- **Turbo path deliberately not content-verified** (`corelink-turbo-bridge/src/lib.rs:27-32`): keys are
  opaque, no hash↔bytes check. Defensible (Turbo owns the hash algorithm) but a deliberate CAS-invariant
  carve-out.
- **R2 multipart unshipped:** no `corelink-r2-multipart` dep in the container; `R2_CHUNK_BUCKET` is never
  written (`dsr/adapter_r2_cas.rs` documents the scope guard).
- **Inert cron triggers added 2026-08-07:** `wrangler.toml` now declares GC daily / DSR-verify hourly /
  heartbeat 5-min crons, but the main worker exports NO `scheduled` handler (`worker/src/index.ts` only
  exports a fetch handler); the container has no `/_internal/gc/run` or `/_internal/replication/heartbeat`
  route. Only the signup-worker's hourly cron (which covers DSR verify) is live. Infra enabled before
  implementation exists.

---

## Code quality — 74

### Strengths (near-best-in-class discipline)
- **Workspace-wide deny lints:** `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `todo`,
  `unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`, `mod_module_files` all `deny`
  (`Cargo.toml:412-424`); `missing_debug_implementations = "deny"`.
- **~3 real production panic sites across 367K LOC**, all documented invariants (Argon2 param guard,
  HMAC key-length guards); even a `const fn` workaround exists to obey the lint
  (`worker/middleware/timing_padding/config.rs:102-118`).
- **`unsafe` eliminated:** 85 crates `#![forbid(unsafe_code)]`; the only ~56 sites are scoped to the
  `corelink-client-verify` FFI module (`#![deny(unsafe_code)]` at crate root, localized `allow`).
- Universal `thiserror`, `#[non_exhaustive]` typed error taxonomies; poison-recovery and fail-closed
  concurrency patterns; newtypes for security-critical IDs (`Digest`, `TenantPrefix`, `TenantId`).

### Weaknesses (structural, not cosmetic)
1. **God files — AND the repo's own rule (L2.10) forbids them, unenforced.** The charter alignment
   in `specs/_proposals/adapters/README.md:141` declares: *"L2.10: every file ≤500 LOC HARD CAP
   (per-file, not per-crate; adapters may be 1000-2000 LOC distributed across files)"* — sitting
   alongside charter rules that ARE enforced (`#![forbid(unsafe_code)]`, `#[non_exhaustive]`). The
   OKF/ZTK corpus codified it as concept `corelink-loc-cap-l210`: *"any `.rs` file must not exceed
   500 LOC (hard cap); ideal ≤200 LOC/file."* All 5 adapter specs carry it as an **unchecked**
   acceptance criterion (`- [ ] L2.10` in `brew.md:166`, `cargo.md:233`, `npm.md:258`, `pip.md:169`,
   `oci.md:209`). **There is zero enforcement:** no `too_many_lines` clippy lint (the full
   `[workspace.lints.clippy]` block has none), no `clippy.toml`, no script, no CI gate, no hook.
   Actual state: **247 `.rs` files exceed 500 LOC** (58 over 1,000; 8 over 2,500). The container
   crate is ~24% of the workspace with 5 of the top-10 files over 2,500 lines:
   - `customer_d1.rs` 4,535 (**9.07× the hard cap**) · `storage/r2_s3.rs` 4,087 (8.2×) · `routes/cas.rs`
     3,468 (6.9×) · `routes/customer.rs` 3,193 · `routes/oci.rs` 2,735 · `routes/turbo_v8.rs` 2,719 ·
     `routes/cas_erase.rs` 2,533 · `tenant_quota.rs` 2,444 · `byte_accounting.rs` 2,414 ·
     `routes/bazel_v2.rs` 2,368.
   - **Even the adapters the rule was written FOR violate it:** `corelink-adapter-host` has 6 files
     over 500 (`oci/server/handlers.rs` 712, `cargo/server.rs` 633, `npm/metadata.rs` 572,
     `oci/auth.rs` 562, `npm/tarball.rs` 561, `oci/bridge.rs` 537). The rule was declared, never
     enforced, and never marked satisfied — a written hard cap with 247 violations.
2. **Copy-paste across route files:** `pat_gate_reject`/`pat_gate_reject_write` duplicated in ~7 route
   files; `clamp_limit`/`DEFAULT_LIST_LIMIT`/`MAX_LIST_LIMIT` identical in `cas.rs` and `ac.rs`;
   `TENANT_SENTINELS` 3× in `turbo_v8.rs`; no shared `routes::common`.
3. **Dead, non-compiled tree:** `gc_worker/` (11 files) untracked + undeclared.
4. **Dependency sprawl:** 36 packages at multiple versions (hashbrown ×5, getrandom/rand/rand_core ×3,
   curve25519-dalek ×2, digest ×2, http ×2 …), 874 KB `Cargo.lock`, 608 distinct crates — crypto-migration sprawl.
5. **Clone churn:** 3,879 `.clone()` across the workspace (868 in the container alone).
6. **Mirror/tautology tests on the money path:** `tier_select_checkout.rs:232-233`
   (`let x: Option<&str> = None; assert!(x.is_none())`) and a mirror of the match arm — see Testing.
7. **SDK versioning inconsistency:** Go `/v1` (semver-stable) vs JS `0.1.0` vs Python `0.1.0a1`.
8. **Docs-as-substitute:** every error variant re-documented; `#[allow(dead_code)]` rare but present.

---

## Testing — 73

### Strengths (real, not theater)
- **Property tests with the actual product invariant:** `corelink-cas/tests/dedup_prop.rs:96-120` pins
  `find_missing_blobs(T, Q) = Q ∖ present_for_T` + tenant isolation under proptest (10k iterations).
- **Mutation testing loop closed:** per-diff mutation gate on Rust PRs (`mutation-pr.yml`), nightly
  kill-rate ≥75% gate with a harness-failure discriminator; `mutation_kills.rs` suites pin exact mutants.
- **Adversarial tenant isolation:** 25 scenarios (cache poisoning, CMK half-state, PAT revoke ToCToU,
  Stripe cross-account replay…) all asserting audit-emitted-before-rejection.
- **Billing webhook negative paths:** tampered signature, skew, idempotent redelivery, audit-failure
  aborts with no state mutation (6 of 7 negative).
- Real libFuzzer targets with AAD-mismatch rejection checks; hourly prod CAS canary
  (`cas-canary.yml`) — a live authenticated BLAKE3 round-trip from a datacenter IP.

### Weaknesses
- **The money path is unautomated:** `StripeCheckoutCreator::create` (network) has zero automated
  tests; the footer says "WP-B live verification is MANUAL — deliberately no automated `#[ignore]`
  harness." The two tests in the file are a tautology and a mirror.
- **"e2e" suites run against in-memory fakes** (`tests/e2e-tenant-isolation/tests/adversarial.rs:393`
  uses `fakes::D1Store`) — contract shape, not the deployed R2/D1 stack.
- **~25 `#[ignore]` tests** (live D1/Neon/Stripe) never run in CI.
- **Trivia inflates the headline count:** ~92 `default*`, `debug_impls`, `re_exports_compile` tests.
- **Conformance dir is 1 file / 4 vectors.**
- **Flake risk:** 20-60 ms sleep-based timing tests; heavy proptest counts on shared runners.

---

## CI/CD — 40

- **Branch protection `required checks = []`** — the merge gate is a human running
  `scripts/pre-merge-gate-check.sh` (repo's own admission, `:18`).
- **CodeQL: documented no-op** (`codeql.yml:26-31` — GHAS not enabled, "performs ZERO static analysis").
- **Coverage: no threshold, zero successful scheduled runs in July** (`coverage.yml:27-29`).
- **Fuzzing: parked** after 30 consecutive build failures on the Mac (`fuzz-nightly.yml:13-27`).
- **Load tests: schedule disabled** (no Linux runner on fleet) (`load-test-nightly.yml`).
- **semgrep: cron-only** (`semgrep.yml:40-42`) despite a header that still claims a PR lane.
- **51 workflows ride the founder's Mac** (self-hosted `corelink-builder`) — the box has crashed under
  CI bursts, had its rustup shim deleted by a workflow, and seen rust-cache prune a sibling's registry.
- **Still real:** SHA-pinned actions, toolchain-pin assertion (`ci-assert-pinned-toolchain.sh`), real PR
  lanes with clippy `-D warnings` + tests for the Rust crates, per-diff mutation gate, criterion perf
  regression with committed baselines (`perf-regression.yml`), honest self-documentation of dead lanes.

---

## Product — 62

- **Right positioning:** a multi-tenant CAS with governance as the wedge, build-cache as ONE protocol
  surface; compliance framing (BYOK, residency honesty, re-derivable audit log) maps to the regulated
  SMB ICP. The R2 zero-egress moat + network-effect CAS is a genuinely defensible economic story.
- **Dogfooding is real:** sccache against CoreLink's own `/cargo` surface on its own CI (2026-08-03) —
  using the product on the workload that hurts most.
- **But:** launch unexecuted (checklist DRAFT, engineering gate pending), marketing describes
  enterprise/compliance features as shipped while the remediation plan (dated the audit day) lists six
  P0–P2 gaps with only Phase 0+1 (infra) landed; and the README's "No code debt" is falsified by the
  dead `gc_worker/` tree and the inert crons.

## Pricing — 55

See justification below. Sound unit economics; un-frozen, un-verified, multi-ladder pricing.

## Performance & economics — 70

- Real criterion benches with committed baselines + per-bench tolerance (5% p99 critical / 15% non).
- Bounded batches, in-flight caps, per-tenant token buckets, 10 MiB body cap — hot-path safety is real.
- **But** load testing is parked, fuzzing dead, and the COGS model's central assumption (avg blob
  256 KB → 4,096 ops/GB) has no validated 30-day prod telemetry yet.

## Git / process hygiene — 82

- DCO (`Signed-off-by`), changelog gate, 52 tags, PR-based merges, disciplined commit messages with
  genuinely useful context, `Co-Authored-By` convention.
- Deductions: 874 KB changelog (verbose to the point of being a tax), one-author repo (89% single name,
  agents committing under it — fine for a solo founder), stale gate numbers (see Docs).

## Docs / knowledge — 74

- The 160-concept OKF wiki with per-concept `source_files` + checkpoint anti-drift is genuinely novel
  engineering discipline.
- Deductions: `CLAUDE.md` gate claim **463/0 is stale** (current: 469 schema + 11 YAML-only = 480);
  pricing docs and the remediation plan drift against the build (BYOK).

---

## Pricing grade — explained (55)

**Why not higher — the substance:**

1. **No committed authoritative price book.** Prices resolve at runtime from `STRIPE_PRICE_ID_*` env
   vars (`crates/corelink-stripe-real/src/client.rs`, `let price_env = format!("STRIPE_PRICE_ID_{}",
   tier.as_str().to_uppercase())`). Nothing in the repo can be checked against what a customer is
   actually charged; the public pricing page is `draft: true` (`apps/docs/docs/explanation/pricing/index.mdx:5`)
   and `last_updated: 2026-06-09`.
2. **Three inconsistent ladders coexist.** (a) Core rate card: Solo $15 / Starter $35 / Pro $50 / Max
   $149; (b) the worksheet's worked P&L + Excel block pinned to the superseded $29 / $199 / $599 card
   (self-declared "do not cite the dollar figures below as current"); (c) the expansion doc's runner
   ladder: Solo $30 / Team $120 — same tier names, different prices, adjacent products.
3. **Five Runner SKUs are priced nowhere.** `TierKind::{RunnerStarter,RunnerPro,RunnerTeam,RunnerScale,
   RunnerMax}` exist in `crates/corelink-tier-selection/src/tier.rs` (11 tier kinds) and wire to
   `STRIPE_PRICE_ID_RUNNER_*`, but no marketing or worksheet row prices them.
4. **BYOK — the compliance wedge the whole ICP is built on — is gated to Max ($149 + $99 add-on) and
   Enterprise**, while Pro ($50) is pitched as "best value." The customer you're selling to (regulated
   orgs needing BYOK) cannot buy BYOK on the tier you recommend.
5. **Flat-fee hard-caps monetize the moat poorly.** The network effect (fuller cache → faster + cheaper)
   never appears on the bill; heavy users hit a 429 and are told to upgrade — the upsell is a rejection,
   not a conversion, which is a churn risk, not an expansion engine.
6. **Free tier is an unbounded loss line** (~$0.74/tenant/mo; 10k tenants ≈ $7.4k/mo) that grows with
   success.
7. **COGS rests on an unvalidated assumption** (avg 256 KB blob → 4,096 ops/GB), with the "Finance
   re-validate against 30-day prod data" step still open.

**Why not lower:** R2 zero-egress is a real structural edge; the COGS math is honest (78-85% modeled
gross margin); hard-cap-without-silent-overage is the trust-first right call; seats correctly not
per-seat-priced; Free/Enterprise routing (instant vs inquiry) is sensible.

---

## Top actionable findings (in order of value)

1. **Automate the Stripe Checkout path** (highest-value test gap; delete the tautology).
2. **Wire or delete `gc_worker/`** — it's uncommitted dead code today; commit it as in-progress or
   remove it until it compiles.
3. **Resolve the inert crons:** the main worker needs a `scheduled` handler and the container needs
   `/_internal/gc/run` + heartbeat routes, or the 2026-08-07 triggers do nothing.
4. **Freeze one price book in the repo** (delete the superseded P&L block; price the Runner SKUs;
   resolve the $15 vs $30 Solo collision) and get Finance sign-off before invoice #1.
5. **Enable GHAS or delete the CodeQL no-op**; add a real coverage threshold; move the fuzz/load lanes
   to hosted runners.
6. **Structurally enforce header-strip:** a Worker test that walks every forward arm and asserts
   strip-then-set (closes the F-012 class).
7. **Update the stale docs** that contradict the build (BYOK in `main.rs` + `REMEDIATION_PLAN.md`,
   `CLAUDE.md` gate numbers).
8. **Enforce L2.10 or rescind it.** The ≤500 LOC hard cap exists in the charter (`adapters/README.md:141`)
   and is violated 247× (9× in `customer_d1.rs`). Either add a real gate (clippy `too_many_lines` per
   crate, or a `wc -l` CI check) and carve `corelink-container` up — or delete the rule. A charter
   rule with zero enforcement is worse than no rule: it trains everyone to ignore the charter.
