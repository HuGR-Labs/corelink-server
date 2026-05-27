# Wave-27 TLA-coverage figure refresh audit — 2026-05-16

> **Doc kind:** wave-27 R-prep cosmetic-doc fix audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-27 TLA-figure-refresh agent (Claude Opus 4.7) — branch `wt/r-prep-tla-figure-refresh`.
> **Base:** `main` @ `a48bbec` ("merge wt/r-prep-cf-worker-prefetch-wire into main (wave-26)").
> **Scope:** close the **wave-25 adversarial-review P1-01 finding** (stale TLA-coverage figures in three SEAL-gate audit docs post wave-25 `auth_pat_revoke.tla` cherry-pick `8fa1c22` + post wave-26 INV-CRITICAL TLA+ coverage milestone). Refresh figures to the wave-26-canonical state. Substantive verdicts unchanged.
> **Freeze classification:** cosmetic-doc fix per `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md` §3.d allowlist.

---

## §1. Background

### §1.1 P1-01 source

`specs/_audits/sealed/2026-05-16-wave25-adversarial-review.md` §3 P1-01 ("Stale TLA-coverage figures in three SEAL-gate audit docs post-cherry-pick"):

> Three audit docs authored EARLIER in the wave-25 merge order (security-attestation, ga-readiness-final, wave25-closure) still cite "81 TLA-verified / 1 CRITICAL TLA-exempt" figures that the registry no longer matches post-cherry-pick. This is the symmetric counterpart to wave-24 P0/P1 — registry now leads doc text by one row, where wave-24 had doc text leading registry by one exemption.

### §1.2 Canonical state at HEAD

Per wave-26 final pre-GA INV-CRITICAL TLA+ coverage audit `specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md` (commit `d1581b5`):

| Metric | Value |
|---|---|
| INV-CRITICAL declared in registry §3 | **61** |
| TLA-verified CRITICAL (spec exists, TLC GREEN) | **61** (61/61) |
| CRITICAL exempt from TLA | **0** |
| CRITICAL without TLA+ proof | **0** (Z=0 unverified) |
| TLA-verified declared INVs (broader scope, includes non-CRITICAL pinned via TLA) | **82** |
| TLA-verified Δ vs wave-25 sweep authoring | +1 from `auth_pat_revoke.tla` cherry-pick `8fa1c22` |

**Audit verdict (wave-26):** PASS — all 61 CRITICAL invariants carry direct or inherited TLA+ proofs. Coverage gate `CRITICAL-without-TLA: 0` met.

### §1.3 Root cause of staleness

The three SEAL-gate docs were authored either on wave-24 SEAL tip (`e9ee8eb` / `33138b5`) BEFORE the wave-25 cherry-pick `8fa1c22` (`auth_pat_revoke.tla`) reordered the registry state, OR earlier in the wave-25 merge order BEFORE the cherry-pick landed. Substantive engineering work (the TLA proof) is on `main` at HEAD; only the consuming-audit text was stale. The wave-26 INV-CRITICAL final-audit milestone confirmed 61/61 CRITICAL TLA-verified, giving the refresh a single canonical target instead of a per-doc point-in-time interpolation.

---

## §2. Per-doc refresh log

### §2.1 `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md`

Authored on `e9ee8eb` (wave-24 SEAL tip) at commit `619d449` (wave-25 stream #8). Was distributed externally as day-1 pentest vendor + GA cutover board input. SEAL-gate distribution makes this the highest-priority refresh target.

| Location | Pre-refresh text (substantive excerpt) | Post-refresh text (substantive excerpt) |
|---|---|---|
| §1 row "INV-CRITICAL TLA+ coverage" (line 29) | `61 CRITICAL declared / 81 TLA+ verified / 1 CRITICAL TLA-exempt (INV-PAT-REVOKE-PROPAGATION — wall-clock obligation, not distributed-consensus property per §4.3)` | `61 CRITICAL declared / 61 of 61 CRITICAL TLA-verified (Z=0 unverified, milestone reached wave-26) / 82 TLA-verified broader scope / 0 TLA-exempt` |
| §3 INV table rows (lines 78–80) | `CRITICAL: 61 (+1 INV-PAT-REVOKE-PROPAGATION — TLA+ exempt per §4.3) / TLA+ verified: 81 / CRITICAL without TLA+: 1 (TLA+ exempt §4.3)` | `CRITICAL: 61 (+1 INV-PAT-REVOKE-PROPAGATION — now TLA-verified via auth_pat_revoke.tla cherry-pick 8fa1c22) / TLA+ verified: 82 / CRITICAL without TLA+: 0` |
| §3 narrative paragraphs (lines 84–86) | "all 60 distributed-systems-property CRITICAL invariants either have TLA+ proof OR carry a documented §4.3 exemption ... 81 TLA-verified (≥ 76+), 0 CRITICAL lacking either proof or documented exemption" | "all 61 CRITICAL invariants have direct or inherited TLA+ proof ... 82 TLA-verified (≥ 76+), 0 CRITICAL lacking proof. §4.3 exemption framework remains in registry but no CRITICAL invariant currently consumes it." |
| §11.1 V&V table row "Invariant registry" (line 306) | `197 INVs + 81 TLA-verified` | `197 INVs + 82 TLA-verified (61/61 CRITICAL TLA-verified post wave-26)` |
| §14 changelog | (v1.0.0 marker added `[SUPERSEDED — see v1.0.1]`) | New v1.0.1 entry documenting the refresh with full per-line citations. |

**Stale figures replaced:** 4 distinct locations (5 stale text spans). Substantive verdict unchanged: CRITICAL-no-TLA was 0 under wave-25 exempt classification, remains 0 under wave-26 direct-proof classification. Cross-ref added to wave-26 final audit `specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md` (commit `d1581b5`).

### §2.2 `specs/_audits/sealed/2026-05-16-ga-readiness-final.md`

Authored on `33138b5` (wave-23 SEAL tip) by wave-24 GA-readiness final-audit agent. This is the canonical CONDITIONAL GO board document feeding the GA-cutover meeting per §13 2-key authorization block.

| Location | Pre-refresh text (substantive excerpt) | Post-refresh text (substantive excerpt) |
|---|---|---|
| §1.2 row "Spec corpus" (line 29) | `1 CRITICAL without TLA+ proof (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt per §4.3 pattern — sub-second revocation propagation is a wall-clock obligation, not a distributed-consensus property)` | `0 CRITICAL without TLA+ proof (was 1; INV-PAT-REVOKE-PROPAGATION TLA-verified via auth_pat_revoke.tla cherry-pick 8fa1c22 wave-25; 61/61 CRITICAL TLA-verified, Z=0 unverified per wave-26 final audit; broader-scope TLA-verified count now 82)` |
| §4 INV table row "└ CRITICAL" (line 120) | `+1 (INV-PAT-REVOKE-PROPAGATION §3.28 — TLA+ exempt per §4.3 pattern: sub-second wall-clock obligation)` | `+1 (INV-PAT-REVOKE-PROPAGATION §3.28 — now TLA-verified via auth_pat_revoke.tla cherry-pick 8fa1c22; §4.3 exemption no longer consumed)` |
| §4 INV table row "TLA+ verified" (line 125) | `81 / 0` | `82 / +1 (was 81 pre-wave-25; +1 from auth_pat_revoke.tla cherry-pick)` |
| §4 INV table row "CRITICAL without TLA+ proof" (line 129) | `1 (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt) / +1` | `0 (61/61 CRITICAL TLA-verified, Z=0 unverified per wave-26 final audit) / -1` |
| §4 narrative paragraph (line 135) | "all CRITICAL invariants either have TLA+ proof OR are documented exempt under §4.3" | "all 61 CRITICAL invariants have direct or inherited TLA+ proof post wave-25 cherry-pick + wave-26 final audit confirming 61/61 CRITICAL TLA-verified, Z=0 unverified" |
| §14 Quality gates row "Canonical consistency validator" (line 363) | `1 CRITICAL without TLA+ proof (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt)` | `0 CRITICAL without TLA+ proof (was 1; tla-verified: 82)` |
| §15 Snapshot record | (added "Refresh history" sub-bullet) | New 2026-05-16 wave-27 refresh entry. |

**Stale figures replaced:** 6 distinct locations. Substantive verdict (CONDITIONAL GO) unchanged.

### §2.3 `specs/_audits/sealed/2026-05-16-wave25-closure.md`

Authored at commit `7cf4bbb` (wave-25 stream #10 — hygiene + cataloguing pass). Survey-only doc — no SEAL from this stream — but referenced widely as the canonical wave-25 closure document.

| Location | Pre-refresh text (substantive excerpt) | Post-refresh text (substantive excerpt) |
|---|---|---|
| §5 INV table row "TLA+ verified" (line 130) | `81 / 0` | `82 / +1 (was 81 at wave-25 sweep authoring; +1 from auth_pat_revoke.tla cherry-pick 8fa1c22 landed later in wave-25 merge order)` |
| §5 INV table row "CRITICAL without TLA+ proof" (line 134) | `1 (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt per wave-24 GA-readiness §4.3)` | `0 (was 1 at wave-25 sweep authoring; 61/61 CRITICAL TLA-verified, Z=0 unverified per wave-26 final audit)` |
| §7 Quality gates row "Canonical consistency validator" (line 231) | `1 CRITICAL without TLA+ (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt per wave-24 §4.3 wall-clock obligation pattern)` | `0 CRITICAL without TLA+ at HEAD (was 1 at wave-25 sweep authoring; tla-verified: 82)` |
| §8.1 caveat bullet "CRITICAL-no-TLA+ count is 1" (line 258) | "CRITICAL-no-TLA+ count is 1 (INV-PAT-REVOKE-PROPAGATION) — TLA+ exempt per wave-24 GA-readiness §4.3" | "CRITICAL-no-TLA+ count is 0 at HEAD (was 1 — INV-PAT-REVOKE-PROPAGATION — at wave-25 sweep authoring; flipped to TLA-verified when auth_pat_revoke.tla cherry-pick 8fa1c22 landed later in wave-25 merge order)" |
| §9 Snapshot record | (added "Refresh history" sub-bullet) | New 2026-05-16 wave-27 refresh entry. |

**Stale figures replaced:** 4 distinct locations. Substantive verdict (wave-25 CLOSED via combined-bucket) unchanged.

---

## §3. Aggregate refresh tally

| Doc | Stale locations refreshed | Substantive verdict change |
|---|---|---|
| `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` | 5 (§1 row + §3 INV table + §3 narrative + §11.1 V&V row + §14 changelog v1.0.1 entry) | None (CRITICAL-no-TLA was 0 under exempt classification, remains 0 under direct-proof) |
| `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` | 7 (§1.2 row + 4× §4 INV table rows + §4 narrative + §14 Quality gates row + §15 refresh history) | None (CONDITIONAL GO retained) |
| `specs/_audits/sealed/2026-05-16-wave25-closure.md` | 5 (§5 INV table 2 rows + §7 Quality gates row + §8.1 caveat + §9 refresh history) | None (wave-25 CLOSED via combined-bucket retained) |
| **Total** | **17 distinct locations across 3 docs** | **0 verdict changes** |

---

## §4. Quality gates verified (this audit)

| Gate | Command | Expected result |
|---|---|---|
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — `_audits/` excluded from `SKIP_ALL`; this audit doc not validated structurally but adjacent specs unchanged |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references introduced (new cross-refs are to existing audit docs on `main`) |
| Stale-figure grep gate | `grep -rn "81 TLA-verified\|81 / 1" specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md specs/_audits/sealed/2026-05-16-ga-readiness-final.md specs/_audits/sealed/2026-05-16-wave25-closure.md` | 0 hits in non-historical / non-quoted context (refresh-history sub-bullets are explicitly quoted "was 81" snapshots; counted as historical, not stale) |
| Freeze allowlist gate | `python3 scripts/check-ga-freeze-allowed.py --staged` | PASS — cosmetic-doc fix per freeze §3.d allowlist (`_audits/` doc-only paths; no INV/SLO/ADR semantic shifts) |

---

## §5. Canonical source-of-truth (post-refresh)

The single canonical TLA-coverage source-of-truth at wave-27 is:

- `specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md` (commit `d1581b5`) — **61/61 CRITICAL TLA-verified, Z=0 unverified, 82 TLA-verified broader scope, 0 TLA-exempt**.

Future audits citing TLA-coverage figures should cross-reference this doc directly rather than copy the figures into prose. The three refreshed docs now all cross-ref to this canonical source.

---

## §6. Snapshot record

- **Branch:** `wt/r-prep-tla-figure-refresh`
- **Base commit:** `a48bbec` (wave-26 SEAL tip)
- **Refresh date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-27 R-prep TLA-figure-refresh agent)
- **Sign-off:** Gustavo Schneiter (final approver)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## §7. Cross-references

- `specs/_audits/sealed/2026-05-16-wave25-adversarial-review.md` (wave-25 adversarial review; §3 P1-01 source).
- `specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md` (wave-26 final pre-GA INV-CRITICAL TLA+ coverage audit — canonical source-of-truth).
- `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` (refreshed — see §2.1).
- `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` (refreshed — see §2.2).
- `specs/_audits/sealed/2026-05-16-wave25-closure.md` (refreshed — see §2.3).
- `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md` (freeze policy §3.d cosmetic-doc allowlist).
- `specs/03_architecture/invariant_registry.md` §3.28 (INV-PAT-REVOKE-PROPAGATION — was the TLA-exempt entry pre-cherry-pick; now TLA-verified).
- `specs/tla/auth_pat_revoke.tla` (the wave-25 cherry-pick `8fa1c22` that flipped the registry state).

---

**End audit.**

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
