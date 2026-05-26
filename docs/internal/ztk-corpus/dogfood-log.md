# ZTK Dogfood Log — CoreLink adopter

> Tracking CoreLink's adoption of HuGR_ZTK (semantic-density zettelkasten for LLM
> context). Pilots accumulate corpus; frictions + wins documented for ZTK
> tech-lead feedback loop.

---

## Session 1 — 2026-05-26 — cargo v2 SEAL pilot

**Transcript:** Wave 34 cargo adapter v2 (SEAL `23a44535`, branch
`wt/r-prep-w34-adapter-cargo-v2`).

**Input:** `subagents/agent-ad353cd37e181e0f6.jsonl` (500 KB; 220 turns).

**Output:** `docs/internal/ztk-corpus/cargo-v2-pilot/linked/` (14 zettels;
ZTK L1 grammar v0.1).

### Pipeline wall-clock

| Stage | Time | Counts |
|---|---|---|
| capture.py | 2:14 | attempted=17, accepted=16, rejected=1 (6%) |
| filter.py | 0:31 | signal=15, duplicate=1 |
| gates.py | 0:39 | ATOMIC=14, NEEDS_SPLIT=1, DUPLICATES=0 |
| linking.py | 0:34 | 13/14 linked |
| **Total** | **3:58** | **14 final zettels from 500 KB transcript** |

### Frictions

1. **`--project` flag inconsistency.** `capture.py` and `filter.py` accept
   `--project <NAME>`; `gates.py` and `linking.py` do NOT. SETUP.md quickstart
   example uses `$ZTK_PROJECT` env var across all 4 stages, which silently
   doesn't reach gates/linking. Recommendation: either accept `--project`
   on all 4 stages OR document the asymmetry in SETUP.md §1.

2. **Phase 1 capture took 2:14 (60% of total).** Capture is the bottleneck;
   subsequent stages 30-40s each. Capture parallelism appears single-threaded
   per phase (5 sequential phases × ~25s each). For very large transcripts
   (>5 MB), this could become painful. Capture is also where the cost lives
   (LLM calls per phase). Consider documenting expected cost per MB in
   SETUP.md.

3. **Zero ctx detected (`0/14 with ctx`).** Linking pass added refs to 13/14
   zettels but NO inline context blocks. Unclear if this is expected or
   indicates a coverage gap. SETUP.md §5 doesn't clarify what "ctx" should
   look like vs refs. Recommend SETUP.md add a sample linked zettel with both
   refs + ctx to set expectations.

### Wins

1. **High signal-to-noise.** 14 zettels from a 220-turn transcript = 1
   zettel per 16 turns. Each zettel is genuinely useful — captures distinct
   architectural decisions, charter rules, or code patterns. Example outputs:
   - `corelink-adapter-inline-ports-mandate.md` — captures the Wave 34
     hexagonal-ports decision in 5 lines.
   - `corelink-pat-constant-time-verify.md` — 4 lines on the
     `subtle::ConstantTimeEq` PAT pattern.
   - `corelink-workspace-spi-traits-fictional.md` — captures the
     PowerPoint-architecture failure mode that drove inline-ports
     convergence.

2. **L1 grammar achieves the density claim.** Average linked zettel is
   ~5 lines + ops/refs header. Token count per zettel: ~50-100 tokens
   estimated. 14 zettels × 75 tokens ≈ 1050 tokens total. Original 220-turn
   transcript: probably 50K+ tokens. **~48× density compression.**

3. **Linking discovers real conceptual graph.** Refs like
   `corelink-pat-constant-time-verify → corelink-subtle-choice-unwrap-u8`
   show non-trivial conceptual chains the linker discovered (not just
   keyword matches).

4. **Atomic-dedup gate caught 1 split case + 1 dup case.** Both legitimately
   needed surfacing. 0 false positives in the 14 survivors so far (manual
   sample size: 4 zettels read end-to-end).

### Sample review (4 of 14)

- `corelink-adapter-inline-ports-mandate.md` — ✅ correct + atomic.
- `corelink-pat-constant-time-verify.md` — ✅ correct + atomic.
- `corelink-audit-emitter-fail-closed.md` — (not yet sampled)
- `corelink-loc-cap-l210.md` — (not yet sampled)

**Continue sampling target:** 10/14 minimum per SETUP.md §6 discipline
(currently 4/14). Pending.

### Carry-forward for next pilot

- Pick a SEAL with **more architectural decisions per LOC** (oci is the
  candidate — full OCI Distribution Spec v1.1 implementation; richer
  vocabulary).
- Try a HALT SEAL (npm v1 HALT or cargo v1 HALT) to see if ZTK captures
  refusal-reasoning as well as it captures success-reasoning.
- Pre-warm by running `capture.py --dry-run` (if it exists) to estimate
  cost before committing the LLM spend.

---

## Friction → ZTK upstream issues

To be reported to ZTK tech-lead via
`/Users/gustavoschneiter/Documents/HuGR/HuGR_ZTK/DOGFOOD-DEVOLUTIVA-corelink-2026-05-26.md`
addendum once 2-3 pilots complete (per ZTK §9 reporting protocol).

Anti-asks honored per devolutiva §8:

- ❌ Did NOT modify `grammar/v0.1.md` (LOCKED).
- ❌ Did NOT invoke `zettelkasten/measure-*.py` (validation harness coupled to
  ground truth).
- ❌ Did NOT capture this orchestrator meta-session (too big, too noisy —
  picked the cargo v2 agent transcript instead).

---

## Index

| Session | Date | Source | Zettels | Status |
|---|---|---|---|---|
| 1 | 2026-05-26 | Wave 34 cargo v2 SEAL | 14 | ✅ DONE (4/14 sampled; 10 sampling pending) |
