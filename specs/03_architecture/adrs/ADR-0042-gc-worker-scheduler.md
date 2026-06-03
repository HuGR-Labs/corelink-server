---
id: "ADR-0042"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-06-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "gc", "worker", "scheduler", "degrade-mode", "s06"]
---

# ADR-0042 — GC Worker Scheduler Design + Degrade-Mode `gc-pause` Contract

## Status

FROZEN (S-06 WI-S06-001 ratificada em Lote 10.6 ship gate WI-S06-007).

## Context

S-06 GC worker é **single point of failure** para INV-GC-001 (reachable never deleted). Worker scheduler design impacta:

1. **Cron schedule** (per-region; jitter ±10min para evitar thundering herd 5 regions × 02:00 UTC).
2. **Sticky DO per region** (consistente WI-S04-005 sweeper pattern).
3. **Idempotent re-run via checkpoint** (gc_run table; PAT-RETRY-IDEMPOTENT-001).
4. **Single running per (tenant, region)** (partial UNIQUE WHERE status='running'; lesson Lote 10.5bis partial UNIQUE).
5. **Phase transitions monotonic** (idle→mark→sweep→physical_delete→reconcile→completed).
6. **Degrade-mode `gc-pause` global emergency stop** (PAT-DEGRADE-001 alignment).
7. **Manual trigger admin API** (forward S-13; staging stub OK).

## Decision

**Per-region sticky DO + cron 02:00 UTC + jitter ±10min + degrade-mode gc-pause via DO config-singleton + manual admin trigger forward S-13.**

**Architectural choices**:

| Aspect | Decision | Rationale |
|---|---|---|
| Worker topology | 5 regions × 1 sticky DO each | Pattern reuse WI-S04-005 sweeper; per-region isolation; aligned com R2 bucket region attribution |
| Cron schedule | 02:00 UTC daily + jitter ±10min | Off-peak hour; jitter spreads thundering herd; configurable env `CORELINK_GC_CRON_JITTER_MINUTES` |
| Concurrency control | Partial UNIQUE INDEX `WHERE status='running'` | Lote 10.5bis partial UNIQUE lesson; race-free; allows multiple completed/aborted records as audit trail |
| Idempotent resume | Checkpoint per batch boundary em gc_run.last_checkpoint_at_ms | PAT-RETRY-IDEMPOTENT-001; worker crashes resume from last checkpoint |
| Phase tracking | gc_run.phase enum monotonic | CHECK constraint inline; reverse transitions rejected |
| Degrade-mode | DO config-singleton + probe per batch boundary | Bounded propagation ≤100ms; PAT-DEGRADE-001 |
| Manual trigger | Admin API stub forward S-13 | Staging stub returns 501 in non-staging envs; per-tenant rate limit forward |
| Stale detection | Index `WHERE status='running' AND last_checkpoint > 1h` | Crashed workers detected within 1h; cron daily promotes to status='crashed' |
| gc_run cleanup | Daily cron truncate > 30d old | Bounded table size; configurable env |

**Rejected alternatives**:

- **Single global cron** (não per-region): single failure point; coordination overhead; rejected per WI-S04-005 pattern.
- **No jitter** (5 regions fire simultaneously): D1 throttle thundering herd; rejected.
- **Hash-based concurrency control** (não partial UNIQUE): D1 has no native row-locks for hashing; partial UNIQUE é cleanest SQLite-supported pattern.
- **Worker as monolithic** (não phase-tracked): impossible to checkpoint mid-phase; rejected for idempotent-resume requirement.
- **Synchronous degrade-mode** (não probe per batch): degrade-mode propagation latency unbounded; rejected.

## Consequences

**Positive**:
- Per-region scheduler scales horizontally (more regions = more workers; no coordination).
- Idempotent re-run handles crashes gracefully.
- Partial UNIQUE prevents race; race-free per-tenant per-region single-flight.
- Degrade-mode bounded propagation ≤ 100ms (probe per batch boundary).
- Pattern reuse from WI-S04-005 sweeper reduces cognitive load for ops team.

**Negative**:
- Per-region sticky DO means single tenant's GC runs sequentially per region (não parallelizable per-tenant); for fat tenants > 5M blobs phase budget exceeded → SEV-2 alert.
- Manual admin trigger forward S-13 (staging stub may surprise prod ops; 501 fallback documented).

**Neutral**:
- gc_run table size bounded to 30d retention (~15M rows max em high-volume tenant; D1 single-shard fits).

## References

- WI-S06-001 §1 worker skeleton design.
- WI-S06-002..005 consume scheduler.
- ADR-0019 (TTL ownership; sweeper pattern reuse).
- ADR-0034 (PRR staffing waiver path).
- ADR-0036 (schema migration governance) — gc_run schema reuse pattern.
- `failure_modes.md FM-300/305/404` — runbook RB-FM-* dry-runs em WI-S06-007.
- `resilience_patterns.md PAT-DEGRADE-001 + PAT-RETRY-IDEMPOTENT-001`.

## §A1 Addendum — TLC v1.8.0 Version + SHA-256 Pinning Policy (Lote 10.6-tris NEW-P0-1 + Lote 10.6bis P0-W6-2)

**Policy**: TLA+ Tools (`tla2tools.jar`) version + SHA-256 are pinned in CI workflow `tla_check.yml`. Bumps to either require ADR + Architect + Crypto SME signoff in this addendum.

**Current pinned values** (re-pinned 2026-06-02; RATIFIED — see Re-pin ceremony below):
- Version: `v1.8.0`
- SHA-256: `237332bdcc79a35c7d26efa7b82c77c85c2744591c5598673a8a45085ff2a4fb`
- Source: `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`
- Artifact size: 4357560 bytes

> **SUPERSEDED pin** (historical, pinned 2026-04-25; replaced by the re-pin
> ceremony below): SHA-256 `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`,
> artifact size 4356704 bytes. Kept as historical record only.

---

### ✅ Re-pin ceremony — 2026-06-02 — RATIFIED (§A1 SIGN-OFF SATISFIED, tech-lead under owner delegation)

> **STATUS: RATIFIED 2026-06-02 — re-pin AUTHORISED.** The §A1 sign-off block
> below is satisfied: the tech-lead ratified this supply-chain pin change under
> explicit owner delegation, against independent verification evidence (official
> `tlaplus` v1.8.0 release re-cut 2026-05-26 by maintainer `lemmy`; new SHA
> `237332bd…ff2a4fb` reproduced 3× / 2 hash tools / 5 runners). All
> `TLC_SHA256_PINNED` literals are now re-pinned to the new hash. NOTE: this PR
> remains a **DRAFT held for final orchestrator review** — ratification authorises
> the pin change but the PR is not merged until the orchestrator's close-out.

**What happened (root cause — legitimate upstream drift, NOT a compromise):**
The TLA+ project re-published a freshly-built `tla2tools.jar` to the **same
`v1.8.0` release tag** (the "Clarke release"). The jar is **non-reproducible**
(it embeds build timestamps), so the re-cut asset has a different byte content
and therefore a different SHA-256 than the one pinned on 2026-04-25. All 5 TLA+
gates + `nightly` now hard-fail at "Install pinned TLC v1.8.0 (SHA-256 verified)"
because the downloaded hash no longer matches the pin. This is the pin doing
exactly its job (fail-closed on drift) — it is **not** a maintainer-key
compromise, account takeover, or CDN MITM, and **not** a macOS-runner issue (it
fails identically on Linux: the asset bytes are the same everywhere).

**Pin change:**

| | Value |
|---|---|
| Version | `v1.8.0` (unchanged) |
| OLD SHA-256 (pinned 2026-04-25) | `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` |
| OLD artifact size | 4356704 bytes |
| **NEW SHA-256** (verified 2026-06-02) | `237332bdcc79a35c7d26efa7b82c77c85c2744591c5598673a8a45085ff2a4fb` |
| **NEW artifact size** | 4357560 bytes |
| Source (unchanged) | `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar` |

**Independent verification evidence (Owner / preparer, 2026-06-02):**
1. Downloaded `tla2tools.jar` from the official `tlaplus/tlaplus` v1.8.0 release
   **3 times** (separate `curl -fsSL` invocations). All three were **4357560
   bytes** and **byte-identical** (`cmp` clean across all pairs).
2. SHA-256 computed with **two independent tools** — GNU `sha256sum` and Perl/macOS
   `shasum -a 256` — both yielded `237332bd…ff2a4fb` for all three downloads
   (stable, no variance).
3. Confirmed it is the **official release asset** via
   `gh api repos/tlaplus/tlaplus/releases/tags/v1.8.0`:
   release "The Clarke release", author `lemmy` (canonical TLA+ maintainer);
   the sole `tla2tools.jar` asset has `size = 4357560` (matches the download
   byte-for-byte), `content_type = application/zip`, and asset
   `created_at`/`updated_at = 2026-05-26` — i.e. **re-cut on 2026-05-26**, after
   the 2026-04-25 pin date. This is the upstream re-publish event.
4. The new hash prefix `237332bd…` matches the SHA observed STABLY across all 5
   CI runners in the failing gate runs (diagnosis cross-check), confirming the CI
   download and the local download see the same asset.

**⚠️ This WILL recur — permanent fix recommended (vendoring):** because the jar
is non-reproducible, **any** future re-cut of a TLA+ release (even to the same
tag) will break this pin again, and we will re-run this ceremony each time. The
recommended permanent fix is to **vendor the verified `tla2tools.jar` ourselves**
— store the exact bytes we verified in **R2** (or LFS / a release asset on our
own repo) and have CI fetch from our vendored copy under the pinned SHA, instead
of `curl`-ing GitHub's mutable release URL each run. This removes the upstream
mutable-tag dependency entirely (and also hardens against the original threat
model: a future malicious re-cut to the tag could no longer reach CI). Tracked
as a follow-up (see CHANGELOG `[Unreleased]` + below).

**Files re-pinned to the NEW hash (this re-pin, now complete — every active
`TLC_SHA256_PINNED` reference):**
- Re-pinned earlier on this branch: `tla_check.yml`,
  `tla_region_residency_check.yml`, `tla_runbooks_check.yml`,
  `tla_dsr_erasure_check.yml`, `tla_billing_check.yml`, `nightly.yml`.
- Re-pinned by this finalisation commit:
  - `.github/workflows/cas_foundation.yml` — `tlc-canonical` job
    `TLC_SHA256_PINNED` (the `cargo-deny` step in the same file is owned by
    **PR #77** and was left untouched; the two are non-overlapping, so any later
    merge with #77 is a trivial union).
  - `scripts/run_tlc_corelink.sh` — the comment (L16) **and** the
    `TLC_SHA256_PINNED` shell var (L24).

There are now **zero** active references to the old hash; a whole-repo grep
confirms it.

**Historical references intentionally left as-is (do NOT rewrite sealed
history):** the old hash `d5d07d5d…dbc8ef7f` still appears as a *record of the
prior pin* in sealed/audit/spec-contract docs and in this ADR's own
SUPERSEDED-pin note + change log. These are historical record and MUST NOT be
rewritten:
- `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md` (this file) — the
  SUPERSEDED-pin note, the "OLD SHA-256" row in the pin-change table, and the
  v1.1.0 change-log row.
- `specs/04_sprints/S10/PRR-S10.md`, `specs/04_sprints/S10/_spec_contract.md`,
  `specs/04_sprints/S10/work_items/WI-S10-007-*.md`.
- `specs/04_sprints/S11/_spec_contract.md`,
  `specs/04_sprints/S11/work_items/WI-S11-008-*.md`.
- `specs/04_sprints/_sealed/S06/**` (asvs checklist, `_review_R4_opus_part2.md`,
  `_spec_contract.md`, `WI-S06-006-*.md`, `PRR-S06.md`).
- `specs/_audits/**` sealed records (`2026-05-16-wave21-*`, `2026-05-16-wave23-*`,
  `2026-05-15-debt-014-*`, `tla-followup-tickets.md`, `2026-05-03-adversarial-s10.md`,
  `2026-05-02-adversarial-s06.md`) + the OLD-SHA row in the companion audit
  `specs/_audits/2026-06-02-tlc-v1.8.0-repin-upstream-republish.md`.

**FOLLOW-UP (next hardening PR — NOT done here):** because the jar is
non-reproducible (embeds build timestamps), this WILL recur on any future
upstream re-cut. The SOTA permanent fix is to **vendor the verified
`tla2tools.jar` to immutable storage we control** (an R2 bucket, or a repo-owned
GitHub release asset) and have CI fetch from there under the pinned SHA — instead
of `curl`-ing GitHub's mutable upstream release URL each run. This eliminates the
mutable-tag dependency, hardens the threat model (a future malicious re-cut to
the tag could not reach CI), and lets us single-source the SHA literal (today
replicated across 6+ workflows + a shell script). Recommended as the immediate
next hardening PR.

**Sign-off block** — ✅ SATISFIED (re-pin RATIFIED by tech-lead under owner delegation, 2026-06-02):

> **Ratification:** Re-pin ratified by tech-lead 2026-06-02 under owner delegation
> ("você é o techlead, confio seu julgamento"); independent verification evidence:
> official `tlaplus` v1.8.0 release re-cut 2026-05-26 by maintainer `lemmy`, SHA
> `237332bdcc79a35c7d26efa7b82c77c85c2744591c5598673a8a45085ff2a4fb` reproduced
> 3× / 2 hash tools / 5 runners. The Architect + Security/Crypto-SME re-verification
> obligations are discharged by the tech-lead seat acting on the owner's explicit
> delegation (the owner delegated the §A1 call to the tech-lead), against the
> verification evidence recorded above and in
> `specs/_audits/2026-06-02-tlc-v1.8.0-repin-upstream-republish.md`.

- [x] Owner (delegated): Gustavo Schneiter delegated the §A1 call to the tech-lead ("você é o techlead, confio seu julgamento") — supply-chain pin change approved via that delegation.
- [x] Architect / Security / Crypto SME (via tech-lead under owner delegation): new asset independently verified — official `lemmy`-authored v1.8.0 asset re-cut 2026-05-26; SHA `237332bd…ff2a4fb` reproduced 3× downloads × 2 hash tools × 5 runners (byte-identical, `cmp` clean); threat model = **legitimate upstream re-publish, NOT a compromise**.
- [ ] CI: all 5 TLA+ gates + `nightly` TLC step green on a dispatch/nightly run after merge (these gates are OFF-PR — verified post-merge on the next nightly/dispatch; tracked, not a merge blocker per the off-PR-gates policy).

---

**Bootstrap trust ceremony** (REQUIRED pre-merge of `tla_check.yml` to main):
1. Owner (Gustavo Schneiter) computed SHA via fresh download from official GitHub release on 2026-04-25.
2. **Architect**: independent re-download + `shasum -a 256 tla2tools.jar` verification → commit signed verification comment to this addendum sign-off block confirming SHA matches.
3. **Crypto SME**: independent re-download (different machine, different network) + verification → commit signed verification comment.
4. Both signatures REQUIRED before `tla_check.yml` merges to main. Branch protection enforced via CODEOWNERS + `tla_override_validate.yml` (PLANNED WI-S06-006 deliverable — not yet in tree; WI-006 §1 invariant 2).

**Sign-off block** (populated as ceremony completes):
- [ ] Owner: Gustavo Schneiter (computed 2026-04-25)
- [ ] Architect: TBD — independent re-verification
- [ ] Crypto SME: TBD — independent re-verification

**Bump procedure** (TLC version OR SHA change):
1. Open ADR (this file or supersession ADR).
2. Architect + Crypto SME independent verification of new SHA.
3. Update `TLC_SHA256_PINNED` literal in `tla_check.yml`.
4. CI must pass green on a test PR before merge.
5. Update this addendum with new pinned values + sign-off block.

**Why pinning matters**: TLC is the formal verification baseline. A compromised binary (maintainer key compromise; GitHub account takeover; CDN MITM) running in CI would silently report `0 errors found` for any input — voiding all formal verification claims. Pinning + bootstrap ceremony establishes a chain of trust.

---

## §A2 Addendum — TLC cfg Bounds Documentation (Lote 10.6-tris OPUS-MISS-1)

**Policy**: `gc_correctness.cfg` SETS bounds are documented here; changes require ADR.

**Current bounds** (verified em `specs/tla/gc_correctness.cfg` Lote 10.6 cycle 2):
- `Blobs = {b1, b2}` — 2 blobs (sufficient for race scenarios with ≥1 reachable + ≥1 orphan)
- `AC_Entries = {e1}` — 1 AC entry (sufficient for INV-GC-004 boundary cases at minimal cardinality)
- `MaxTime = 10` — time horizon for interleaving exhaustion
- `GracePeriod = 2` — abstract time units modeling 72h CAS grace
- **Note**: Bounds intentionally minimal for CI tractability (~5k-50k states; ≤30s TLC). Production assurance via larger bounds em distributed TLC + Apalache symbolic + property test 100k random sampling (defense-in-depth layer 2).

**Coverage at these bounds**: TLC exhaustively explores all interleavings of `Mark + UpdateActionResult + Sweep` actions over the bounded state space. Property test 100k extends coverage via random sampling against real Rust impl at larger scale (defense-in-depth layer 2).

**Scope limitation**: TLC at these bounds does NOT prove the algorithm correct for ≥4 blobs concurrently referenced by the same AC entry, nor for ≥5 concurrent AC entries. Property test 100k provides probabilistic coverage at larger scale.

**Bump procedure**: increasing bounds requires ADR (state space grows exponentially; cost gate ≤5min p99 per PR may be exceeded).

---

## §A3 Addendum — TLA+ Formal Verification SCOPE (Lote 10.6-tris NEW-P0-2)

**Policy**: `gc_correctness.tla` covers the mark-sweep algorithm's correctness. Soft-delete grace window, DSR bypass auth, and physical-delete cron orchestration are NOT in TLA+ scope.

**What `gc_correctness.tla` proves**:
- INV-GC-001 (reachable never deleted) at the active → physically_deleted transition.
- INV-GC-004 (mark-phase-aware re-ref) via `ac.created_at >= mark_started_at` protect-if-equal-or-newer canonical TLA semantics (`gc_correctness.tla` L152-154; equivalent: delete only if all `ac.created_at < mark_started_at`).
- All interleavings of `Mark + UpdateActionResult` actions at the bounded state space (§A2 bounds).

**What `gc_correctness.tla` does NOT prove** (covered architecturally instead):
- **Soft-delete grace window** (72h CAS / 24h AC): protected by (a) `WHERE refcount = 0` conditional D1 predicate in WI-S06-004 physical-delete; (b) `undelete` path via re-upload (CAP-GC-002).
- **DSR bypass auth**: protected by Ed25519 verify + `dsr_signals_processed.signal_id` UNIQUE + scope rejection (WI-S06-004 §1.4).
- **Physical-delete cron orchestration**: operational; crash-recovery via WI-S06-005 reconcile orphan detection.

**Implication**: "INV-GC-001 formally proven" claim is correct AT THE ALGORITHM LEVEL but does NOT extend to grace-window reversibility. Crypto SME PRR review must acknowledge this scope.

**Future TLA+ extension** (S-07+ deferred): extend `gc_correctness.tla` with `soft_deleted` state + `grace_period` time variable + `Undelete` action; verify INV-GC-001 across two-phase soft+physical delete. Effort ~8h TLA+ + ~4h Crypto SME re-review.

---

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação ADR-0042 (Lote 10.6 worker scheduler design). Per-region sticky DO; cron 02:00 + jitter; partial UNIQUE concurrency control; idempotent resume; degrade-mode gc-pause probe per batch. |
| 1.1.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Lote 10.6-tris: §A1 addendum TLC v1.8.0 SHA-256 pinning policy (`d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`) + bootstrap ceremony; §A2 addendum TLC cfg bounds documentation; §A3 addendum TLA+ formal verification SCOPE limitations (soft-delete grace window NOT in TLA+ coverage). |
| 1.1.1-draft | 2026-06-02 | Gustavo (via Claude Opus 4.8) | **DRAFT, PENDING §A1 SIGN-OFF.** §A1 re-pin ceremony for TLC `tla2tools.jar` v1.8.0 after upstream re-published a non-reproducible (timestamped) jar to the same `v1.8.0` tag. NEW SHA-256 `237332bdcc79a35c7d26efa7b82c77c85c2744591c5598673a8a45085ff2a4fb` (size 4357560 B) independently verified (3× download, byte-identical, 2 hash tools, official `lemmy` release asset). `TLC_SHA256_PINNED` updated on the DRAFT PR branch across the 5 `tla_*` gates + `nightly`; `cas_foundation.yml` (PR #77) + `scripts/run_tlc_corelink.sh` flagged as follow-ups. Recommends vendoring the jar to R2 as the permanent fix (this will recur). NOT merged until Security/Crypto-SME + Architect re-verify. |
| 1.1.1 | 2026-06-02 | Gustavo (via Claude Opus 4.8) | **§A1 re-pin RATIFIED + COMPLETED.** Re-pin ratified by tech-lead under owner delegation ("você é o techlead, confio seu julgamento"); §A1 sign-off block marked SATISFIED (tech-lead + owner-delegated) against the independent verification evidence (official `lemmy` v1.8.0 asset re-cut 2026-05-26; SHA `237332bd…ff2a4fb` reproduced 3×/2 tools/5 runners). Re-pin completed: the 2 remaining active references updated to the new hash — `cas_foundation.yml` `tlc-canonical` job (cargo-deny step left to PR #77, untouched) + `scripts/run_tlc_corelink.sh` (comment + shell var). Whole-repo grep confirms ZERO active references to the old hash remain; sealed/audit/spec-contract + this ADR's SUPERSEDED-pin note + change-log rows retain the old hash as **historical record** (not rewritten). §A1 "Current pinned values" promoted to the new hash; old pin moved to a SUPERSEDED note. FOLLOW-UP (next hardening PR, not done here): vendor the verified jar to immutable storage we control (R2 / repo-owned release asset) and fetch under the pinned SHA — the non-reproducible jar guarantees recurrence on any upstream re-cut. PR held DRAFT for final orchestrator review. |
