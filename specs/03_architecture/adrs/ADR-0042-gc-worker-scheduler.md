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

**Enforcement (added 2026-08-02 — the policy above is no longer prose-only).**
This addendum is the SINGLE SOURCE OF TRUTH for the pin, and
`scripts/check_tlc_pin_consistency.py` fails CI whenever any carrier disagrees
with the "Current pinned values" block below, or drops its pin entirely. It runs
in `spec_validation.yml` on every PR that touches a carrier or this ADR.

That check exists because the prose version of this policy did not hold: the
2026-07-09 bump skipped this addendum, and for three weeks §A1 declared
`237332bd…` while all 8 carriers used `33de7da9…`. Nothing failed, because a
sentence in Markdown cannot fail a build. Verified by reintroducing both failure
modes (a diverged carrier, and a removed pin) and confirming the gate goes red on
each — a gate that has never been proven to fail is not a gate.

**Current pinned values** (re-pinned 2026-08-02; RATIFIED — see the 2026-08-02 ceremony below):
- Version: `v1.8.0`
- SHA-256: `e22f8ffb4bacdea0a871f444dd94fe5fb0d8013b3388ae39e82e26f852c735d5`
- Source: `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`
- Artifact size: 4486015 bytes

> **SUPERSEDED pins** (historical record only):
> - pinned 2026-04-25 — `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`, 4356704 bytes.
> - pinned 2026-06-02 — `237332bdcc79a35c7d26efa7b82c77c85c2744591c5598673a8a45085ff2a4fb`, 4357560 bytes.
> - pinned 2026-07-09 — `33de7da9ce1b7fffb9d1c184021178dbb051747be48504e65c584c423721a32e`.
>   **⚠️ POLICY GAP, recorded retroactively.** This bump shipped in commit
>   `1b05baa3` ("tlaplus re-uploaded v1.8.0 on 2026-07-09") and was **never entered
>   in this addendum**, although §A1 requires exactly that for any pin change. The
>   omission is why §A1 and CI disagreed for three weeks: this file still declared
>   `237332bd…` while every `TLC_SHA256_PINNED` literal in the workflows said
>   `33de7da9…`. A pinning policy whose record of truth silently diverges from the
>   artifact it pins provides no assurance at all — see the standing recommendation
>   at the end of the 2026-08-02 ceremony.

---

### ✅ Re-pin ceremony — 2026-08-02 — RATIFIED (§A1 SIGN-OFF SATISFIED, tech-lead under owner delegation)

> **STATUS: RATIFIED 2026-08-02 — re-pin AUTHORISED.** The owner asked the
> tech-lead to carry out the §A1 call directly and to be rigorous about it,
> consistent with the standing delegation recorded in the 2026-06-02 ceremony.
> Authorisation rests on the independent verification recorded below — **not** on
> the convenience of turning a red gate green. The pin was deliberately NOT
> updated to "whatever the download produced": a pin re-set to match its own input
> verifies nothing.

**What happened (legitimate upstream drift — NOT a compromise):** `v1.8.0` is a
**mutable tag**. Upstream has re-published `tla2tools.jar` to it at least three
times (2026-05-26, 2026-07-09, 2026-07-31). The jar is non-reproducible — its own
manifest embeds `Build-TimeStamp` — so every rebuild yields different bytes and a
different SHA-256, and the pin hard-fails by design. The asset now served is the
official v1.8.0 release published 2026-07-31T18:55:53Z.

**Independent verification performed 2026-08-02 (strongest evidence last):**

| # | Check | Result |
|---|---|---|
| 1 | Downloaded and hashed locally (`shasum -a 256`) | `e22f8ffb…c735d5` — matches what CI reported, so CI was not misreporting |
| 2 | Size vs GitHub Releases API | 4486015 B on both — no truncation or substitution in transit |
| 3 | Jar manifest identity | `Implementation-Title: TLA+ Tools`, `Implementation-Vendor: Microsoft Corp.`, `Main-class: tlc2.TLC` |
| 4 | Jar contents | 173 expected classes present (`tlc2/TLC.class`, `tla2sany/*`, `pcal/trans`) |
| 5 | Build provenance in manifest | `Built-By: runner`, `Build-TimeStamp: 2026-07-31T18:48:30Z` — 6 min before the asset's `created_at` 18:54:19, consistent with a CI build then upload |
| 6 | **Embedded `X-Git-Revision: 30cc3601321c3fc02e044d0ecb5c58d8921e18df`** | **exists in `tlaplus/tlaplus`** |
| 7 | **Tag `v1.8.0` → commit** | resolves to **exactly `30cc3601…`** — the binary is tied to public, reviewable source |
| 8 | What that commit is | `Upgrade javax.mail from 1.6.3 to 1.6.8 to fix CVE-2025-7962` — a plausible security bump, not an unexplained change |

Checks 6 and 7 carry the weight. They bind the binary to a specific public commit
rather than to a maintainer's assertion: a substituted jar would have to embed a
git revision that resolves in the upstream repository AND be the exact target of
the release tag.

**Limits of this verification — stated plainly, so the next reader does not
over-trust it.** The jar is unsigned; there is no upstream checksum file or
signature to compare against, so checks 1-2 establish integrity of *transfer*,
not authenticity of *origin*. Checks 6-8 establish that the artifact's own
metadata is consistent with public source, but a build pipeline compromise
upstream would produce exactly the same evidence. This is the strongest assurance
available for an unsigned artifact fetched from a mutable tag — which is precisely
the argument for the standing recommendation below.

**Pin change:**

| | Value |
|---|---|
| Version | `v1.8.0` (unchanged) |
| OLD SHA-256 (pinned 2026-07-09, undocumented) | `33de7da9ce1b7fffb9d1c184021178dbb051747be48504e65c584c423721a32e` |
| **NEW SHA-256** (verified 2026-08-02) | `e22f8ffb4bacdea0a871f444dd94fe5fb0d8013b3388ae39e82e26f852c735d5` |
| **NEW artifact size** | 4486015 bytes |
| Source (unchanged) | `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar` |

Updated in all 8 carriers: the five `tla_*` gates, `nightly.yml`,
`cas_foundation.yml`, and `scripts/run_tlc_corelink.sh`.

**What this outage cost:** with the pin mismatched, the install step hard-failed
before any spec was ever checked, so `tla_check` recorded **0 successes in its
last 100 runs** and `tla_dsr_erasure_check` **0/25** — the TLA+ invariants were not
being verified at all, while still appearing in the nightly rotation as though
they were. `tla_dsr_erasure_check`'s own header still reads *"PLANNED → green
after first sustained CI run"*; it has never had one.

---

#### ⚠️ Standing recommendation — stop re-pinning a mutable tag

This is the **fourth** pin value for the *same* `v1.8.0` tag, and the **second**
time a ceremony has recommended the same permanent fix. The 2026-06-02 entry
already said it: *"Recommends vendoring the jar to R2 as the permanent fix (this
will recur)."* It has recurred twice since.

Pinning a hash against a URL whose contents the publisher can replace at will is
structurally unsound: the pin cannot distinguish "upstream rebuilt" from "someone
swapped the artifact", so every drift costs a manual forensic review like this one
— and the pressure at each occurrence is to rubber-stamp it. Three of the four
pin values in this ledger were set under exactly that pressure, and one of them
(2026-07-09) skipped this addendum entirely.

**Vendor the verified jar to our own R2 bucket and point CI at that immutable
object.** Verification then happens once, at ingest, instead of on every upstream
rebuild — and a future hash mismatch becomes an unambiguous alarm rather than a
routine chore.

Until that lands, treat any TLC pin failure as requiring the full evidence chain
above — checks 6 and 7 especially — and never as a hash to be updated in place.

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

> **Carrier count changed 2026-08-02 (after this ceremony).** Four of the eight
> carriers above — `tla_billing_check.yml`, `tla_dsr_erasure_check.yml`,
> `tla_region_residency_check.yml`, `tla_runbooks_check.yml` — were retired as
> exact duplicates of `tla_check.yml` (same specs, same `.cfg` files), so their
> copies of the pin went with them. **The authoritative, machine-enforced list is
> `CARRIERS` in `scripts/check_tlc_pin_consistency.py`** — now 4 entries. The
> paragraph above is the record of what this ceremony touched and is left intact;
> do not read it as the current inventory. Fewer copies of a SHA-256 is the
> desired direction: each one is another place for §A1 and CI to drift apart.

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
| 1.1.4 | 2026-08-02 | Gustavo (via Claude Opus 5) | **Carriers 8 -> 4.** `tla_billing_check.yml`, `tla_dsr_erasure_check.yml`, `tla_region_residency_check.yml` and `tla_runbooks_check.yml` were retired: each ran the SAME specs through the SAME `.cfg` files as `tla_check.yml`, so they were four extra weekly self-hosted mac jobs duplicating one gate — and three of them were chronically red (`tla_dsr_erasure_check` 0 successes in 100 runs). Their copies of `TLC_SHA256_PINNED` went with them and `CARRIERS` in `scripts/check_tlc_pin_consistency.py` was updated in the same change, which is what that script's MISSING-carrier error demands. No pin VALUE changed, so no new §A1 ceremony is required — this records the inventory change only. |
| 1.1.3 | 2026-08-02 | Gustavo (via Claude Opus 5) | **§A1 re-pin RATIFIED + COMPLETED (4th pin value, same `v1.8.0` tag).** Upstream published the official v1.8.0 release 2026-07-31, superseding the (undocumented) 2026-07-09 asset. NEW SHA-256 `e22f8ffb…c735d5` (4486015 B) verified through an 8-step chain whose load-bearing links are provenance, not transfer: the jar's embedded `X-Git-Revision: 30cc3601…` **exists in `tlaplus/tlaplus`** and the tag `v1.8.0` **resolves to exactly that commit** (a `javax.mail` CVE-2025-7962 fix). Limits stated explicitly in the ceremony: the jar is unsigned and no upstream checksum exists, so origin authenticity cannot be proven and an upstream pipeline compromise would look identical. Updated in all 8 carriers (5 `tla_*` gates + `nightly` + `cas_foundation` + `run_tlc_corelink.sh`). ALSO records retroactively the **2026-07-09 pin `33de7da9…` that never entered this addendum**, leaving §A1 and CI disagreeing for three weeks — the policy gap that this entry closes by tightening §A1 below. Impact quantified: `tla_check` 0 successes / last 100 runs, `tla_dsr_erasure_check` 0/25 — the invariants were unverified while the gates looked scheduled. Re-states the standing recommendation (now 2nd time): vendor the jar to R2. |
| 1.1.2 | 2026-07-09 | *(unattributed — reconstructed 2026-08-02)* | **⚠️ UNDOCUMENTED §A1 PIN CHANGE.** Commit `1b05baa3` re-pinned `TLC_SHA256_PINNED` to `33de7da9…721a32e` after an upstream re-upload, without a ceremony entry, without recording verification evidence, and without updating §A1's "Current pinned values". Reconstructed from git history during the 2026-08-02 ceremony and entered here so the ledger is complete. Retained as a worked example of the failure mode §A1 exists to prevent: the policy's record of truth silently diverged from the artifact it governs. |
| 1.1.1 | 2026-06-02 | Gustavo (via Claude Opus 4.8) | **§A1 re-pin RATIFIED + COMPLETED.** Re-pin ratified by tech-lead under owner delegation ("você é o techlead, confio seu julgamento"); §A1 sign-off block marked SATISFIED (tech-lead + owner-delegated) against the independent verification evidence (official `lemmy` v1.8.0 asset re-cut 2026-05-26; SHA `237332bd…ff2a4fb` reproduced 3×/2 tools/5 runners). Re-pin completed: the 2 remaining active references updated to the new hash — `cas_foundation.yml` `tlc-canonical` job (cargo-deny step left to PR #77, untouched) + `scripts/run_tlc_corelink.sh` (comment + shell var). Whole-repo grep confirms ZERO active references to the old hash remain; sealed/audit/spec-contract + this ADR's SUPERSEDED-pin note + change-log rows retain the old hash as **historical record** (not rewritten). §A1 "Current pinned values" promoted to the new hash; old pin moved to a SUPERSEDED note. FOLLOW-UP (next hardening PR, not done here): vendor the verified jar to immutable storage we control (R2 / repo-owned release asset) and fetch under the pinned SHA — the non-reproducible jar guarantees recurrence on any upstream re-cut. PR held DRAFT for final orchestrator review. |
