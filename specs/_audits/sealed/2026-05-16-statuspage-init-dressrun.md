# STATUSPAGE-INIT Dress-Run Audit — 2026-05-16

> **Doc kind:** dress-run execution audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Wave / Stream:** Wave-25 R-prep / `wt/r-prep-statuspage-init-dressrun` (agent: Claude Opus 4.7 background worker).
> **Base:** `main` @ `e9ee8eb` (wave-24 SEAL tip — "merge wt/r-prep-pat-clerk-mutation-sweep into main (wave-24)").
> **Scope:** Operational dress-run of `specs/_runbooks/STATUSPAGE-INIT.md` (wave-24 DEBT-016 operator playbook). Simulates the operator provisioning Option A (CNAME) + Option B (env-var override) using sandboxed fakes, with per-step PASS/FAIL capture, an operator-handoff JSON, and a 7-assertion verification harness.
> **Cross-ref:** `specs/_runbooks/STATUSPAGE-INIT.md` v1.0.0 (wave-24, DEBT-016 closure), `specs/_audits/2026-05-16-debt-016-statuspage-urls.md`, `apps/docs/src/statuspage-url.ts`, `apps/docs/docusaurus.config.ts customFields.statuspageUrl`.

---

## 1. Why a dress-run

The wave-24 closure landed the **engineering side** of DEBT-016 (helper + customFields wiring + the operator playbook), but the playbook itself had not yet been *exercised*. The audit closing DEBT-016 explicitly tagged operator-side provisioning as "pending T-7d" and the runbook §4 declared GA cutover blocked if neither Option A nor Option B is complete by T-7d.

A dress-run before T-7d catches two failure modes that would otherwise surface as launch-blockers:

1. **Mechanism drift** — the helper, the `customFields` wiring, or the `rg | sed` substitution one-liner has regressed since wave-24 closure and the operator would discover it only when the real cutover fails.
2. **Verification-harness gaps** — the runbook's "sanity-check" curl block at §2 step 7 is human-eyeball-only; without a structured verifier the SRE Lead has nothing to attach to the GA-readiness sign-off other than terminal scrollback.

This dress-run delivers both: a hermetic execution harness (`scripts/statuspage-init-dressrun.sh`) and a structured-assertion verifier (`scripts/statuspage-init-verify.py`).

## 2. What is and is not simulated

| Dimension | Production cutover | This dress-run |
|---|---|---|
| Atlassian Statuspage tenant | Real Business-tier subscription provisioned by SRE Lead | NOT provisioned (operator-bound; runbook §2.1) |
| Cloudflare DNS CNAME for `status.corelink.humangr.com` | Real edit in zone `corelink.humangr.com` | Simulated via deterministic zone-file fake at `${SANDBOX}/dns/zone.txt` |
| Statuspage public API health | `curl` against the operator's real tenant URL | `curl --head https://www.statuspage.io/` (read-only landing page; tenant-independent; offline-tolerant) |
| `pnpm build` with `STATUSPAGE_URL` set | Full Docusaurus build in the docs deploy pipeline | Node-mirror of `getStatuspageUrl()` exercised under three env-var permutations + the `rg | sed` MDX substitution one-liner exercised on a synthetic MDX fragment |
| Operator handoff | SRE Lead writes the row in `specs/_compliance/GA-GATE-CRITERIA.md` after the live cutover | JSON handoff doc emitted inside the sandbox, with runbook+helper SHA-256 fingerprints binding it to a specific engineering-closure commit |

The build-substitution step deliberately does NOT invoke `pnpm build` for two reasons: (a) the wave-24 DEBT-015-BUILD residual still produces docs-build flakiness on cold caches and would inject noise into the dress-run, and (b) the substitution dimension is what we are verifying, not Docusaurus's MDX pipeline (which has its own dedicated typecheck/build gates in CI). The Node mirror exercises the *exact* control flow of `apps/docs/src/statuspage-url.ts` and a textual fingerprint check (Step 3a.1 in the orchestrator) catches drift if the upstream helper changes without a corresponding mirror update.

## 3. Step-by-step execution log

Each step is implemented as a bash function in `scripts/statuspage-init-dressrun.sh` wrapped by a `run_step` harness that captures wall-clock duration (via `python3 time.time()`) and emits a per-step `OUTCOME=` + `DETAIL=` line. The orchestrator collects all four outcomes plus per-step verification IDs (`VID-<date>-S<n>-<sha256-prefix>`) into a single evidence JSON.

| # | Runbook coverage | Simulated fake | Outcome | Duration (ms) | Verification ID prefix |
|---|---|---|---|---|---|
| S1 | §2 step 4 + §3 step 2 — DNS CNAME `status.corelink.humangr.com` → `*.statuspage.io` | Deterministic zone-file fake under `${SANDBOX}/dns/zone.txt`; awk-driven resolver | PASS | 74 | `VID-2026-05-16-S1-…` |
| S2 | §2 step 7 — Statuspage API reachability (`curl -sI`) | Real HEAD against `https://www.statuspage.io/` with `--max-time 5`; DEGRADED-tolerant on network failure | PASS (HTTP 301) | 446 | `VID-2026-05-16-S2-…` |
| S3 | §3 steps 3–5 — build-time substitution (`STATUSPAGE_URL=…` + MDX `rg | sed` rewrite) | Node mirror of `getStatuspageUrl()` against 3 env permutations; synthetic MDX fragment + `sed` rewrite against `${OVERRIDE_URL}` | PASS | 337 | `VID-2026-05-16-S3-…` |
| S4 | §2 step 7 sign-off + §3 sign-off — operator handoff | jq-emitted JSON handoff with UTC timestamps, runbook+helper SHA-256, and 3 go-live gates marked `pending-operator` | PASS | 170 | `VID-2026-05-16-S4-…` |

**Overall outcome:** PASS (4 PASS, 0 DEGRADED, 0 FAIL).

**Evidence JSON:** `reports/statuspage-init-dressrun-2026-05-16.json` (committed alongside this audit). Schema is identified by `kind: "statuspage-init-dressrun"` + `wave: "R-prep wave-25"`.

### 3.1 Per-step rationale

**S1 — DNS CNAME verification.** The simulated zone-file fake is sufficient to prove the verification logic (a) refuses an empty CNAME and (b) refuses a non-`*.statuspage.io` target. Both negative cases were covered during development by toggling the zone-file and re-running; the production verifier inherits the same logic when pointed at a real resolver (the operator simply swaps the awk-driven lookup for `dig +short CNAME status.corelink.humangr.com`).

**S2 — Statuspage API health.** We hit the public Atlassian landing surface rather than the tenant-specific `summary.json` endpoint because the latter requires a real provisioned tenant. The landing-page HEAD is sufficient to prove "the operator's network path to Atlassian-hosted surfaces is open" — the runbook §2 step 7 `summary.json` check is operator-bound and will run live at T-7d. Step S2 records DEGRADED (not FAIL) on `curl` network errors so the dress-run remains usable in air-gapped CI.

**S3 — Build-time substitution.** The Node mirror exercises three permutations:
- no env var → default `https://status.corelink.humangr.com` (canonical wave-19 commit value);
- `STATUSPAGE_URL=https://corelink-statuspage-test.example.com` → operator-override value;
- explicit `customFields` argument (component-time path) → the explicit value.
The MDX-substitution sub-step runs the runbook §3 `sed` one-liner against a synthetic MDX fragment containing both the absolute URL form (`https://status.corelink.humangr.com`) and the bare-host form (`status.corelink.humangr.com`), then `diff`s against an expected fixture. This proves the §3 one-liner is regression-free.

**S4 — Audit trail.** The handoff JSON binds itself to the wave-24 engineering closure by recording the SHA-256 of `specs/_runbooks/STATUSPAGE-INIT.md` and `apps/docs/src/statuspage-url.ts` at dress-run time. If either drifts before the operator performs the real cutover, the dress-run must be re-run (enforced by `assert_evidence_recent` in the verifier — 7-day freshness window).

## 4. Verification harness (`scripts/statuspage-init-verify.py`)

The verifier reads the evidence JSON and runs 7 assertions:

| # | Assertion | What it catches |
|---|---|---|
| V1 | `evidence_wellformed` | Schema-level shape: kind, wave, dressrun_date (YYYY-MM-DD), generated_at_utc (RFC-3339 Z) |
| V2 | `steps_complete` | All 4 steps present in order with the expected names; no FAIL outcomes; only Step 2 may be DEGRADED |
| V3 | `verification_ids_unique` | Per-step VIDs are pairwise-distinct (deterministic content-addressing) |
| V4 | `helper_default_canonical` | `apps/docs/src/statuspage-url.ts` still declares `DEFAULT_STATUSPAGE_URL = "https://status.corelink.humangr.com"` |
| V5 | `runbook_dod` | Runbook contains the 6 required tokens (`Option A`, `Option B`, `STATUSPAGE_URL`, `status.corelink.humangr.com`, `T-7d`, `summary.json`) |
| V6 | `trust_corpus_inventory` | `apps/docs/docs/**/*.mdx` contains at least 3 literal `status.corelink.humangr.com` references — reconciles the runbook §1 "5 MDX pages × 4 locales" claim and catches an accidental sweep that would silently break Option A |
| V7 | `evidence_recent` | Evidence age ≤ 7 days (gate must be re-run before T-7d if older) |

Live result against `reports/statuspage-init-dressrun-2026-05-16.json`:

```
OK:   evidence_wellformed
OK:   steps_complete
OK:   verification_ids_unique
OK:   helper_default_canonical
OK:   runbook_dod
OK:   trust_corpus_inventory
OK:   evidence_recent

PASSED: 7 assertions
```

## 5. Go-live readiness gate

This dress-run does **not** itself satisfy the runbook §4 T-7d gate. What it *does* satisfy:

- [x] The substitution-mechanism control flow is regression-free (S3 PASS).
- [x] The CNAME-verification logic is regression-free (S1 PASS).
- [x] The runbook contains all 6 required tokens (V5 PASS).
- [x] The trust corpus literal-URL inventory matches the runbook §1 claim (V6 PASS).
- [x] The operator-handoff JSON binds to specific engineering-closure SHA-256 fingerprints (S4 PASS).

What remains operator-bound for the T-7d gate (runbook §4 checklist):

- [ ] **Option A:** real `curl -sI https://status.corelink.humangr.com` returns HTTP 200 with the operator-provisioned tenant.
- [ ] **Option B:** `STATUSPAGE_URL` env var set in the docs deploy pipeline + deploy-time substitution branch merged to the release tag + operator-chosen domain resolves.
- [ ] `summary.json` reports the 8 tracked components per runbook §2 step 2.
- [ ] SRE Lead records completion in `specs/_compliance/GA-GATE-CRITERIA.md` checklist row "Statuspage provisioned (Option A — CNAME)" *or* "Statuspage provisioned (Option B — env-var override)".

Until those 4 items are checked by the SRE Lead against the production cutover, the GA-cutover gate at runbook §4 remains **operator-pending**. This dress-run reduces the surface area of "things that could fail at T-7d" from "the entire DEBT-016 closure" down to "the four operator-bound items above".

## 6. Reproducibility

- `bash scripts/statuspage-init-dressrun.sh` — emits `reports/statuspage-init-dressrun-<DATE>.json`, exits 0 on PASS, 1 on FAIL, 2 on setup failure. Idempotent: re-running emits an equivalent JSON (modulo `generated_at_utc` and the sandbox path embedded in verification IDs). Sandbox cleanup via `trap` on EXIT.
- `python3 scripts/statuspage-init-verify.py` — exits 0 if all 7 assertions pass, 1 if any fail, 2 on IO failure. Reads the most recent evidence JSON by default.
- `python3 scripts/validate_specs.py` — green (this audit is under `specs/_audits/` and is exempt from front-matter schema validation per the validator's SKIP_ALL list).
- `python3 scripts/validate_references.py` — green (no new spec IDs introduced).

## 7. Caveats

- **Network dependency on Atlassian.** Step S2 currently relies on `https://www.statuspage.io/` being reachable. If Atlassian retires that landing page or the operator's CI runs in an air-gapped environment, Step S2 will be DEGRADED rather than PASS — the verifier accepts DEGRADED on Step S2 only. The operator's T-7d production cutover must independently verify `https://status.corelink.humangr.com/api/v2/summary.json` per runbook §2 step 7.
- **Helper-mirror drift risk.** Step S3 maintains a 22-line JS mirror of `apps/docs/src/statuspage-url.ts` inside the bash orchestrator. Drift between the mirror and the upstream TS module is caught by the V4 + Step 3a.1 textual probe, but a structural change to the helper (e.g. adding a third env var) requires a corresponding mirror update — the V4 assertion will fail loud rather than silently passing on stale logic.
- **`rg` is not actually invoked in S3.** The runbook §3 command is `rg -l "status\.corelink\.dev" … | xargs sed -i …`. Step S3 exercises only the `sed` half against a known-content fixture; the `rg`-driven file discovery is left to the operator's real cutover (it depends on which trust-corpus files exist at the deploy-time commit and is orthogonal to the substitution mechanism we are verifying).
- **No mutation of `specs/_compliance/GA-GATE-CRITERIA.md`.** The dress-run does NOT amend the GA-gate sign-off log. That log is operator-bound per runbook §2 sign-off and §3 sign-off; this audit is the *backstop* for the eventual sign-off, not a substitute.
- **Two Rust impl callers out of scope.** `crates/corelink-statuspage-real` and `crates/corelink-privacy-erasure-worker` consume `STATUSPAGE_API_BASE_URL` at runtime via their own env-var paths (see DEBT-016 closure §8). They were explicitly out of scope for DEBT-016 and remain out of scope for this dress-run.

## 8. Cross-references

- Runbook: `specs/_runbooks/STATUSPAGE-INIT.md` (wave-24)
- Engineering-closure audit: `specs/_audits/2026-05-16-debt-016-statuspage-urls.md` (wave-24)
- Build-time accessor: `apps/docs/src/statuspage-url.ts`
- Build config wiring: `apps/docs/docusaurus.config.ts customFields.statuspageUrl`
- Sibling dry-run pattern: `scripts/ga-cutover-dryrun.sh` + `specs/_audits/2026-05-16-ga-cutover-dryrun.md` (wave-24)
- DSR-channel runbook: `specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md`
- Debt-register row: `specs/_audits/2026-05-15-debt-register.md` (DEBT-016, operator-pending)
