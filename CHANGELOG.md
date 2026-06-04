# Changelog

All notable changes to CoreLink will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Sprint-tagged sections below mirror the 21-sprint spec-corpus → impl-sealed
trajectory (S-00 through S-20 + the `ga-engineering-gate-complete` cutover
on 2026-05-14). Each post-S-13 sprint receives its own dated section; the
S-00 → S-13 spec-corpus phase is collapsed under `[0.x]`.

Each entry cross-references:

- **WI-S__-___** — sprint work items (see `specs/04_sprints/S__/`)
- **CAP-_____** — capabilities (declared in `_spec_contract.md` §4)
- **R1-9!** / **R2-__** — security findings closed (see `ROADMAP-TO-GA.md`)
- **P0/P1** audit-doc IDs — sprint-close adversarial review findings
  (see `specs/04_sprints/S__/_audits/sprint-close-round-*.md`)

---

## [Unreleased]

### Added

- **Self-serve tier-select checkout backend — ironclad core** (WI-S19-004 PRR
  wiring). `crates/corelink-container/src/routes/tier_select.rs`: the
  transport-agnostic, fail-CLOSED core of `POST /v1/onboarding/tier-select`.
  Constant-time internal-auth gate + edge-verified `x-corelink-tenant-id` only
  (no client-supplied tenant) + the durable orchestration behind
  `TierSelectStore` / `CheckoutCreator` / `TierSelectAudit` trait seams:
  audit-before-mutate; INV-ONBOARD-DPA-FIRST (Stripe is never called without DPA,
  proven by spy); durable 60s lock → 409; UNIQUE active subscription → 409;
  Stripe failure → 502 + lock release; free → instant. 17 adversarial tests.
  Production D1/Stripe adapters + the route mount land in a follow-up.
- Customer-facing CHANGELOG generation tooling (`scripts/generate-changelog.sh`)
  and PR-level enforcement workflow (`.github/workflows/changelog-validate.yml`).
- Customer-facing **release-notes auto-generator** (`scripts/generate-release-notes.py`)
  with `--from / --to / --dry-run` flags, tag-trigger CI
  (`.github/workflows/release-notes.yml`), polish template
  (`releases/TEMPLATE.md`), and operator editorial guide
  (`marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md`). On every `v*` tag
  push, CI generates `releases/RELEASE-<version>.md`, opens an editorial PR,
  and creates a draft GitHub Release.
- **Wave-36 Trigger A** — new leaf crate `corelink-billing-stripe-traits`
  (4 traits + 9 types) extracted to break the
  `corelink-stripe-real ↔ corelink-billing-materializer` dep-graph cycle
  surfaced during Wave-33 stage-2.C consumer migration.
  Tag `wave-36-final-sealed`. SEAL:
  `specs/_audits/sealed/2026-05-27-w36-trigger-a-seal.md`.
- **Wave-36 proptest follow-ups** — +14 proptests across three adapter
  crates: 6 in `corelink-wasm` (`put` / `get` / `stat` JS/TS surface),
  4 in `corelink-clerk-cf`, 4 in `corelink-statuspage-real`. SEALs:
  `specs/_audits/sealed/2026-05-26-w36-proptest-wasm-seal.md`,
  `specs/_audits/sealed/2026-05-26-w36-proptest-fu-002-seal.md`.

### Changed

- **Wave-35 Phase-2 absorption campaign** — 9 absorption SEALs collapsed
  the bulk of Wave-33 satellite crates into umbrella canonical paths
  (CAS, TELEMETRY, ADAPTER-HOST, REPLICATION, BILLING, PRIVACY, OPS, AC,
  BYOK). Workspace `members` reduced wave-over-wave per each per-umbrella
  SEAL audit. Tag `wave-35-phase-2-sealed`. SEAL audits live at
  `specs/_audits/sealed/2026-05-26-w35-p2-{cas,telemetry,adapter-host,
  replication,billing,privacy,ops,ac,byok}-absorption.md`.
- **Wave-36 Stage 2.C** — 5 consumer migration sites flipped from
  absorbed-adapter direct imports to the canonical umbrella paths
  (`corelink_billing::stripe::real::*` and peers). SEAL:
  `specs/_audits/sealed/2026-05-26-w36-stage2c-closure.md`. Tag
  `wave-36-stage-2-sealed`.

### Deprecated

- (none)

### Removed

- (none)

### Fixed

- **Complete Wave-35 rename-rot sweep — stale `cargo --test/--bench` target
  names in CI workflows and runbook scripts.** Wave-35 absorption prefixed test
  and bench filenames with a module name (e.g. `tests/prop_quota.rs` ->
  `tests/quota_core_prop_quota.rs`), but 40+ `cargo test --test <X>` /
  `cargo bench --bench <X>` invocations across `.github/` and `scripts/` still
  used the old basenames, causing `error: no test target named <X>` (exit 101)
  in ~1.5 s. PR #126 fixed 9; this sweep closes the remaining 13 distinct stale
  refs (30+ invocation sites) across 8 files:
  `nightly.yml` (8 refs), `region_pinning.yml` (3 refs),
  `d1-migration-validate.yml` (1 ref + comment),
  `scripts/rb_fm_250_dry_run.sh` (5 refs incl. package rename
  `corelink-edge` -> `corelink-cas`, `corelink-quota-cas` -> `corelink-billing`,
  `corelink-abuse` -> `corelink-billing`),
  `scripts/rb_fm_059_dry_run.sh` (4 refs, `corelink-quota` -> `corelink-billing`),
  `scripts/rb_billing_001_replay_forensic_dry_run.sh` (5 refs,
  `corelink-billing-replay` -> `corelink-billing`),
  `scripts/rb_fm_302_billing_drift_dry_run.sh` (1 ref),
  `scripts/rb_region_leak_dry_run.sh` (4 refs,
  `corelink-privacy-residency-enforcement` -> `corelink-privacy`).
  Old -> new mapping: `prop_dedup` -> `dedup_prop`, `prop_quota` ->
  `quota_core_prop_quota`, `prop_lru` -> `lru_tracker_prop`, `prop_edge` ->
  `edge_prop`, `prop_quota_cas` -> `quota_cas_prop_quota_cas`, `prop_abuse` ->
  `abuse_prop_abuse`, `prop_quota_fsm` -> `quota_fsm_prop_quota_fsm`,
  `prop_billing_replay` -> `replay_prop_billing_replay`, `calibration_abuse` ->
  `abuse_calibration_abuse`, `migration_canonical_0011` ->
  `edge_migration_canonical_0011`, `d1_migration_integration` ->
  `migrations_d1_migration_integration`, `property_region_pinning_30k` ->
  `residency_property_region_pinning_30k`, `region_adversarial` ->
  `residency_region_adversarial`. Zero stale refs remain; all 51 active
  test/bench target refs validated against `find crates -path '*/tests/*.rs'`
  and `find crates -path '*/benches/*.rs'`. Supersedes #123 + #126.

- **`spec-validation` CI gate — `check_cost_regression.py` now resolves sealed
  sprint contracts.** S07 and S09 are real HIGH_RISK hot-path sprints (eviction +
  audit/metrics) that were sealed and moved to `specs/04_sprints/_sealed/`; their
  `_spec_contract.md` files already document the §14.10 cost regression gate.  The
  script was only checking the active path `specs/04_sprints/<sprint>/` and therefore
  reported "not found" for both, making the gate fail.  Added `find_sprint_contract()`
  which checks the active path first then falls back to `_sealed/<sprint>/`; REQUIRED
  set is unchanged (`S07 S08 S09 S10 S14`).

- **`TLA+ Model Check — Runbooks` gate — robust `tlc`-wrapper build on the macOS
  fleet.** `.github/workflows/tla_runbooks_check.yml` built its `tlc` wrapper with
  a quoted heredoc and then injected `$RUNNER_TEMP` via `sed`; the runner path's
  `/` slashes collided with sed's substitution syntax (`sed: extra characters at
  the end of g command`), failing the step before any TLC run. Dropped `sed`
  entirely and now build the wrapper with an unquoted heredoc (shell expands
  `$RUNNER_TEMP` at write-time, `\$@` stays literal), mirroring the working
  `tla_check.yml` / `tla_billing_check.yml` pattern. SHA-256 pin + run logic
  unchanged.
- **`terraform validate` — sensitive `for_each` in `cloudflare-secrets`
  (sealed the secret-leak vector).** With the `terraform-lint` gate finally
  *running* validate (after the `setup-terraform` SHA repin above), the
  staging env failed `Error: Invalid for_each argument` at
  `infra/terraform/modules/cloudflare-secrets/main.tf:53` — `for_each =
  var.secrets` fed the **sensitive** `map(string)` (secret name → value)
  straight into `for_each`, which Terraform forbids because the resulting
  resource-instance keys surface in plan output and could expose the secret.
  Fixed the SOTA-secure way: iterate over the **names only** via
  `for_each = nonsensitive(toset(keys(var.secrets)))` (names are not
  sensitive; verified via `terraform console` to render as the plain key set)
  and look the still-**sensitive** value up *by key* inside the resource —
  `sha256(var.secrets[each.key])` for the re-trigger hash and
  `SECRET_VALUE = var.secrets[each.key]` for the local-exec env (both confirmed
  to render as `(sensitive value)`). The plaintext secret VALUES therefore
  never enter `for_each` / instance keys / plan output. `terraform validate`
  on `environments/staging` (the gate target, TF 1.7.5) now exits 0;
  `terraform fmt -check` stays clean. (`cloudflare-base`'s `for_each` over the
  non-sensitive `dkim_records` map was already valid — untouched.)
- **Owner-key-gated CI gates now pass-or-skip cleanly pre-launch instead of
  falsely red.** Four checks were reddening on owner secrets / a paid feature
  that intentionally do not exist before launch; each is now gated gracefully
  and reactivates the instant the owner provides the key/feature — no test was
  weakened. (1) **CodeQL** (`codeql.yml`): the SARIF upload to the Security tab
  requires **GitHub Advanced Security** (code scanning), which is not enabled on
  this private repo, so the upload returned "Code scanning is not enabled for
  this repository" and reddened the whole job even though the scan passed. Split
  the upload out of `analyze` (now `upload: never`, writing SARIF locally for the
  severity gate) into a dedicated `github/codeql-action/upload-sarif` step marked
  `continue-on-error: true` (`if: always()`) — mirroring the existing tfsec
  pattern in `terraform-lint.yml`. The job status now tracks the actual scan +
  severity gate; once GHAS is enabled the SARIF populates the Security tab with
  no further change. (2) Same one-line `continue-on-error: true` on the
  `upload-sarif` step in **`semgrep.yml`** (its scan already drives pass/fail via
  its own exit code). (3) **`e2e-clerk-signup.yml`** + (4) **`e2e-stripe-checkout.yml`**
  hit LIVE Clerk/Stripe prod via launch-day secrets (`CLERK_SECRET_KEY`;
  `STRIPE_SECRET_KEY` + `STRIPE_WEBHOOK_SECRET`) that are unset pre-launch — added
  a tiny `gate` job that routes the secret(s) through `env` (the `secrets` context
  is not usable in a job-level `if:`) and emits a `run` output; the `e2e` job now
  `needs: gate` + `if: needs.gate.outputs.run == 'true'`, so it **skips** (not
  fails) when the key is absent and runs once set (mirrors the needs-output gate
  in `pentest-findings-sync.yml`). **`smoke-install.yml`**'s existing Docker
  preflight was extended to also skip (green, with a `::notice::`) when its repo
  secret `CORELINK_TEST_TOKEN_CI` is unset. All five workflows `actionlint`-clean.
  GHAS (paid) + the live Clerk/Stripe keys remain the owner's to provide for full
  coverage.
- **TLA+ `billing_atomicity` — fixed the `AggregateCounter` partial-function
  crash + the two unsound strict-equality invariants it was masking (real TLC
  violations, launch-critical billing).** TLC v1.8.0 crashed at State 4 with
  `Attempted to apply function: <<>> to argument <<t1, sku1, p1>>, which is
  not in the domain of the function`. Three findings, all **spec-modeling
  bugs, NOT billing-code bugs** (the production `corelink-billing-aggregator`
  is an UPSERT-safe ledger — `INSERT … ON CONFLICT … DO UPDATE`, row-absent →
  `Inserted` via the `else` branch of `if let Some(prior) = rows.get(&key)` —
  and `corelink-billing-reconcile/src/drift.rs` reconciles the three layers
  with a bounded relative-error drift ladder, i.e. transient inter-layer drift
  is expected, not a defect):
  1. **Function-domain crash** (the reported violation). `AggregateCounter` and
     `GenerateInvoice` gated a counter/line-item read with a `\/` disjunct
     (`key \notin DOMAIN f \/ f[key] # bucket`). TLA+ `\/` is **commutative**,
     so TLC may evaluate the second disjunct even when the domain check is TRUE,
     applying the empty function `<<>>` (the `Init` state of `counters` /
     `invoice_line_items`) to a key outside its domain. Fix: the lazily-
     evaluated `IF key \in DOMAIN f THEN f[key] # bucket ELSE TRUE` (the idiom
     the `counters'` / `invoice_line_items'` writes already used).
  2. **`INV_BILLING_CHAIN_INTEGRITY` was unsound** (surfaced once the crash was
     gone — the abort had masked it). It asserted `invoice_line_items[k] =
     counters[k]` as a per-state invariant, but the invoice line item is a
     **snapshot** taken by `GenerateInvoice` while the aggregator legitimately
     keeps advancing `counters[k]` as later events drain into R2. Corrected to
     the sound, intent-preserving **monotone containment**
     `invoice_line_items[k] \subseteq counters[k]` (every invoiced event is a
     real aggregated event — no phantom / over-billing) + an explicit
     `k \in DOMAIN counters` guard.
  3. **`INV_LAYER_1_RECONCILE` had the same flaw** at the Stripe layer: strict
     `Cardinality(stripe_invoiced[k]) = Cardinality(live R2 bucket)` breaks the
     instant an event drains to R2 after the (frozen, idempotent) Stripe charge.
     Corrected to the sound zero-**over**-drift bound `<=` (Stripe is never
     billed for more events than physically exist in R2 — the load-bearing
     financial tooth; transient under-count is the drift the production
     reconciler handles).
  No invariant was weakened to pass: the strict-equality forms were genuinely
  **unsound** for this async emit→aggregate→invoice→Stripe pipeline (a frozen
  snapshot can never equal a still-growing live set every instant); the
  containment/`<=` forms are the precise atomicity guarantees (no loss / no dup /
  no phantom-billing), with no-loss + no-dup still pinned by
  `INV_BILLING_NO_LOSS` + `INV_BILLING_NO_DUP`. Re-verified with the pinned TLC
  (`scripts/run_tlc_corelink.sh billing_atomicity`, SHA `237332bd…`): the
  State-4 crash is gone and the bounded model graph explores **past the
  previously-failing depth with zero invariant violations**. Spec-only change
  (`specs/tla/billing_atomicity.tla`).
- **Terraform CI cluster — un-broke the whole `terraform-lint` gate.** Three
  tangled fixes landed together: (1) native `tfsec` (the Docker action is
  Linux-only) with the one real finding (BYOK aws-kms `kms:ReEncrypt*` wildcard,
  scoped to customer ARNs) justified-ignored + flagged for review; (2) resolved
  the pre-existing `tflint` warnings (unused decls, missing provider/version
  constraints) across the terraform modules; (3) repinned the **non-existent**
  `hashicorp/setup-terraform@e9ce11f7` (the `# v3.0.0` SHA 404s — broke
  `terraform fmt`/`validate` at "Set up job") to the real `@b9cd54a3` (v3.1.2).
  Plus a `docker info` preflight guard on `smoke-install`. (Consolidates #78 + #83.)
- **Key Python CI validators → system `python3` (setup-python is unprovisionable
  on the mac fleet).** Five validation workflows — `spec_validation`,
  `openapi-validate`, `canonical-consistency`, `dashboard_validation`,
  `api-deprecation-check` — used `actions/setup-python`, which hard-failed
  fleet-wide with `mkdir: /Users/runner: Permission denied`: on the self-hosted
  macOS runners `RUNNER_TOOL_CACHE` is unset, so the action falls back to the
  GitHub-hosted default `/Users/runner` tool-cache path, which is unwritable and
  not overridable without sudo. Every one of these jobs died at the *Set up
  Python* step before reaching its `python3 scripts/*.py` payload. Dropped the
  `setup-python` step from each and run on the system `python3` (3.14.5, present
  on every runner) — the same fix proven green on `main` since #75 for
  `action-sha-audit` + `secrets-drift`. The three workflows that `pip install`
  deps (`spec_validation` → `requirements-ci.txt`; `openapi-validate` →
  `pyyaml`/`openapi-spec-validator`; `api-deprecation-check` → `pyyaml`) now
  isolate those deps in a repo-local venv (system Python is externally-managed)
  and prepend the venv `bin` to `$GITHUB_PATH` so subsequent steps resolve it;
  the two stdlib-only workflows (`canonical-consistency`, `dashboard_validation`)
  just drop the step. `python3 scripts/validate_specs.py` → 463/0 on the system
  interpreter; all five `actionlint` clean.
- **`audit-chain-daily-verify` 7-day window → portable date math
  (BSD/macOS `date`).** The `Compute 7-day window date set` step used
  GNU-only `date -u -d "$i days ago"`; BSD `date` on the all-macOS
  self-hosted fleet rejects `-d` (`date: illegal option -- d`), and under
  `set -euo pipefail` that hard-failed the `seven-day-verify` job before any
  R2 list/verify ran. Replaced the GNU-date loop with system `python3`
  (`datetime` + UTC), emitting the identical descending `YYYY-MM-DD` set
  (today … today-6) the downstream paginated CF-API-v4 R2 walk consumes
  unchanged. Date computation only — the audit-chain verification logic is
  untouched. (`.github/workflows/audit-chain-daily-verify.yml`.)
- **`ffi-matrix-ci` pyo3 build → system python3 (≥ 3.10).** The Rust
  unit-test job (`rust-unit`, plus `cross-language-verify`) in
  `.github/workflows/ffi-matrix-ci.yml` compiles `corelink-py`, which pulls
  `pyo3` with `abi3-py310` and therefore requires an interpreter ≥ 3.10. The
  job had no working Python pin (setup-python is broken on the self-hosted
  fleet — it provisions into an unwritable `/Users/runner`), so pyo3
  auto-detected a stale 3.8 on PATH and failed with `cannot set a minimum
  Python version 3.10 higher than the interpreter version 3.8 (abi3-py310)`.
  Set `PYO3_PYTHON: python3` at job level so pyo3 builds against the fleet's
  system `python3` (3.14), which satisfies the abi3-py310 floor. FFI matrix
  logic unchanged.
- **Bazel starter cold build → add missing `rules_cc` bzlmod dep** (WI-S15-002).
  `examples/bazel-starter` declared `rules_cc` only via `http_archive` in
  `WORKSPACE`, but modern Bazel (Bazelisk's default, no `.bazelversion`) runs in
  Bzlmod mode and does not load `WORKSPACE`, so the `cc_binary`/`cc_library` loads
  from `@rules_cc//cc:defs.bzl` failed with
  `@rules_cc could not be resolved: No repository visible as '@rules_cc'`, breaking
  the customer-facing `bazel-starter-ci` quickstart gate. Added a `MODULE.bazel`
  with `bazel_dep(name = "rules_cc", version = "0.0.17")` (the WORKSPACE
  `http_archive` is kept only as a legacy `--enable_workspace` fallback).
- **Phantom-Linux-runner nightlies → `workflow_dispatch`-only.**
  `endurance-2h-nightly` and `load-test-nightly` are pinned to
  `runs-on: [self-hosted, Linux, X64]`, but the self-hosted fleet is all-macOS
  (zero Linux runners), so their `schedule:` crons queued forever — perpetually
  pending / red, never executing. Dropped the `schedule` trigger from both
  (kept `workflow_dispatch:` so the k6 endurance/load suites can still be run on
  demand). The `runs-on` pin is intentionally unchanged — these k6 suites are
  too heavy for the macOS builder fleet (the Mac *is* the fleet). Re-add the
  nightly `schedule` once a Linux self-hosted runner is registered.
- **`corelink-reapi` mutation-coverage gap — `http_read` auth extractors.**
  `cargo mutants -p corelink-reapi` reported surviving mutants in
  `crates/corelink-reapi/src/http_read.rs`: the `extract_bearer_http` token-slice
  arithmetic and both `extract_request_id_http` return-value mutants
  (`String::new()` / `"xyzzy".into()`) were never asserted. Added four targeted
  unit tests that pin the EXACT extracted bearer token (incl. scheme/token
  boundary + interior-space tokens) and the EXACT echoed `x-request-id` (present
  case) plus the minted-UUID invariant (absent case), so each killable mutant now
  changes asserted output and fails. Tests only — no production-logic change.
- **`admin-ui lighthouse` gate — probed 404 routes (`/en`, `/onboarding/tenant`).**
  `apps/admin-ui/lighthouserc.cjs` collected `http://localhost:3000/en` and
  `/en/onboarding/tenant`, both of which 404 (`ERRORED_DOCUMENT_REQUEST`), failing
  the gate on every `apps/admin-ui/**` PR. The admin-ui serves its homepage
  un-prefixed at `/` (`src/app/page.tsx`; locale is resolved per-request in the
  root layout — the `[locale]` segment has **no** root `page.tsx`, so `/en` itself
  has never been a route), and the legacy `/onboarding/tenant` wizard was collapsed
  into a `/welcome` redirect by the Phase-0 PLG change. Re-pointed the URL list at
  four routes that actually return 200 — `/`, `/en/privacy`, `/en/consent/new`,
  `/en/admin/audit` — matching the S-16 §6 DoD canonical set. Config-only; no
  workflow or app change. (WI-S16-007 deliverable 2.)
- **`dtolnay/rust-toolchain` ↔ host rustup `bin/cargo` conflict on the
  self-hosted Macs.** The cargo-fuzz / cargo-mutants jobs (and the
  `region_pinning.yml` Rust jobs) used `dtolnay/rust-toolchain@…` to
  provision a toolchain, but the self-hosted Macs already ship
  rustup+cargo, so the action's component install collided with
  `error: failed to install component: 'cargo-x86_64-apple-darwin',
  detected conflict: 'bin/cargo'` (with a companion `cargo: command not
  found` downstream). Replaced those provisioning steps with a `run:` that
  puts the **pre-installed host toolchain** on `$GITHUB_PATH` (the CLAUDE.md
  "rustup proxy is broken" pattern) — `nightly-x86_64-apple-darwin` for the
  cargo-fuzz jobs (cargo-fuzz needs nightly) and `1.91.1-x86_64-apple-darwin`
  (the `rust-toolchain.toml`-pinned channel) for the cargo-mutants +
  region-pinning jobs. Touches the offending jobs only in
  `corelink-worker.yml`, `corelink-meta.yml`, `corelink-hash.yml`,
  `corelink-reapi.yml`, `tenant-path.yml`, and `region_pinning.yml`; the
  `pr-gate`/`wasm-build` steps (which request `components`/`targets`) are
  left untouched, and the heavy nightly fuzz/mutants jobs stay
  `if: schedule`-gated.
- **Welcome greeting → native `gh` (Docker-on-mac keystone).** The
  first-PR welcome workflow used `actions/first-interaction` — a Docker
  *container action* (Linux-only) that hard-failed `Container action is only
  supported on Linux` on the all-macOS fleet, on **every** PR. Because it runs
  via `pull_request_target` (base-branch workflow), that single red blocked the
  pre-merge gate-check on every open PR at once. Replaced with a native `gh`
  first-timer greeter (same idiom as `size-label.yml`), keeping
  `pull_request_target` for the fork-PR write token. (This is the keystone that
  un-jams the merge queue; the sibling tfsec/smoke Docker-on-mac conversions
  land in #78.)
- **SBOM workflow `cargo-cyclonedx` output flag** — the `Generate SBOM`
  step in `sbom.yml` (pinned `cargo-cyclonedx 0.5.4`) passed
  `--output-cdx sbom.cdx.json`, a flag that does not exist in the 0.5.x
  CLI (it was dropped with the `--output-prefix`/`--output-pattern`
  removal in 0.5.0), so the gate failed with
  `error: unexpected argument '--output-cdx' found`. Switched to the
  0.5.x-supported `--override-filename sbom.cdx`; with `--format json`
  the tool appends the format extension to the override, emitting the
  literal `sbom.cdx.json` that every downstream job (artifact upload,
  NTIA validate, TSA attest, DT ingest, release upload) already
  references. Matches the `--override-filename` convention used by
  `scripts/sbom-aggregate.sh` / `sbom-consolidated.yml`.
- **CodeQL gate — scope extended query suites per-language.**
  `.github/workflows/codeql.yml` applied `queries:
  security-extended,security-and-quality` to *every* matrix leg, but the Rust
  CodeQL pack (`codeql/rust-queries`) ships no `*-security-extended` /
  `*-security-and-quality` suites, so the rust leg failed `Initialize CodeQL`
  with `Query pack rust-security-extended cannot be found`. Moved `queries:`
  into a per-language matrix value — empty (default `codeql/rust-queries`) for
  rust, `security-extended,security-and-quality` for the mature js/ts + python
  packs — wired via `with: queries: ${{ matrix.queries }}`.
- **BYOK key-provider-isolation matrix gate had silently never run.**
  `.github/workflows/byok_matrix_weekly.yml` invoked
  `cargo test --package corelink-byok-matrix-test --test {byok_matrix_test,
  prop_byok_per_provider,adversarial_byok_per_provider}` across all three jobs,
  but that crate was physically absorbed into `crates/corelink-byok/tests/`
  (wave-33 stream-b.2c, commits `cfda9f26` / `ea7e7e12`). Every run died at the
  build-config step with `package ID specification 'corelink-byok-matrix-test'
  did not match any packages` — never reaching a single matrix cell, so the
  weekly 4-providers × 4-ops isolation gate (and its proptest + adversarial
  tiers) had **never actually executed**. Re-pointed all three jobs at the real
  targets in the `corelink-byok` umbrella: `--package corelink-byok --features
  _matrix-test --test {matrix,matrix_prop,matrix_adversarial}` (the
  `_matrix-test` feature is mandatory — it is the `required-features` gate that
  compiles all four internal provider modules into one binary). No test logic
  changed; only the build-config invocation. (The job's `SEV-2 matrix cell
  break` echo was a red herring — the failure was a build-config error, not a
  cell failure.)
- **Unresolvable action SHA pins in `terraform-drift.yml`** — the drift-detection
  workflow failed at "Set up job" with `Unable to resolve action … unable to find
  version` because `hashicorp/setup-terraform` and `slackapi/slack-github-action`
  were pinned to non-existent commit SHAs (a bad SHA-pinning pass; both returned
  HTTP 404 from the GitHub git-refs API). Repinned each `uses:` (and its trailing
  comment) to the real commit SHA for the version in the comment:
  `setup-terraform` → `b9cd54a3c349d3f38e8881555d616ced269862dd` (`v3.1.2`);
  `slack-github-action` → `485a9d42d3a73031f12ec201c457e2162c45d02d` (`v2.0.0`).
  Every `uses:` remains SHA-pinned (supply-chain constraint WI-S01-007 / WI-S13-004).
- **`mutation-nightly` gate reported false `0.0%` kill-rates on large/slow
  crates.** The run step invoked `cargo mutants --output ./mutants.out`, and
  cargo-mutants *unconditionally* creates a subdirectory literally named
  `mutants.out` **inside** the `--output` directory (`in_dir.join("mutants.out")`,
  doc: *"Create `mutants.out` within this directory"*) — so results actually
  landed at `./mutants.out/mutants.out/`. The gate's read path was internally
  consistent with that, so the doubled path was not itself the defect. The real
  defect: cargo-mutants writes `mutants.json` *before* the baseline build and
  *before* any scenario, but the `caught/missed/unviable/timeout.txt` lists only
  as scenarios complete; when a large crate's baseline build/test fails (or the
  run is killed before a single scenario finishes), `total>0` with **zero**
  recorded outcomes — which the harness silently reported as a fake `0.0%`
  *kill-rate regression* (e.g. `corelink-auth` 454-mutant / `corelink-pat`
  203-mutant lanes), while crates whose baseline completed read correctly
  (`corelink-billing` 96.89%). Fix: (1) pass `--output .` so results land at the
  plain `./mutants.out/` and both the gate and summary read that single path
  (removing the confusing nesting); (2) when `total>0` but no outcomes are
  recorded, fail with a **distinct "Incomplete mutation run — HARNESS failure"**
  error instead of a fake `0.0%` regression, and mark the per-crate summary
  `incomplete` (`kill_rate_pct: null`) so the aggregator never opens a false
  sub-floor regression issue; (3) count `timeout.txt` survivors and guard
  `mutants.json` with a clear diagnostic. The `≥80%`/`baseline − 5pp` gate
  threshold is unchanged — a genuine sub-floor kill-rate still fails. NOTE: the
  large crates additionally need a higher per-mutant `--timeout` (and may need a
  package-scoped baseline) to actually *complete* a sweep on the self-hosted Mac;
  that capacity work is tracked separately and is out of scope for this path fix.
- **Semgrep SAST gate produced ZERO signal — dead `returntocorp` placeholder +
  zero-runner label.** `.github/workflows/semgrep.yml` could never run, so the
  static-analysis security gate was silently blind. Two compounding faults:
  (1) it ran in a job `container:` pinned to
  `docker.io/returntocorp/semgrep@sha256:000…0` — a PLACEHOLDER zeroed digest
  (`failed to resolve reference … not found` → `Value cannot be null
  (ContainerId)`) — and invoked the deprecated `returntocorp/semgrep-action`
  (returntocorp rebranded to `semgrep/semgrep` long ago; the action repo is
  archived); (2) it was pinned to `runs-on: [self-hosted, Linux, X64]`, a label
  set matching ZERO runners after the 2026-05-31 macOS-only cutover — the same
  fault class as the `actionlint` / `action-sha-audit` regressions below — so it
  would have hung pending even with a valid image, and a job `container:` cannot
  run on the Docker-less mac fleet anyway. Repointed to a native Semgrep
  invocation: re-targeted to `[self-hosted, mac, corelink-builder]`, dropped the
  container, and run pinned `semgrep==1.164.0` via the host `python3`/`pip3` in a
  throwaway venv (no Docker; not `actions/setup-python`, whose
  `RUNNER_TOOL_CACHE` provisioning is unwritable on the fleet). The same bundled
  rulepacks + repo-local CoreLink custom pack (`p/security-audit`, `p/rust`,
  `p/typescript`, `p/python`, `p/owasp-top-ten`, `p/cwe-top-25`, `./semgrep.yml`)
  now drive `semgrep scan --error --sarif`; the existing
  `codeql-action/upload-sarif` upload + findings-summary steps are preserved
  (`outputs.sarif` is now set by the new step). Expect first-run Security-tab
  alerts: this gate has been emitting no findings, so its first real execution
  may surface a backlog. (2026-06-02)
- **`license-policy` gate false positive — first-party UNLICENSED crates.**
  `scripts/license-audit.sh` ran `cargo license --json` over the whole
  workspace and flagged every first-party crate as a license violation
  (e.g. `corelink-failover-router`, `e2e-failover-router`,
  `e2e-replication-failover` — ≈87 reported), because each workspace member
  inherits the intentional `license = "UNLICENSED"` default from
  `[workspace.package]` (correct for the ~85 proprietary server crates) and
  `cargo-license` has no concept of private workspace members. The audit now
  enumerates workspace members via `cargo metadata --no-deps` and excludes
  them before the allow-list check, so only THIRD-PARTY dependencies are
  license-checked — mirroring `cargo deny check licenses`, which already skips
  them via `[licenses.private] ignore = true` in `deny.toml`. No crate was
  relicensed; the allow-list semantics for third-party crates are unchanged,
  so a genuinely-forbidden copyleft dep (GPL/AGPL/SSPL) is still caught. OUR
  OWN OSS-tagged crates' literal `MIT OR Apache-2.0` tags remain asserted by
  the companion `scripts/check-oss-license-tags.sh`.
- **macOS self-hosted CI-fleet hardening — migrate-to-self-hosted
  regressions.** The 2026-05-31 cutover to the macOS self-hosted runner fleet
  (5× `corelink-builder`, all macOS, zero Linux) left several gates silently
  broken; the new pre-merge gate-check surfaced them. Each had been failing at
  an *infra* step *before* its real check could run — masking both the
  breakage and, in one case, a real finding underneath:
  - **`actionlint` hung pending forever** — pinned to `runs-on:
    [self-hosted, Linux, X64]`, a label set matching ZERO runners. Re-targeted
    to the mac fleet. Its `docker://rhysd/actionlint` action is Linux-only and
    `taiki-e/install-action` does not package actionlint (it is not a cargo
    crate), so it now runs the fleet's host `actionlint` binary (1.7.12),
    falling back to the official pinned-release installer if a runner lacks it.
  - **`action-sha-audit` + `secrets-drift` failed at `actions/setup-python`**
    with `mkdir: /Users/runner: Permission denied` — the self-hosted runners
    have no working `RUNNER_TOOL_CACHE`, so `setup-python` cannot provision an
    interpreter and falls back to the unwritable hosted-runner path (it is the
    interpreter *provisioning*, not the Python version, that fails). Dropped
    `setup-python` from both — they only need `python3`, which ships on every
    runner. With `action-sha-audit` finally able to run, it caught a genuine
    masked violation: `smoke-install.yml` referenced `actions/checkout@v4`
    (a tag, not a 40-char SHA) — now pinned.
  - Cleared the one `SC2129` shellcheck finding `actionlint` flagged in
    `release-notes.yml` once it could finally run (grouped the
    `$GITHUB_OUTPUT` redirects).
  Same fallout class as the earlier `size-label` container-action fix. The
  remaining broken gates — the repo-wide `setup-python` → system-`python3`
  migration, the Docker-on-mac `cargo-deny` / tfsec / welcome conversions, and
  the supply-chain findings the broken `cargo-deny` was masking — land in a
  dedicated follow-up rather than a per-PR avalanche on the 5-runner Mac.
- **`cargo-audit` supply-chain gate was un-auditable — pinned tool too old for
  CVSS-4.0 advisories.** The daily cron (and PR gate) hard-failed at advisory-DB
  load with `error loading advisory database: … RUSTSEC-2026-0073.md: TOML parse
  error at line 5, column 8 — unsupported CVSS version: 4.0`. Root cause: the
  pinned `cargo-audit` `0.21.2` bundles a pre-4.0 `cvss` crate (CVSS-4.0 support
  landed in `cvss` 2.1.0, 2025-06-06), so it rejected the `cvss = "CVSS:4.0/…"`
  field on the first CVSS-4.0 advisory now present upstream. `rustsec`'s DB
  loader propagates that first per-advisory parse error and aborts the *entire*
  load (`Entries::load_file(path)?` in `database.rs`), so one modern advisory
  silently left the whole repo **un-audited** — NOT a real CVE in our dependency
  tree, and NOT fixable by pinning advisory-db (no DB commit is loadable by an
  older-than-CVSS-4.0 tool). Fix: bump `cargo-audit` `0.21.2 → 0.22.1` (rustsec
  lib 0.32 / `cvss` 2.2.0, which parses CVSS 4.0; MSRV 1.85 ≤ repo 1.91.1) —
  governed by ADR-S12-045 §6 / §14.s12.004.1 (tooling-pin bump → ADR + Security
  review; this PR's `/techlead` + Owner sign-off is that review). Additionally
  SHA-pinned the advisory DB for deterministic, reproducible audits (both jobs
  `git clone` + `git checkout` a vetted-good `rustsec/advisory-db` commit,
  `501c03f38eadd16a79d6712df424fd7d38369088`, 2026-06-02 — verified loadable by
  the bumped tool across all 1082 advisories incl. CVSS-4.0) and run
  `cargo audit --db <pinned> --no-fetch --stale`. The PR gate stays fail-closed
  (`--deny warnings`); the cron tries the live upstream DB first (fresh detection
  within SLA) and falls back to the pinned DB only if the live load fails, so it
  is never dark again. The CRITICAL→exit-1 / HIGH→warn classifier is unchanged —
  real RUSTSEC advisories in our deps still fail/alert exactly as before. Bump
  the pinned commit when refreshing the advisory floor.
  Un-blinding the gate surfaced 3 advisories that are ALREADY waived in
  `deny.toml` `[advisories].ignore` (so cargo-deny and cargo-audit must agree):
  `RUSTSEC-2023-0071` (rsa Marvin sidechannel — signing-only on operator inputs,
  mitigated, ADR-S20-RSA-MARVIN-MITIGATION), `RUSTSEC-2025-0119` (number_prefix
  unmaintained, via `indicatif → corelink-cli`, operator tool only; also added
  to `deny.toml` by #77), and `RUSTSEC-2025-0134` (rustls-pemfile unmaintained,
  dev/test-only via `bollard → testcontainers`). Mirrored those exact 3 ids
  (with the same justifications) into a new committed **`.cargo/audit.toml`**
  `[advisories].ignore` — auto-loaded by `cargo audit` from the repo root — so
  cargo-audit waives EXACTLY what `deny.toml` already waives (one logical source
  of truth, two tools), with a header cross-reference keeping the two in sync.
  Nothing not already in `deny.toml` is ignored; any NEW/unwaived advisory still
  exits 1 (verified: `.cargo/audit.toml` ignoring an unrelated id still fails on
  the live finding).
  Ratification: the `cargo-audit` `0.21.2 → 0.22.1` bump (ADR-S12-045 §6 /
  §14.s12.004.1 tooling-pin review) was **ratified by Owner + techlead on
  2026-06-02** — required for CVSS-4.0 parsing, MSRV 1.85 ≤ repo 1.91.1; this
  satisfies the §14.s12.004.1 ADR + Security review for the bump.
- **`cargo-deny` CI gate native-ized on the macOS fleet + masked supply-chain
  findings closed** (the promised `cargo-deny` follow-up above). The
  `EmbarkStudios/cargo-deny-action` is a Docker container action (Linux-only)
  and hard-failed on every macOS runner with "Container action is only
  supported on Linux" — so the gate had been RED at an *infra* step before its
  real policy check ever ran. Replaced that step in both `cargo-deny.yml` and
  `cas_foundation.yml::cargo-deny` with the same native path
  `dependabot-policy.yml` already uses (`dtolnay/rust-toolchain` +
  `taiki-e/install-action` pinning `cargo-deny@0.16.4`, all SHA-pinned). With
  the gate finally able to run, it surfaced TWO genuine findings it had been
  masking, both now closed: (1) `RUSTSEC-2025-0119` — `number_prefix` 0.4.0
  unmaintained-ONLY (no vuln), reached solely via `indicatif 0.17.11 →
  corelink-cli` with no safe upgrade available; added to `deny.toml`
  `[advisories].ignore` with a re-evaluate-on-indicatif-bump justification.
  (2) a `bans.wildcards` violation — the publishable `corelink-client-verify`
  (`publish = true`, OSS SDK) depended on `corelink-hash` via a path-only
  workspace dep, which crates.io disallows for public crates; added `version =
  "0.1.0"` to the `corelink-hash` workspace dep (in lockstep with the
  workspace `[package].version`). No `publish = false` anywhere. cargo-deny now
  exits 0 (`advisories ok, bans ok, licenses ok, sources ok`).
- **Secrets-matrix verify-gate scanned build output** — both validators
  (`scripts/secrets-checklist-verify.sh`, `scripts/validate_secrets_matrix.py`)
  walked gitignored `.open-next`/`.wrangler` bundles, whose embedded
  Sentry/OpenNext SDK references ~130–148 vendor CI-detection env vars
  (`CIRCLE_SHA1`, `VERCEL_*`, `ZEIT_*`, …) — masking the real matrix↔code drift
  (the gate was red only on a machine where admin-ui had been built). Excluded
  build output from the scan; added 5 previously-undocumented secret rows
  (#141–#145: `PAT_SIGNING_KEY`, `CORELINK_INTERNAL_AUTH_KEY`, `R2_TDK_HEX`,
  `CORELINK_CLI_RELEASE_TOKEN`, `SENTRY_AUTH_TOKEN`); reconciled the
  `CF_API_TOKEN`/`CLOUDFLARE_API_TOKEN` dual-name drift; classified ~23
  non-secret vars into the allowlists; removed 2 deleted orphans
  (`HUGR_AUDIT_CHAIN_HMAC_KEY`, `HUGR_SESSION_HMAC_KEY`, confirmed absent on all
  5 prod workers). Both gates green; `validate_specs.py` 463/0. See
  `specs/_audits/2026-06-02-secrets-naming-reconciliation.md`.
- **OSS license-tag regression (DEBT-002 reopened)** — the Wave 33-36 reorg
  silently re-inherited every crate to the `UNLICENSED` workspace default,
  wiping the `MIT OR Apache-2.0` tags on the OSS crates (0 of 13 actually
  tagged). Re-tagged the 4 pre-launch crates (`corelink-hash`,
  `corelink-client-verify`, `corelink-tenant-path`, `corelink-rate-headers`)
  with literal license + `publish = true` + `repository`; removed the dead
  `corelink-ratelimit` dep from `corelink-rate-headers` (publish blocker since
  ratelimit is closed); reclassified `corelink-audit` closed (re-export
  facade); added `scripts/check-oss-license-tags.sh` + a `license-policy.yml`
  guard step. See `specs/_audits/2026-05-31-oss-split-prep.md`.
- **Wave-36 Trigger A** — resolved `corelink-stripe-real ↔
  corelink-billing-materializer` dep-graph cycle by inverting the
  dependency direction onto the new leaf `corelink-billing-stripe-traits`
  crate (see Added). Unblocks Wave-33 stage-2.C closure path (c).
- **Wave-36 Trigger B** — `corelink-ops` platform-gated under
  `cfg(not(target_arch = "wasm32"))` inside the
  `dsr-statuspage-scheduler` consumption site; restores green wasm32
  build for `corelink-wasm` worker target. SEAL:
  `specs/_audits/sealed/2026-05-26-w36-trigger-b-seal.md`.

### Security

- **TLC `tla2tools.jar` v1.8.0 SHA-256 re-pin — RATIFIED + COMPLETE.**
  The TLA+ project re-published a non-reproducible (timestamped) `tla2tools.jar`
  to the **same `v1.8.0` tag**, so the pinned SHA-256 drifted and all TLA+
  model-check gates + `nightly` failed fail-closed at the pin check.
  Independently verified the new official asset (3× download byte-identical, two
  hash tools agree, confirmed via `gh api` it is `lemmy`'s re-cut release asset,
  re-cut 2026-05-26): OLD `d5d07d5d…dbc8ef7f` → NEW `237332bd…ff2a4fb`
  (size 4356704 → 4357560 B). **Re-pinned `TLC_SHA256_PINNED` across EVERY active
  reference:** `tla_check.yml`, `tla_region_residency_check.yml`,
  `tla_runbooks_check.yml`, `tla_dsr_erasure_check.yml`, `tla_billing_check.yml`,
  `nightly.yml`, **`cas_foundation.yml` (`tlc-canonical` job only — its
  `cargo-deny` step is owned by PR #77 and left untouched)**, and
  **`scripts/run_tlc_corelink.sh` (comment + shell var)**. A whole-repo grep
  confirms zero active references to the old hash remain (sealed/audit/spec
  records keep it as historical record, intentionally not rewritten). **Ratified
  by the tech-lead under owner delegation (2026-06-02);** §A1 sign-off marked
  SATISFIED. Ceremony + evidence in `ADR-0042-gc-worker-scheduler.md` §A1 and
  `specs/_audits/2026-06-02-tlc-v1.8.0-repin-upstream-republish.md`.
  **FOLLOW-UP (next hardening PR, not done here):** the jar is non-reproducible so
  this WILL recur on any upstream re-cut — **vendor the verified jar to immutable
  storage we control (R2 or a repo-owned release asset)** and fetch under the
  pinned SHA, removing the mutable-tag dependency.
- **Wave-36 Stage 3 — cargo-deny lockdown.** Added `[bans] deny` rules
  for the 4 absorbed-but-canonical adapter HTTPS crates
  (`corelink-stripe-real`, `corelink-statuspage-real`,
  `corelink-slack-real`, `corelink-clerk-cf`) with surgical
  `wrappers` allowlists permitting only the Wave-33 umbrella
  re-export shims to import them directly. New workspace consumers
  must route through the canonical façades
  (`corelink-billing::stripe`, `corelink-ops::statuspage` /
  `corelink-ops::slack`, `corelink-auth::clerk_cf`,
  `corelink-adapters-cloud::{stripe,statuspage,slack,clerk}`). Closes
  Wave-33 → Wave-34 follow-up #1 closure path (c). SEAL:
  `specs/_audits/sealed/2026-05-27-w36-stage-3-seal.md`. Tag
  `wave-36-final-sealed`.
- **Wave-33 → Wave-34 closure-followups #1–#5** flipped
  `audit_status: ACTIVE → CLOSED` (5/5 deferrals delivered).

---

## [1.0.0] - DRAFT — pending `framework-v1-0-0-ga` tag + Owner approval

> **DRAFT.** This section is the technical changelog companion to
> `RELEASE-NOTES-v1.0.0-GA.md`. Publication is gated on the
> `framework-v1-0-0-ga` tag and the 2-key Owner + on-call SRE approval
> recorded in `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §13.
> Wave references below trace to `specs/_audits/2026-05-16-wave{N}-closure.md`.

### Wave summary — production wiring + adversarial review + DEBT closure

The 21-sprint spec-corpus phase (S-00 → S-20) is captured in the `[0.x]`
and per-sprint sections below. The post-S-20 production wiring + GA
readiness phase ran across **29 waves** dispatched on `main` between
the `ga-engineering-gate-complete` tag (2026-05-14) and the GA
cutover window (2026-05-16+). Each wave layered adversarial review +
debt closure + production wiring + chaos / endurance evidence on top
of the sealed sprint contracts.

| Wave | Focus | SEAL evidence |
|---|---|---|
| **R-prep + wave-1..17** | Per-sprint SEAL cadence; spec-corpus build-out; production wiring layer additions (real Stripe wasm32, real BYOK providers, real CF bindings, real Neon driver, replica coordinator, DSR worker production, customer dashboard, statuspage init, breach notification templates, etc.) | Per-sprint `_audits/sprint-close-round-*.md` |
| **Wave-18** | Audit-export production wiring (Stream A); Neon shadow analytics plane (Stream B); first cold-tool adversarial review pass | `specs/_audits/sealed/2026-05-16-wave18-aggregate-closure.md` (9.5 / 10) |
| **Wave-19** | S-18 pentest scope freeze; CLI `verify-ndjson` HTTP wiring; SDK example expansion | `specs/_audits/sealed/2026-05-16-wave19-adversarial-review.md` (8.86 / 10) |
| **Wave-20** | Audit-export streaming + payload column; 10-stream adversarial review | `specs/_audits/sealed/2026-05-16-wave20-closure.md` (9.40 / 10) |
| **Wave-21** | WallClock cross-route closure; DEBT-008 mutation sweep (hash 77.78 → 97.22 %); tenant-config region resolver (9.85 / 10) | `specs/_audits/sealed/2026-05-16-wave21-closure.md` (9.55 / 10) |
| **Wave-22** | Tenant-path UUID fix (9.9 / 10); Stripe MatClock wasm32 (9.7 / 10); chaos campaign harness (8 isolated fail-CLOSED scenarios); 24h endurance harness | `specs/_audits/sealed/2026-05-16-wave22-closure.md` (9.45 / 10) |
| **Wave-23** | Chaos combined-failures matrix (executor-loss × replication-lag × tenant-isolation); pilot onboarding E2E rig; CS playbook; beta-feedback triage; LFPDPPP MX attorney-package; INV-PAT-REVOKE-PROPAGATION promotion | `specs/_audits/sealed/2026-05-16-wave23-closure.md` (9.20 / 10) |
| **Wave-24** | GA cutover dry-run (RB-GA-CUTOVER §3, G1..G6 GREEN); GA readiness final audit (CONDITIONAL GO); DEBT-008 wave-24 closure batch; PAT-revoke TLA-exempt registration; ADR-0034b dual-hat path | `specs/_audits/sealed/2026-05-16-wave24-closure.md` (codex-Opus pass in flight) |
| **Wave-25** | External pentest engagement scope freeze (RFP + shortlist + SOW); DEBT-015-BUILD path-(3) ssgRequire; endurance 10-min dress-rehearsal (0 SLO / 0 INV violations); statuspage init dress-run; tenant-config CF prod-wire; pre-GA security attestation; GA-readiness DEFER drift detector (8 → 7 scrub); wave-24 adversarial-review pass (recovery cherry-picks `d172a4a` + `8fa1c22`) | `specs/_audits/sealed/2026-05-16-wave25-closure.md` |
| **Wave-26** | **GA-1 feature freeze** (`74b8faa` — engineering corpus feature-complete from here forward); INV-CRITICAL TLA final audit (61 / 61 CRITICAL TLA+-proved, **Z = 0 milestone established**); Lote 6 v1.0.0 GA RC2 absorption; wasm32 baseline lock + getrandom fix; CF Worker prefetch wire; release notes v1.0.0 GA DRAFT (`6ce134b`); **production-tier dress-run scoring 9.36 / 10 PROCEED** + v1.0.0-GA tag draft; wave-25 adversarial review (9.00 / 10 PASS); DEBT-026 RFP tracker | `specs/_audits/sealed/2026-05-16-wave26-closure.md` |
| **Wave-27** | **GA cutover wave** (anchor) — final cutover-readiness verdict CONDITIONAL GO; 7-day endurance soak streak harness dispatched; Statuspage T-7d provisioning rehearsal; ShadowSinkFactory partial consumer adoption; wave-26 adversarial review prep; post-GA continuity runbook; pilot admin shell scripts + dashboard SSOT | `specs/_audits/sealed/2026-05-16-wave27-closure.md` |
| **Wave-28** | Cutover-prep automation wave — AWS Artifact fetch automation (DEBT-003 engineering-CLOSED); pilot-announcement comms package; Statuspage provisioning automation (DEBT-016 engineering-CLOSED); pentest finding absorption framework (7-state machine, 48 test cases); LFPDPPP MX engagement final (DEBT-025 engineering-CLOSED); pentest RFP send ceremony (DEBT-026 engineering-CLOSED); pre-cutover weekly verification cron; wave-28 adversarial review **8.96 / 10 PASS** | `specs/_audits/sealed/2026-05-16-wave28-adversarial-review.md` |
| **Wave-29** | **Cutover-wait-state + customer-acquisition wave** — engineering corpus feature-complete since wave-26 GA-1 freeze. signup.corelink.humangr.com backend + landing page + pilot admin web UI (DEBT-027 engineering-CLOSED); ShadowSinkFactory full adoption (wave-21 → -27 → -29 follow-on chain); customer-facing artefacts (audit-chain viz UI + pricing page calculator + trust center publish); perf-baseline GA freeze snapshot; DEBT register: **8 nominally OPEN → 5 engineering-CLOSED operator-bound + 3 engineering-side P1 partial** | `specs/_audits/sealed/2026-05-16-wave29-closure.md` |

### Added — production wiring + customer-facing surfaces

- **Audit export** — NDJSON streaming via signed URL + offline verifier
  (`corelink audit verify-ndjson`) + payload column + Merkle proof
  embed (waves 18 + 20).
- **Neon shadow analytics plane** — wired with RLS WITH CHECK at the
  SQL layer; real driver; replication SLO §4.27 – §4.29 observation
  streak active (waves 18 + 22).
- **BYOK 4-provider matrix** — AWS KMS, GCP KMS, Azure Key Vault,
  HashiCorp Vault — real provider pattern documented and exercised
  (R-prep + wave-25 attestation rollup).
- **Customer-facing audit export** + **customer dashboard** + **Stripe
  customer portal** (R-prep + waves 18 – 25).
- **Customer breach notification templates** (R-prep).
- **`RB-GA-CUTOVER.md`** + `RB-GA-LAUNCH-ROLLBACK.md` +
  `RB-LAUNCH-WAR-ROOM-COORDINATION.md` + 13 additional SEV-class
  runbooks (waves 19 – 24).
- **Chaos campaign** — 8 isolated fail-CLOSED scenarios + 3
  combined-failure scenarios under `cargo test --features chaos`
  (waves 22 – 23).
- **24-hour endurance harness** — built wave-22; 10-minute dress-run
  wave-25; soak scheduled in the pre-cutover T-24h window.
- **Statuspage** at `status.corelink.humangr.com` — URL-substitution mechanism
  (wave-24), dress-rehearsed wave-25.
- **External pentest engagement** — scope frozen wave-25
  (`specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md` +
  `specs/_audits/sealed/pentest/SOW-S20-EXTERNAL-PENTEST.md`); vendor engagement
  scheduled 2026-Q3.
- **Pre-GA security attestation package** — wave-25 rollup
  (`specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md`).
- **Pilot onboarding E2E rig** + **CS playbook** + **beta-feedback
  triage pipeline** (wave-23).
- **GA gate** — `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria
  across 6 tracks) + `GA-GATE-GO-NOGO-TEMPLATE.md` (2-key signature
  template) + ADR-0034 / ADR-0034b PRR staffing + dual-hat waiver
  paths.
- **GA-readiness DEFER drift detector** — CI gate that prevents
  silent regression of the DEFER population between wave-25 and
  cutover (wave-25 stream #4).
- **GA-1 feature freeze** (wave-26 `74b8faa`) — engineering corpus
  feature-complete from this commit forward; post-freeze admits only
  `specs/` / `docs/` / `.github/` / `scripts/` / `apps/` changes per
  ADR-0034b §3 scope-fence.
- **Production-tier dress-run** (wave-26) — 13 / 13 steps PASS,
  6 / 6 greenlights GREEN, 0 / 6 rollback triggers fired,
  prep-ring isolation guard verified, **GA-readiness 9.36 / 10
  PROCEED** (`specs/_audits/sealed/2026-05-16-prod-deploy-dressrun.md`);
  pre-authored v1.0.0-GA tag draft at `docs/release/v1.0.0-GA-tag-draft.txt`.
- **7-day endurance soak streak** harness (wave-27 stream #4) —
  wall-clock 168 h SLO observation streak feeding NO-GO trigger #5.
- **Post-GA continuity runbook** (`RB-POST-GA-CONTINUITY.md`) —
  wave-27 anchor.
- **Pilot admin path** — shell scripts (`grant-pilot-tier.sh`,
  `list-pilot-tenants.sh`, `pilot-24h-checkin.sh`) wave-27;
  Grafana dashboard SSOT `dashboards/grafana/dash-pilot-tenants.yml`;
  **Owner-facing pilot admin web UI** wave-29 stream #3.
- **AWS Artifact PDF recorder + fetch automation** (wave-28) —
  DEBT-003 engineering-CLOSED.
- **Statuspage provisioning automation** (wave-28) — DEBT-016
  engineering-CLOSED; operator DNS + ORG-ID swap is the only
  remaining T-7d step.
- **Pentest finding absorption framework** (wave-28) — 7-state
  machine (`RECEIVED → TRIAGED → IN_FIX → FIXED → RETEST_SUBMITTED →
  RETEST_PASSED → ABSORBED`); CVSS / P-tier coherence enforced; 48
  test cases passing.
- **Pre-cutover weekly verification cron** (wave-28) — automated
  weekly green-light digest against the cutover commit base.
- **Pilot announcement comms package** (wave-28).
- **signup.corelink.humangr.com backend + landing page** (wave-29 streams #1, #2) —
  token-based pilot-slot reservation; idempotent token issuance + 24h
  replay-safe consumption; 4-section public landing (hero / value-prop
  / 3-tier pricing-summary / signup form). DEBT-027 engineering-CLOSED.
- **Customer-facing audit-chain visualisation UI** (wave-29 stream #6) —
  tenant-scoped Merkle-path inspector + tamper-evidence proof viewer.
- **Public pricing page calculator + 4-tier comparison + internal
  cost-worksheet** (wave-29 stream #7).
- **Trust center publish pipeline** (wave-29 stream #8) — consolidates
  SOC 2 / ISO 27001 / PCI DSS / LGPD / FedRAMP-informational status +
  DEBT-003 AWS Artifact PDF link (gated by DEBT-003 closure).
- **Perf-baseline GA freeze snapshot** (wave-29 stream #9) — 5 SLO
  families pinned at the cutover commit base; consumed by NO-GO
  trigger #5 (endurance 7-day soak streak).
- **ShadowSinkFactory full adoption** (wave-21 → wave-27 → wave-29
  follow-on chain) — zero direct `ShadowSink::new(...)` call sites in
  consumer crates post-SEAL; all sinks issued via
  `ShadowSinkFactory::for_tenant(tenant_id)` with region-aware
  resolution.
- **Release-notes editorial-polish audit**
  (`specs/_audits/sealed/2026-05-16-release-notes-editorial-polish.md`) and
  **customer-facing FAQ** (`RELEASE-NOTES-v1.0.0-GA-FAQ.md`) — wave-30
  stream #10.

### Changed

- **Invariant registry** — 197 declared (61 CRITICAL, 132 HIGH,
  4 MEDIUM); **82 TLA+ verified**; **all 61 CRITICAL TLA+-proved**
  (Z = 0 milestone established wave-26 stream #9, preserved across
  waves 26 → 29); 0 orphan refs; 143 / 143 WI-coverage; 15 legacy →
  canonical aliases documented; 0 UNKNOWN severity classifications.
- **DEFER counter** scrubbed wave-25 (`specs/_audits/sealed/2026-05-16-ga-readiness-defer-scrub.md`):
  stale "Docs CI billing reinstatement" row removed (CI runs locally
  per `feedback_ci_local`); current counter is **7 external items**
  (5 user-bound + 1 vendor-bound + 1 mixed); drift detector exit 0
  held stable across waves 25 → 29.
- **`canary` → `staging-only`** chaos discipline at GA per S-17
  cross-functional decision (production chaos not authorised on
  the GA cutover day).

### Fixed

- **DEBT-001** — secrets matrix tighten (closed wave-15);
  `validate_secrets_matrix.py` code-only false-positive resolved
  wave-22.
- **DEBT-002, DEBT-004, DEBT-005, DEBT-006, DEBT-007, DEBT-009,
  DEBT-011, DEBT-012, DEBT-014, DEBT-017 – DEBT-020, DEBT-022,
  DEBT-024** — closed (no waiver) across waves 15 – 24.
- **DEBT-008** — mutation kill-rate baseline empirically CLOSED for 8
  of 15 crates (≥ 75 % floor on the remaining 5 via the
  `mutation-nightly.yml` CI-nightly matrix).
- **WallClock cross-route** — wave-21 closure (`f3462c6`);
  9.55 / 10 adversarial score.
- **`INV-PAT-REVOKE-PROPAGATION`** — promoted to CRITICAL wave-23;
  TLA+ exempt under §4.3 (wall-clock obligation, not consensus
  property); empirical sub-second propagation verified via mutation
  sweep + runbook drill.

### Security

- **External pentest** — scope frozen (wave-25); **RFP send ceremony
  executed wave-28** (DEBT-026 engineering-CLOSED); vendor 30-day
  selection clock running. Earliest retest letter target 2026-07-29.
  Pentest finding absorption framework SEAL'd wave-28 (7-state
  machine; 48 test cases). HIGH / CRITICAL findings gate any future
  `GA-Full` / `v1.1.0` promotion. **No external pentest report is
  yet published**; customer-facing security claims do not depend on
  a completed external pentest at v1.0.0 GA.
- **Adversarial review** — 10 consecutive waves; last-5 PASS-trend
  rolling mean **9.32 / 10** (raw chronological 8.83; charter
  rolling-window framing 9.41); wave-28 review re-anchored at
  **8.96 / 10 PASS**; 0 P0 / 0 outstanding P1 at any wave boundary
  since wave-19 SEAL. Wave-24 6.95 / 10 CONDITIONAL recovered via
  wave-25 cherry-picks `d172a4a` + `8fa1c22` to a 9.40 projection.
- **GA-readiness** — wave-26 production-tier dress-run **9.36 / 10
  PROCEED** (13 / 13 steps PASS; 6 / 6 greenlights; 0 / 6 rollback
  triggers); wave-27 final cutover-readiness verdict CONDITIONAL GO.
- **Compliance** — SOC 2 Type I ready, ISO 27001 Stage-1 eligible,
  GDPR / LGPD / PCI-SAQ-A / CCPA ready; **LFPDPPP MX engineering-CLOSED
  wave-28** (DEBT-025; attorney sign-off operator-paced); FedRAMP
  Moderate documented as not in scope for GA.
- **BYOK** — 4-provider FIPS attestation matrix; **AWS Artifact
  recorder + fetch automation SEAL'd wave-28** (DEBT-003
  engineering-CLOSED; operator T-7d download remains).

---

## [1.0.0-rc.1] - 2026-05-14 — GA Engineering Gate complete

Tag: `ga-engineering-gate-complete` (HEAD `f09d640`, alias of `s20-impl-sealed`).

The **GA Engineering Gate** is the binary technical readiness boundary —
distinct from `Launch Orchestration` (CAP-LAUNCH-001, marketing/PR/Product
Hunt). This tag asserts that 21 sprint specs are SEALED, ~70 Rust crates
compile and test, 8 TLA+ specs check, 62 runbooks exist, and 14 canonical
sources are green. Production wiring (Wave R-2..R-4) and external evidence
(Wave R-5..R-7) follow under `ROADMAP-TO-GA.md`.

### Added — GA Engineering Gate

- **CAP-GA-001** — Engineering gate `CONDITIONALLY_APPROVED` per PRR-S20-GA
  pending 8 D+60 evidence items (`ROADMAP-TO-GA.md` §5 Wave R-5/R-7).
- **CAP-GA-002** — External pentest engagement contract template + report
  intake workflow (Schellman / A-LIGN); EVT-040 evidence event registered.
- **CAP-GA-003** — SOC 2 gap analysis preparation (Drata / Vanta) with
  concrete GAP-XX items and fix timeline for Type I engagement at D+180.
- **CAP-GA-004** — Lighthouse customer migration framework (2 team-tier,
  1 enterprise-BYOK); SLA-claim-met-in-30d criterion encoded.
- **CAP-GA-005** — SLA contractual terms + DPA v1 template published in
  `legal/`; ready for 3-customer signature flow.
- **CAP-GA-006** — Incident response 24/7 PagerDuty schedule covering
  3 regions; response-time < 5 min tested.

### Changed

- "10 canonical sources" → **14 canonical sources** (codex finding;
  alignment with §3 of `_spec_contract.md`).
- "Full SBOM v1.0" → **CycloneDX 1.5+** (alignment with S-12 R-S12-3).
- Roadmap-to-GA marketing/launch concerns separated from engineering
  gate per codex feedback; `CAP-LAUNCH-001` carved out of `CAP-GA-*`.

### Security

- 14 canonical sources verified green at gate (SECURITY-MODEL,
  PRIVACY-MODEL, AUTH-MODEL, KEY-MANAGEMENT, COMPLIANCE-MATRIX,
  STORAGE-SEMANTICS-MATRIX, RESILIENCE-PATTERNS, OBSERVABILITY-MODEL,
  SLO-CATALOG, FAILURE-MODES, INVARIANT-REGISTRY, DATA-MODEL,
  REMOTE-CACHE-PRODUCT-PROFILE, FRAMEWORK-00).

---

## [0.20.0] - 2026-05-14 — S-20: GA Readiness

Tag: `s20-impl-sealed` (HEAD `f09d640`).

### Added

- **WI-S20-001..008** — PRR global execution + external pentest contract
  + 30d-sustained-staging evidence framework + SOC 2 gap-analysis prep
  + 3-lighthouse-customer migration scaffold + SLA/DPA v1 publish
  + incident-response 24/7 PagerDuty rotation + launch orchestration kit.
- **WI-S20-007** — 30d staging evidence framework + TLA+ 4 runbooks
  + 90d SBOM retention + PRR-S20 closing audit.
- **WI-S20-008** — Launch orchestration prep: press release + 5 blog
  posts + 3 case studies + Product Hunt kit + social media kit
  + launch runbook + launch-metrics dashboard. **Final WI of final sprint.**

### Changed

- Sprint-close round-1 P0 remediation (7.2/10 → SEAL approved); see
  `_audits/sprint-close-round-1.md`.
- Synthetic-page-drills migration renumbered `0042` → `0043` to remove
  conflict with `0042_lighthouse_customers.sql`.

### Fixed

- P0 audit findings round-1 cascade — engineering-gate-vs-launch
  separation enforced in §4 capability table.

### Security

- External pentest report intake gated on EVT-040; HIGH/CRITICAL
  remediation pre-condition for `GA-Full` tag promotion.

---

## [0.19.0] - 2026-05-14 — S-19: Customer Onboarding

Tag: `s19-impl-sealed` (HEAD `e967c65`).

### Added

- **WI-S19-001..006** — Self-service signup business logic + DPA
  click-through (CTRL-PRIV-CONSENT-001..006 capture, EVT-049, signed
  JWT receipt) + tier selection + Stripe Checkout integration
  + enterprise inquiry form with white-glove handoff
  + conversion-funnel instrumentation + DPA versioning re-acceptance.
- **WI-S19-004** — Tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST
  + D1 row-lock atomicity (subscription activation requires DPA signed).
- **WI-S19-005** — Enterprise inquiry + Slack/CRM atomic outbox + 24h
  auto-reply SLA.
- 4 D1 migrations: `0037_signup_orchestration` · `0038_dpa_acceptances`
  · `0039_tier_selection` · `0040_enterprise_inquiries`
  · `0041_dpa_versioning`.

### Changed

- Lane upgrade STANDARD → **HIGH_RISK** per codex finding
  (FF-HR-009 customer-facing contract).
- Region pinning derives from rendered-locale cookie `corelink_locale`
  (set by S-16 middleware), not `Accept-Language` header
  (Lote 10.19 codex P1 canonical fix).
- Sprint-close round-1 P1 remediation (8.4/10 → SEAL approved).

### Security

- FF-HR-009 enforced: DPA + Terms click-through cryptographically
  proven via signed JWT receipt (legal-exposure mitigation).
- Stripe webhook signature verification + D1 idempotency keys
  (`0044_stripe_webhook_events_processed`).

---

## [0.18.0] - 2026-05-14 — S-18: Public Docs + API Reference + Pricing

Tag: `s18-impl-sealed` (HEAD `a1d00f1`).

### Added

- **WI-S18-001..005** — Docusaurus 3.x at `apps/docs/` deployed to CF
  Pages with Diátaxis taxonomy (tutorial / how-to / reference /
  explanation) + 5-min Bazel/Buck2/Native quickstart + REAPI v2
  auto-generated reference + SDK guides (Python/Go/JS/CLI) + compliance
  & security page (SOC 2 timeline + SBOM access + pentest exec summary)
  + pricing page (5 tiers + feature matrix + calculator).
- **CAP-DOCS-007** — i18n (en/pt-BR/es) + WCAG 2.2 AA + Lighthouse ≥ 95.
- **CAP-DOCS-008** — Vale tone-lint + lychee broken-link CI gates.

### Changed

- Sprint-close round-1 P0 remediation (5.5/10 → SEAL target reached).
- Sprint-close round-2 P1 — replace 28 i18n stub relative imports with
  `@site/src/` alias.
- Pin Node 20 + add `.npmrc` / `.nvmrc` — root-cause Docusaurus build
  failure under Node 22.

### Fixed

- Cross-functional anti-scope gate (§10): pricing/security claims
  require Finance + Legal + Security review before publish.

---

## [0.17.0] - 2026-05-14 — S-17: Ops Maturity

Tag: `s17-impl-sealed` (HEAD `bbbd99a`).

### Added

- **WI-S17-001..006** — Chaos engineering automation (weekly staging
  chaos, deterministic seed, ≥ 8 FMs covered, auto-rollback on SEV-1)
  + DR drill scheduler (semestral cadence, full region outage simulation)
  + runbook dry-run tracker EVT-017 (monthly P0/P1 runbook cadence)
  + incident + blameless post-mortem templates + oncall rotation with
  fatigue tracking + chaos catalog + game-day tabletop exercises.
- **WI-S17-003** — Runbook dry-run tracker + 3 P0/P1 monthly cadence
  (PAT-RUNBOOK-DRILL-001).
- **WI-S17-004** — Incident + blameless post-mortem templates +
  1 synthetic SEV-2 post-mortem + RB-POSTMORTEM-PROCESS.
- **WI-S17-005** — Oncall scheduler + fatigue tracking + PagerDuty
  integration trait + Grafana dashboard.
- 4 D1 migrations: `0033_chaos_runs` · `0034_dr_drill_runs`
  · `0035_runbook_drills` · `0036_oncall_pages`.

### Changed

- Sprint-close round-1 P0 remediation (7.6/10 → SEAL approved).
- Sprint duration corrected 2.5 → 4 weeks to accommodate parallel
  4-week chaos test (codex finding).

### Security

- Chaos discipline: production chaos NOT authorized at GA;
  staging-only weekly for 4 weeks pre-GA.

---

## [0.16.0] - 2026-05-14 — S-16: Frontend Admin UI

Tag: `s16-impl-sealed` (HEAD `5d70701`).

### Added

- **WI-S16-001..007** — Next.js 15 at `apps/web/` deployed to CF Pages
  with: tenant onboarding flow + per-tenant usage dashboard
  + audit-log viewer (CloudEvents R2 query proxy) + consent management
  UI (6-field proof: notice_text_hash + version + locale + wording_id
  + ui_capture_ts + submission_ts) + DSR request form (6 rights, MFA
  re-auth, JWT receipt) + PAT management + privacy/sub-processors
  pages + billing overview.
- **WI-S16-007** — Playwright e2e + Lighthouse + axe sweep + CSP
  enforce + UX workshop + PRR-S16.
- **CAP-UI-009** — i18n (en/pt-BR/es) + WCAG 2.2 AA baseline.

### Changed

- Sprint-close round-1 P0 remediation (7.4/10 → SEAL prep).
- HF-S17-001 — remove nested `<html>` from `[locale]/layout.tsx`.
- **CAP-UI-002** (full Grafana embed) **DEFERRED** post-S-16 to S-18 or
  post-GA (Lote 10.16 codex P0 fix); S-16 ships basic plan/quota
  progress widget + audit-viewer link + billing overview.

### Deprecated

- Inline `--telemetry=on` CLI flag (rejected per Lote 10.15 alignment;
  telemetry only via persistent `~/.corelink/config.toml`).

### Security

- Hardened CSP `default-src 'none'` + explicit allowlists enforced;
  XSS-exfiltration of PAT mitigated.

---

## [0.15.0] - 2026-05-14 — S-15: CLI + SDK Integration

Tag: `s15-impl-sealed` (HEAD `88cd55b`).

### Added

- **WI-S15-001..006** — `corelink` CLI (7 subcommands: ls / get / put
  / stat / bench / doctor / version) cross-OS signed (macOS notarized,
  Linux GPG-signed, Windows Authenticode-signed) + Bazel starter project
  with credential-helper-protocol + Buck2 starter project + FFI wrappers
  (Python pyO3, Go cgo, JS/TS WASM) with client-verify default-on
  + CI templates (GitHub Actions + GitLab + CircleCI).
- **WI-S15-006** — Fuzz 1M + 3-OS signing + 2 OSS-proof
  conformance + PRR-S15 + adversarial summary.
- `corelink doctor` 8-check actionable diagnostic (network, auth,
  storage write, storage read, BYOK, region, quota, client-verify).

### Changed

- Sprint-close round-1 P0 remediation (8.7/10 final).
- Bazel `.bazelrc` uses credential-helper protocol (Bazel 6+) — PAT
  via stdout-JSON, never in `argv` (CTRL-CRED-001 enforcement;
  Lote 10.15 canonical fix).

### Security

- Tokens never in CLI args; only env-var `CORELINK_PAT` or
  `~/.corelink/config.toml`.

---

## [0.14.0] - 2026-05-14 — S-14: Region Expansion + BYOK

Tag: `s14-impl-sealed` (HEAD `70ea887`).

### Added

- **WI-S14-001..009** — 4 production regions (WNAM us-west, ENAM us-east,
  WEUR eu-west, SAM sa-east) + tenant primary_region pinning + hot-blob
  cross-region replication (top 1% via offline aggregation, escaping
  INV-OBS-CARDINALITY-BUDGET) + PAT-REGION-FAILOVER-001 read failover.
- **CAP-BYOK-001..006** — BYOK adapter trait `crates/corelink-byok`
  with 4 KMS providers: AWS KMS (FIPS 140-3 L1), GCP KMS (FIPS 140-2 L1),
  Azure Key Vault Premium (FIPS 140-2 L2), HashiCorp Vault Enterprise
  (FIPS 140-3 L1).
- **WI-S14-007** — Ed25519 (FIPS 186-5) erasure attestation + JCS
  canonicalization + 7y retention + verify path.
- **WI-S14-008** — DPA amendment + Schrems II TIA + Legal external
  review path.
- **WI-S14-009** — TLA+ `region_residency` spec + RB-BYOK-REVOKE
  prod-grade + 3 RB dry-runs + pentest stub + PRR-S14.
- 6 D1 migrations: `0027_region_provisioning` · `0028_tenant_primary_region`
  · `0029_hot_blobs` · `0030_byok_envelope` · `0031_byok_tenant_status`
  · `0032_erasure_attestation`.

### Changed

- Customer kill switch SLA ≤ **6 min p99** (60s detection + 5min DEK
  cache TTL hard, codex-corrected from initial 5 min target).
- **DEK derivation** — random 32 bytes via `getrandom::getrandom`
  CSPRNG, **not** BLAKE3-derived from blob hash (Lote 10.14 codex P1
  fix; deterministic DEK = compromise propagation across blobs).
- AES-256-GCM (FIPS 197 + FIPS 140-3 approved) with 96-bit random
  nonce; per-blob envelope encryption.
- Sprint-close P0+P1 remediation cascade (6.48/10 FAIL → 8.5+).

### Fixed

- Port `corelink-byok-revocation` + `customer-alerts` to canonical
  trait surface (sprint-close P0-1 + P0-2 resolution).

### Security

- **R1-9!** FIPS-mode toggle documented per provider in
  `compliance/byok-fips-matrix.md`.
- Cross-region tenant isolation property-tested at 20k iter, 0 leaks.
- Customer kill switch end-to-end runbook RB-BYOK-REVOKE dry-run
  evidence committed.

---

## [0.x] - 2026-04 to 2026-05 — Spec corpus phase (S-00 → S-13)

Tags: `s00-impl-sealed` ... `s13-impl-sealed` (14 tags).

This collapsed section records the **pre-1.0 framework history**: the
foundation sprints that delivered the spec corpus (262 docs · 136 invariants),
the reference Rust crates (~70 crates wired against `InMemoryFake` traits),
the 8 TLA+ specifications, and the canonical-source bedrock that
everything from S-14 onwards inherits from.

| Sprint | Date | Name | Lane | Highlights |
|---|---|---|---|---|
| **S-00** | 2026-04-15 | Roadmap & Planning | n/a | 14 canonical sources skeleton + sprint waveform planned. |
| **S-01** | 2026-04-29 | CAS Foundation (write path + HMAC + integrity) | HIGH_RISK | `corelink-hash` BLAKE3 + `corelink-worker` R2 PUT + `corelink-reapi` REAPI v2 gRPC handlers (BatchUpdateBlobs, Capabilities, ByteStream::Write); vendored bazelbuild/remote-apis @ v2.12.0 proto subset; CloudEvents 1.0 audit envelope; INV-CAS-INTEGRITY enforced. |
| **S-02** | 2026-04-30 | CAS Read Path + Client Verify | HIGH_RISK | Server-and-client BLAKE3 verify default-on; `corelink-client-verify` crate; CTRL-CAS-002. |
| **S-03** | 2026-05-01 | Auth Real | HIGH_RISK | Clerk integration + PAT scope model + MFA + `corelink-clerk` + `corelink-clerk-cf`. |
| **S-04** | 2026-05-01 | Action Cache (AC) | HIGH_RISK | AC put/get + HKDF-keyed-MAC signature + dedup-safe; `corelink-ac::merkle` RFC 6962-style domain separation. |
| **S-05** | 2026-05-01 | Multipart Upload + Chunking + Merkle (blobs > 5 MiB) | HIGH_RISK | `corelink-chunker` + `corelink-manifest` Merkle manifest with O(1) streaming-memory verify (INV-MULTIPART-STREAMING-MEMORY); MAX_CHUNKS_PER_BLOB = 81920. |
| **S-06** | 2026-05-01 | Garbage Collection: Mark & Sweep + INV-GC-001/004 | HIGH_RISK | `corelink-gc` worker binary + scheduler + 8 GcEventType audit taxonomy + degrade overload-detector + partial-UNIQUE running-status invariant. |
| **S-07** | 2026-05-02 | Dedup + Eviction Policy (intra-tenant default; cross-tenant backlog) | STANDARD | LRU + LFU + size-tiered eviction; intra-tenant dedup default; cross-tenant deferred. |
| **S-08** | 2026-05-03 | Rate Limiting Multi-Camada + Quotas + Abuse Detection | HIGH_RISK | Token-bucket multi-layer + abuse-score + edge blocklist + quota FSM + circuit breaker. |
| **S-09** | 2026-05-05 | Observability Stack | HIGH_RISK | OTLP traces + structured logs + 4-burn-rate SLO alerts + audit-log CloudEvents R2 + cardinality budget INV-OBS-CARDINALITY-BUDGET. |
| **S-10** | 2026-05-07 | Billing Pipeline | HIGH_RISK | Stripe webhook idempotency + usage-event-idem + billing replay audit + reconciliation drift detector + Stripe Checkout. |
| **S-11** | 2026-05-09 | Privacy Pipeline | HIGH_RISK | CTRL-PRIV-CONSENT-001..006 6-field consent capture + 6 DSR rights + erasure log + notice-text-hash canonicalization. |
| **S-12** | 2026-05-11 | Supply Chain Hardening | HIGH_RISK | CycloneDX 1.5+ SBOM + cosign signing + SLSA L3 attestation + cargo-audit + cargo-deny + Dependabot auto-merge + Bazel/Buck2 starter CI. |
| **S-13** | 2026-05-13 | Admin Plane | HIGH_RISK | Admin op-log + rotation-state + admin surfaces gated behind feature-flag for tenant emergency ops. |

**Cumulative deliverables at end of phase:**

- ~70 Rust crates compiling + testing under `cargo test --workspace`.
- 26 D1 migrations (`0001` ... `0026`) all additive-only (INV-AUTH-MIGRATION-ADDITIVE).
- 8 TLA+ specifications.
- 244 vitest cases in `apps/web` + 264 in `apps/docs`.
- 62 runbooks in `specs/_runbooks/`.
- 14 canonical sources SEALED (`SECURITY-MODEL`, `PRIVACY-MODEL`,
  `AUTH-MODEL`, `KEY-MANAGEMENT`, `COMPLIANCE-MATRIX`,
  `STORAGE-SEMANTICS-MATRIX`, `RESILIENCE-PATTERNS`,
  `OBSERVABILITY-MODEL`, `SLO-CATALOG`, `FAILURE-MODES`,
  `INVARIANT-REGISTRY`, `DATA-MODEL`, `REMOTE-CACHE-PRODUCT-PROFILE`,
  `FRAMEWORK-00`).

---

## Migration Notes (post-1.0)

### Applying D1 migrations

CoreLink ships **43 D1 migrations** (`migrations/d1/0001_blob_meta.sql` ...
`migrations/d1/0044_stripe_webhook_events_processed.sql`, with one
renumbering hop `0042_lighthouse_customers.sql` introduced in S-20).
All migrations are **additive-only** (INV-AUTH-MIGRATION-ADDITIVE):
no destructive `DROP`, no breaking column rename without dual-write
transition window.

Apply via the canonical runner:

```bash
./scripts/d1-migration-runner.sh <staging|prod> <d1-binding-id>
```

Pre-conditions:

- Cloudflare auth (`wrangler login`) from an operator workstation.
- D1 binding ID for the target environment.
- Read `specs/_runbooks/RB-D1-MIGRATION-APPLY.md` before promoting to prod.

Offline pre-flight (CI also runs these):

```bash
python3 scripts/check_migrations_additive.py
python3 scripts/d1-migration-verify.py --schema-only
cargo test -p corelink-d1-migrations --test d1_migration_integration
```

### Required environment variables

The following secrets MUST be set in the operator workstation or CF
Workers binding before a fresh deploy will boot:

| Var | Scope | Notes |
|---|---|---|
| `CORELINK_PAT` | CLI / SDK | Tenant PAT; never pass via CLI argv (CTRL-CRED-001). |
| `CLERK_SECRET_KEY` | Worker | Server-side Clerk JWT verify. |
| `CLERK_PUBLISHABLE_KEY` | Worker / Web | Client-side. |
| `STRIPE_SECRET_KEY` | Worker | Subscription + webhook. |
| `STRIPE_WEBHOOK_SECRET` | Worker | Signature verify. |
| `PAGERDUTY_INTEGRATION_KEY` | Worker | Events API v2 (S-17). |
| `SLACK_WEBHOOK_URL` | Worker | Enterprise inquiry notify (S-19). |
| `HUBSPOT_API_KEY` | Worker | CRM atomic outbox (S-19). |
| `AWS_KMS_KEY_ARN` | Tenant BYOK | Per-tenant; required if `tenant.byok_provider = aws`. |
| `GCP_KMS_KEY_NAME` | Tenant BYOK | Per-tenant. |
| `AZURE_KEY_VAULT_URI` | Tenant BYOK | Per-tenant. |
| `VAULT_TRANSIT_KEY` | Tenant BYOK | Per-tenant. |
| `GRAFANA_CLOUD_PUSH_TOKEN` | Worker | Metrics push. |
| `DRATA_API_TOKEN` | Worker | SOC 2 evidence collection (S-20). |

### Breaking changes warning — major version bumps

When bumping the major version (e.g., `2.0.0`):

1. Review all entries under `### Removed` and `### Changed` since the
   previous major in this changelog.
2. Run `scripts/d1-migration-verify.py --diff-from <prev-major-tag>`
   to catalogue destructive operations introduced under the new major.
3. **Required**: 6-month deprecation window for any public REAPI v2
   surface change; cross-reference `specs/_canonical/REMOTE-CACHE-PRODUCT-PROFILE.md`
   §wire-compat-matrix.
4. **Required**: signed customer notice 30d before any breaking change
   that affects PAT scope, BYOK envelope format, or audit event schema.
5. Re-run external pentest (Schellman or A-LIGN) before tagging `vN.0.0`.
6. Bump `compliance/version_pins.yaml` and trigger SOC 2 Type II
   continuous-monitoring re-baseline.

[Unreleased]: https://github.com/humangr-labs/corelink/compare/ga-engineering-gate-complete...HEAD
[1.0.0-rc.1]: https://github.com/humangr-labs/corelink/releases/tag/ga-engineering-gate-complete
[0.20.0]: https://github.com/humangr-labs/corelink/releases/tag/s20-impl-sealed
[0.19.0]: https://github.com/humangr-labs/corelink/releases/tag/s19-impl-sealed
[0.18.0]: https://github.com/humangr-labs/corelink/releases/tag/s18-impl-sealed
[0.17.0]: https://github.com/humangr-labs/corelink/releases/tag/s17-impl-sealed
[0.16.0]: https://github.com/humangr-labs/corelink/releases/tag/s16-impl-sealed
[0.15.0]: https://github.com/humangr-labs/corelink/releases/tag/s15-impl-sealed
[0.14.0]: https://github.com/humangr-labs/corelink/releases/tag/s14-impl-sealed
[0.x]: https://github.com/humangr-labs/corelink/compare/s00-impl-sealed...s13-impl-sealed
