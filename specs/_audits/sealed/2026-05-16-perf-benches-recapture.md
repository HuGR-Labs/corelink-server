---
id: "AUDIT-PERF-BENCHES-RECAPTURE-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "R-prep wave-30 stream-5"
parent_wi: "R-PREP-PERF-BENCHES-RECAPTURE"
owner: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "performance", "baseline", "ga-freeze", "wave-30", "criterion", "recapture"]
---

# Wave-30 stream-5 — Perf benches recapture (wave-29 stream-9 follow-up)

> **doc_status:** REVIEW · **scope:** complete the eight `pending-recapture`
> rows left over from `2026-05-16-perf-baseline-ga-freeze.md` §2 by
> running each crate's criterion bench end-to-end under `--quick` mode
> and persisting the numeric fields into the per-bench JSON files and
> the umbrella manifest.
>
> **Base commit:** `main @ 04f2dff` (post wave-29 perf-baseline-ga-freeze
> merge). The umbrella tag remains `perf-baseline-ga-2026-05-16` —
> this audit does **not** rotate the tag; it back-fills the numeric
> rows under it.

---

## 1. Why this audit exists

Wave-29 stream-9 captured the canonical pre-GA performance baseline
freeze (audit `2026-05-16-perf-baseline-ga-freeze.md`). The laptop
runner under the 60-minute stream cap completed end-to-end runs of
three CRITICAL benches (`derive_prefix_v2`, `blake3`,
`jcs_canonicalize`) but cold-compile time consumed ~50 of the 60
minutes, leaving the other eight benches as `pending-recapture`
(§2 of the wave-29 audit, §7 follow-up bullet 2).

Wave-30 stream-5 picks up those eight rows on a fresh laptop
budget (75 min) with two efficiency moves:

1. **Shared `CARGO_TARGET_DIR`** — point all eight bench runs at a
   single tmpfs-backed directory so the cold compile cost amortizes
   across benches that share most workspace dependencies (`tokio`,
   `serde`, `proptest`, `criterion` itself, etc).
2. **Sequential ordering by expected cold-compile cost** —
   `corelink-tier-selection` (smallest fan-in) ran first to seed
   the shared deps cache, followed by `corelink-signup`,
   `corelink-byok`, `corelink-tenant-path`, `corelink-hash`,
   `corelink-audit-chain`, `corelink-dpa-acceptance`, and
   `corelink-stripe-real`.

This back-fill closes the wave-29 follow-up bullet #2 and brings the
manifest from 3-of-11 to 11-of-11 numeric rows populated, all under
the same `perf-baseline-ga-2026-05-16` umbrella tag.

---

## 2. Methodology

### 2.1 Tooling

Same as wave-29 stream-9 §1.1 — `criterion 0.5` with `--quick`
mode (~10 samples, ≤1s wall-clock per bench). Criterion `--quick`
returns a 3-value `[lower / point estimate / upper]` confidence
band on the median; per the wave-29 convention this audit persists:

- `median_ns` = point estimate (middle value of the band)
- `mean_ns`   = same as median (criterion `--quick` does not split
  these; wave-29 precedent)
- `p99_ns`    = upper 95%-CI bound of the median (proxy for true
  per-iteration p99; canonical `--measurement-time 5` recapture
  remains an operator follow-up per wave-29 §5 and §7)
- `sample_count` = `null` (criterion `--quick` does not surface
  the iteration count in `estimates.json`)

### 2.2 Run command

```
CARGO_TARGET_DIR=/tmp/perf-recapture-target \
  cargo bench -p <crate> --bench <bench> -- --quick
```

### 2.3 Why `--quick` over `--measurement-time 5`

Inherited from wave-29 §1.3: the wave-22 split-threshold gate
operates on **p99 deltas**, not absolute numbers, with budgets
(5 % CRITICAL / 15 % NON_CRITICAL) well above the ≤2 % run-to-run
jitter documented for `--quick` on the staging linux runner.
The on-runner `--measurement-time 5` canonical refresh remains
operator-bound; this audit's job is to back-fill the wave-22
schema so the gate has a numeric comparison row for every bench.

---

## 3. Per-bench results

All eight benches were run sequentially on a single laptop
between 14:09Z and 14:38Z UTC on 2026-05-16. The numeric fields
below match what was persisted into the per-bench JSON files
(`reports/perf/baseline-<crate>-<bench>.json`) and the umbrella
manifest (`reports/perf/baseline-ga-2026-05-16-365dd38.json`).

> Format: `median (ns)` · `p99 proxy (ns)` · representative input.
> Multi-input rows are captured fully in the umbrella manifest.

| #  | Crate                     | Bench                   | Class        | Representative input               | median (ns)  | p99 proxy (ns) |
|----|---------------------------|-------------------------|--------------|------------------------------------|--------------|----------------|
| 1  | corelink-tenant-path      | derive                  | CRITICAL     | `derive_prefix`                    | 1 666.5      | 1 682.3        |
| 4  | corelink-hash             | blake3_bench            | CRITICAL     | `Digest::compute/1024 KiB`         | 523 770.0    | 529 030.0      |
| 5  | corelink-audit-chain      | merkle_append           | CRITICAL     | `audit_chain/append_single`        | 19 694.0     | 19 717.0       |
| 7  | corelink-dpa-acceptance   | accept_and_verify_jwt   | CRITICAL     | `dpa/verify_receipt_rs256`         | 46 630.0     | 46 799.0       |
| 8  | corelink-byok             | envelope_roundtrip      | NON_CRITICAL | `byok/wrap_unwrap_roundtrip`       | 17 757.0     | 17 780.0       |
| 9  | corelink-signup           | orchestrator            | NON_CRITICAL | `signup/provision_new`             | 41 626.0     | 41 920.0       |
| 10 | corelink-tier-selection   | select                  | NON_CRITICAL | `tier_selection/select_starter`    | 10 272.0     | 10 766.0       |
| 11 | corelink-stripe-real      | webhook_verify          | NON_CRITICAL | `stripe_webhook/verify/4096B`      | 26 599.0     | 27 152.0       |

(Bench numbering matches the 11-row umbrella manifest; rows 2 / 3 / 6
were captured in wave-29 stream-9 and are unchanged here.)

### 3.1 Multi-row benches — full capture into the umbrella manifest

Five of the eight benches expose multiple sub-rows (criterion
sub-benchmarks); the umbrella manifest captures every sub-row,
while the per-bench JSON file persists the representative row
chosen for the gate. The per-bench `notes` field names the sibling
rows for traceability.

- `corelink-hash::blake3_bench` → 6 rows (`1 KiB`, `64 KiB`,
  `1024 KiB`, `5120 KiB`, `VerifiedBody::new (1 MiB, ok)`,
  `Digest::verify_constant_time`). Representative: `1024 KiB`
  (mirrors the sibling `blake3` bench's `1MiB` row).
- `corelink-audit-chain::merkle_append` → 2 rows (`append_single`,
  `append_10k/sequential`). Representative: `append_single`.
- `corelink-dpa-acceptance::accept_and_verify_jwt` → 2 rows
  (`accept_rs256_sign`, `verify_receipt_rs256`). Representative:
  `verify_receipt_rs256` (the verification path is the hot p99
  path on `accept_and_verify_jwt`).
- `corelink-byok::envelope_roundtrip` → 3 rows (`wrap_dek`,
  `unwrap_dek`, `wrap_unwrap_roundtrip`). Representative:
  `wrap_unwrap_roundtrip` (full envelope path).
- `corelink-signup::orchestrator` → 2 rows (`provision_new`,
  `provision_idempotent_replay`). Representative: `provision_new`
  (cold-path orchestration; replay is sub-µs).
- `corelink-tier-selection::select` → 2 rows (`select_free`,
  `select_starter`). Representative: `select_starter` (worst-case).
- `corelink-stripe-real::webhook_verify` → 5 rows (`verify/256B`,
  `verify/1024B`, `verify/4096B`, `verify/16384B`,
  `compute_signature_4KiB`). Representative: `verify/4096B`
  (typical Stripe webhook payload size).

### 3.2 Bench compile/run wall-clock log

| #  | Bench                     | Compile (mm:ss) | Run | Notes                                  |
|----|---------------------------|-----------------|-----|----------------------------------------|
| 10 | tier_selection::select    | 4:21            | <2s | Cold start; seeds shared deps cache    |
| 9  | signup::orchestrator      | 3:18            | <2s | Reused deps cache                       |
| 8  | byok::envelope_roundtrip  | ~2:30           | <2s | Reused deps cache                       |
| 1  | tenant-path::derive       | 5:51            | <2s | New `bytes`/`base64` link surface       |
| 4  | hash::blake3_bench        | <0:30           | ~5s | Mostly cached from wave-29 blake3 bench |
| 5  | audit-chain::merkle_append| 3:16            | ~6s | append_10k sequential is the long row   |
| 7  | dpa::accept_and_verify_jwt| 1:42            | ~5s | jsonwebtoken/rsa compile incremental    |
| 11 | stripe::webhook_verify    | 1:54            | ~6s | sha2/hmac mostly cached                 |
|    | **Total run window**      | **~29 min**     |     |                                         |

The 75-minute time budget for the stream was respected with ~46
minutes of headroom left for the artefact writes (per-bench JSONs,
umbrella manifest, this audit, cross-refs).

---

## 4. Sanity check vs class budgets

The wave-22 split-threshold gate budgets a **5 % p99 regression**
for CRITICAL benches and **15 % p99** for NON_CRITICAL benches.

Every wave-30-recaptured row shows a p99-proxy / median ratio
≤ ~5 % (CRITICAL hot paths) and ≤ ~5 % (NON_CRITICAL hot paths
sampled here), comfortably inside the gate budget headroom. No
anomalous outliers and no readings that would indicate a hot-path
regression vs the wave-29 measurements of rows 2, 3, 6.

Spot-check ratios (p99-proxy / median, lower is better):

| Bench                                      | Class        | ratio  |
|--------------------------------------------|--------------|--------|
| corelink-tenant-path::derive               | CRITICAL     | 1.009  |
| corelink-hash::blake3_bench (1024 KiB)     | CRITICAL     | 1.010  |
| corelink-audit-chain::merkle_append        | CRITICAL     | 1.001  |
| corelink-dpa-acceptance::accept_and_verify | CRITICAL     | 1.004  |
| corelink-byok::envelope_roundtrip          | NON_CRITICAL | 1.001  |
| corelink-signup::orchestrator              | NON_CRITICAL | 1.007  |
| corelink-tier-selection::select_starter    | NON_CRITICAL | 1.048  |
| corelink-stripe-real::webhook_verify/4096B | NON_CRITICAL | 1.021  |

The `select_starter` 4.8 % spread is the widest in the set and
remains well under the 15 % NON_CRITICAL budget; it reflects the
`--quick` mode's small-sample variance, not a real-world hot-path
regression.

---

## 5. Cross-ref: wave-29 follow-up closure

The wave-29 audit (`2026-05-16-perf-baseline-ga-freeze.md`) §7
operator follow-up bullet 2 reads:

> If this stream's bench run did not populate all 11 numeric rows
> (laptop wall-clock cap), re-run via `scripts/refresh-perf-baseline.sh`
> on a green main-branch CI and commit the refresh under the same
> tag-rotation rule used in wave-22 §5.

**Wave-30 stream-5 closure:** numeric rows for all 11 benches are
now populated under the same `perf-baseline-ga-2026-05-16` tag.
The CI canonical refresh (`scripts/refresh-perf-baseline.sh` with
`--measurement-time 5`) remains a follow-up for a future tag
rotation; the recapture here delivers the gate-ready numbers under
`--quick` for the GA-day +24h comparison.

Updated wave-29 audit `§7` bullet 2 status: **CLOSED-WAVE-30**
(see §7 of this audit for the back-link). The other two bullets
(tag creation; runbook update) remain as wave-29 follow-ups.

---

## 6. GA-baseline health verdict

- All 11 manifest rows are populated.
- p99-proxy / median ratios are within class budget headroom.
- No anomalous outliers indicative of hot-path regression.
- Schema continuity preserved: same per-bench JSON shape, same
  umbrella manifest shape, same gate-comparison contract.

**Verdict:** GA-baseline is healthy. Tag application of
`perf-baseline-ga-2026-05-16` on the current main tip is
authorized by the recapture stream operator (Gustavo Schneiter,
wave-30 stream-5 task spec).

The tag-application operator command (executed under the
tag-rotation rule from wave-22 §5) is:

```
git tag -a perf-baseline-ga-2026-05-16 <main-tip-sha> \
  -m "GA performance baseline freeze (wave-29 + wave-30 recapture)"
git push origin perf-baseline-ga-2026-05-16
```

Operator execution remains tag-rotation policy bound (push from
the operator's signed workstation, not from the worktree agent).

---

## 7. Operator follow-ups

- [x] Re-run the eight pending-recapture benches and populate the
      numeric rows in the umbrella manifest and per-bench JSONs.
      **(This audit; closes wave-29 §7 bullet 2 as CLOSED-WAVE-30.)**
- [ ] Apply the `perf-baseline-ga-2026-05-16` git tag on the merge
      commit of this stream (operator-bound; authorized §6 above).
- [ ] CI canonical refresh: run
      `scripts/refresh-perf-baseline.sh --measurement-time 5` on
      a green main-branch CI run and commit the canonical p99
      numbers under a future tag rotation. (Inherits wave-29 §5.)
- [ ] Update `RB-PERF-REGRESSION.md` triage flow to name the
      new tag as the comparison point for D-day +24h (inherits
      wave-29 §7 bullet 3, unchanged).

---

## 8. Cross-references

- `specs/_audits/2026-05-16-perf-baseline-ga-freeze.md` (wave-29 stream-9 — superseded numeric rows; §7 bullet 2 closure)
- `specs/_audits/2026-05-16-perf-regression-ci-tightened.md` (wave-22)
- `reports/perf/baseline-ga-2026-05-16-365dd38.json` (umbrella manifest)
- `reports/perf/baseline-ga-diff-vs-wave22.md` (machine-readable diff)
- `reports/perf/baseline-corelink-tenant-path-derive.json`
- `reports/perf/baseline-corelink-hash-blake3_bench.json`
- `reports/perf/baseline-corelink-audit-chain-merkle_append.json`
- `reports/perf/baseline-corelink-dpa-acceptance-accept_and_verify_jwt.json`
- `reports/perf/baseline-corelink-byok-envelope_roundtrip.json`
- `reports/perf/baseline-corelink-signup-orchestrator.json`
- `reports/perf/baseline-corelink-tier-selection-select.json`
- `reports/perf/baseline-corelink-stripe-real-webhook_verify.json`
- `.github/workflows/perf-regression.yml`
- `specs/_runbooks/RB-PERF-REGRESSION.md`
- `docs/internal/PERFORMANCE-PLAYBOOK.md` §"How regression gates work"
