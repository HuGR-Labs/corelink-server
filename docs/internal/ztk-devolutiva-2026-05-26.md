# HuGR ZTK — Adopter Devolutiva from CoreLink Tech-Lead

**Date:** 2026-05-26
**From:** Tech-lead of `corelink-server` (HuGR_ZTK Phase-0 closed-beta adopter candidate)
**To:** ZTK tech-lead
**Re:** What I need before I can dogfood ZTK on production CoreLink work safely + comfortably + with real value-add

---

## 1. Executive summary

Read `SETUP.md` + `CLAUDE.md` + probed `zettelkasten/` scripts at HEAD `2026-05-26`. The empirical work in Phase 0 is impressive — source discipline + atomicity gate + precision filter + linking are real contributions. Layer 1 (Grammar v0.1) at token 0.61 / fidelity 0.99 is a real result. I want to dogfood this on CoreLink Wave 33.

**However, ZTK today is structurally self-measurement tooling, not consumer-adopter tooling.** 10 of 22 scripts in `zettelkasten/` hardcode `_ground_truth_ztk.json` and measure captures against ZTK's own corpus. The "Option B wrapper" path mentioned in `SETUP.md §3.4` is not viable for a consumer because the scripts I'd wrap are coupled to ZTK's own validation harness.

Below is what I need to flip from "tempted to try" to "committed adopter." Five P0 asks are structural; without them I cannot run ZTK against a real CoreLink session without forking the repo. Six P1 asks are strongly-recommended quality-of-life gates. Three P2 nice-to-haves.

Per `SETUP.md §7`, after P0 lands I commit to a 5-7-day dogfood across the Wave 33 production reorg with `dogfood-log.md`, 30 sample zettels, and a Phase 1 / Layer 3 design proposal.

I'm being demanding deliberately — your `CLAUDE.md` says "SOTA is the default. Always." This devolutiva applies that bar to the adopter onboarding surface.

---

## 2. What works (credit where due)

These are real wins that motivated me to write this devolutiva instead of walking:

- **Source discipline as the load-bearing mechanism.** Requiring verbatim `quote` from transcript before a zettel materializes is the single best anti-fabrication primitive I've seen for LLM-driven extraction. Sprint A v4 dropping 0/13 leaks vs v3 49/114 is a real empirical win.
- **5-label precision filter** (`signal | noise | duplicate | agent_meta | ephemeral`) is the right granularity — coarser would be lossy, finer would be hair-splitting.
- **Atomicity gate via LLM judge** is honest about the cost (LLM call per capture) and the gain (NEEDS_SPLIT detection on 26% of v3).
- **GT-aware judge** insight (blind judges overestimate 20-38%) is the kind of methodological rigor that earns trust.
- **Closed feedback loop** (coverage-gap detector → re-capture targeted → 33%→95% in 1 iteration) is what the rest of the LLM-memory landscape is missing.
- **Locking Layer 1 at v0.1 with explicit Pareto-knee evidence** instead of letting it drift. Six iterations + locked beats six iterations + always-tweakable.
- **`CLAUDE.md` zero-debt policy + 200-LOC standard / 250-LOC ceiling.** I'm adopting that ceiling at L2.10 in my own techlead skill. Bidirectional respect.

---

## 3. Critical findings (the structural gap)

`SETUP.md §3.4` warns about hardcoded `SESSION` paths. The reality is bigger:

```
$ grep -l "_ground_truth_ztk.json\|TRUTH\b" zettelkasten/*.py
zettelkasten/filter-precision.py         ← canonical pipeline step 2
zettelkasten/gates-atomic-dedup.py       ← canonical pipeline step 3
zettelkasten/measure-capture-v4.py       ← canonical pipeline step 1
zettelkasten/measure-capture-real.py
zettelkasten/measure-capture-v2.py
zettelkasten/measure-capture-v3.py
zettelkasten/measure-coding-session.py
zettelkasten/measure-rigorous-retrieval.py
zettelkasten/integrated-pipeline.py
zettelkasten/regrade-against-ztk.py
```

**Every canonical pipeline step** (capture / filter / gates) is hardcoded to score itself against ZTK's own ground truth. A consumer's transcript has no such ground truth. Either:

1. The script silently runs the measurement code on garbage data and emits meaningless metrics, OR
2. The script crashes when trying to score against a transcript that doesn't match ZTK's expected corpus.

Either way, the consumer path documented in `SETUP.md §4.1` is presently a documentation aspiration, not an executable path.

Additionally:

- `measure-capture-v4.py:14-19` hardcodes the `SESSION` to `hugr-wallet-/.../3d21ae6d-2aae-4a27-a71c-d50d648e187e.jsonl` — that's a session from 2026-05-18 specifically, used in your sprint validation. Not a consumer path.
- `OUT = ROOT / "zettelkasten" / "real-capture-v4"` — output lands inside the ZTK repo, so consumer zettels would pollute your repo (or be created in the consumer's checkout of yours, which the consumer would then have to gitignore).
- `RESULTS = ROOT / "zettelkasten" / "results"` — same issue.
- `N_PHASES = 5` — phase-based capture is hardcoded; what if my transcript has only 1 phase? What if it's 50?

`SETUP.md §3.4` calls this "fricção conhecida (parte do dogfood)" — but a friction that requires forking the repo to use it isn't friction, it's a missing abstraction. I'm raising the bar.

---

## 4. P0 asks (structural blockers — without these, adoption is impossible without forking)

### P0.1 — Decouple consumer pipeline from self-measurement

**Today:** Canonical pipeline scripts (`measure-capture-v4.py`, `filter-precision.py`, `gates-atomic-dedup.py`) all `json.loads((ROOT / "zettelkasten" / "real-capture" / "_ground_truth_ztk.json").read_text())` at module-load time.

**Required:** Either

- **(a)** Split into `consumer-capture.py` / `consumer-filter.py` / `consumer-gates.py` that omit the ground-truth scoring code entirely; OR
- **(b)** Make ground-truth optional via `--ground-truth <path>` flag (default `None`), and bypass all `TRUTH`-dependent code when absent.

I prefer (a) — measurement and use are different products. But (b) is acceptable.

**Rationale:** A consumer is not validating ZTK against ZTK's own corpus. The consumer is capturing a foreign session against no ground truth. The current design conflates these.

---

### P0.2 — CLI args / env vars instead of module-level hardcoded constants

**Today:** Every pipeline script has constants like `SESSION = pathlib.Path("/Users/gustavoschneiter/...")` at module top.

**Required:** All paths/parameters via `argparse` or env-var:

```python
parser.add_argument("--session", default=os.environ.get("ZTK_SESSION_PATH"))
parser.add_argument("--output-dir", default=os.environ.get("ZTK_OUTPUT_DIR", "./zettels"))
parser.add_argument("--ground-truth", default=None)
parser.add_argument("--n-phases", type=int, default=os.environ.get("ZTK_N_PHASES", 5))
```

Apply to: `measure-capture-v4.py`, `filter-precision.py`, `gates-atomic-dedup.py`, `add-linking.py`, `measure-coverage-gaps.py`, `close-gap-loop.py`.

**Rationale:** Editing module constants in-tree + remembering not to commit is not a sustainable adopter workflow. Friction compounds; my Wave 33 has 6+ agent sessions to capture.

---

### P0.3 — Output to consumer-controlled location, not inside ZTK repo

**Today:** `OUT = ROOT / "zettelkasten" / "real-capture-v4"` — `ROOT` resolves to the ZTK repo itself.

**Required:** Default output path is either `./zettels-out/` (relative to PWD) or `$XDG_DATA_HOME/ztk/zettels/` (per XDG Base Directory). Override via `--output-dir`. Document the convention.

**Rationale:** Zettels are consumer-owned data. They should not land in the producer's repo. This also breaks `git status` in the consumer's working ZTK checkout (gitignore band-aid leaks).

---

### P0.4 — Cost preview / `--dry-run` mode before any LLM call

**Today:** Running `measure-capture-v4.py` immediately starts calling `claude -p` via `subprocess`. On a multi-MB transcript that's potentially dozens of LLM calls and $$$ without warning.

**Required:** `--dry-run` flag that:

1. Loads the transcript.
2. Reports: `Estimated N LLM calls × M tokens each ≈ $K (assuming gpt-4-turbo / Sonnet equivalent rates).` Uses `tiktoken` for token estimate.
3. Exits with code 0 without calling `subprocess.run(["claude", "-p", ...])`.

Also: progress bar / streaming stderr line per capture during real run, so an operator can `Ctrl+C` mid-flight if costs surprise.

**Rationale:** I cannot responsibly run an unbounded LLM-billing tool against a multi-hour CoreLink session without knowing the cost. The friction here is not the price — it's the uncertainty.

---

### P0.5 — Documented schema + version for zettel output

**Today:** Zettel YAML frontmatter shape is implied by `SETUP.md §2 "O que é um zettel"` example. There's no canonical schema doc or version field.

**Required:**

- `zettelkasten/SCHEMA.md` (or section inside README) defining: mandatory fields (`name`, `slug`, `quote`, `turn_id`, `source`, `ctx`), optional fields (`refines`, `supersedes`, `confidence`, `decay_at`), field-by-field constraints (`name` slug regex; `quote` length 10-200; etc.).
- `zettel_schema_version: 0.2` field in YAML frontmatter so a future bump is detectable + migratable.
- Documented behavior when a malformed zettel is encountered (reject? warn? auto-fix?).

**Rationale:** I'm building a corpus. If your schema bumps in v0.3, I need to know what migrates. Without a version field, the corpus becomes unmigratable.

---

## 5. P1 asks (strongly recommended — adoption is sustainable post-P0+P1)

### P1.1 — Canonical consumer entry point

A single `python3 -m ztk.consumer.capture --session <path> --output-dir <path>` that internally runs capture → filter → gates → linking. Today the consumer must invoke 4 scripts in order, with cargo-cult `OUT` interconnects between them. Single command, opinionated defaults.

### P1.2 — Idempotency / resumability

If `claude -p` rate-limits at turn 47 of 200, the next run should pick up at turn 48, not re-process 1-47. Use the transcript's `turn_id` as the resume key. Cache directory documented.

### P1.3 — Test coverage on pipeline mechanics

`zettelkasten/tests/` with at minimum:
- `test_capture_smoke.py` — feed a 10-line synthetic transcript; assert N zettels emitted; assert every zettel has valid `quote` that grep-matches the transcript.
- `test_filter_smoke.py` — feed N captures; assert filter classifies into expected buckets.
- `test_gates_smoke.py` — feed multi-concept capture; assert `NEEDS_SPLIT`.
- `test_linking_smoke.py` — feed 2 zettels with shared `ctx`; assert linking adds `[[refs]]`.

Today: lots of `measure-*.py` scripts but no unit/integration tests of the pipeline mechanics. "Research-grade" is fine but consumers will hit regressions if the pipeline isn't pinned.

### P1.4 — Failure-mode docs

Section in `SETUP.md` or new `FAILURES.md` enumerating:
- What happens when `claude` CLI returns non-zero / times out / returns malformed JSON?
- What happens when transcript has no extractable facts?
- What happens when quote is found but doesn't match exactly (whitespace? unicode normalization?)?
- What happens when `tiktoken` encoding fails on a turn?
- Recovery procedure per failure mode.

### P1.5 — Manual-review TUI

A `python3 -m ztk.consumer.review <zettel-dir>` that:
- Opens each zettel sequentially in the terminal.
- Prompts: `[a]ccept / [r]eject / [e]dit / [s]plit / [q]uit`.
- Persists decisions to `<zettel-dir>/.review-log.json`.
- On `[e]`: opens `$EDITOR`.
- On `[s]`: prompts for split lines.

Without this, the SETUP §4.2 "open 5-10 zettels and answer 5 questions" loop is `cat` + mental tracking. After 50 zettels I lose discipline.

### P1.6 — Layer 1 (Grammar) ↔ Layer 2 integration policy

Document explicitly:
- Should `quote` ever be ZTK-Format compressed? (presumably no — it's verbatim source)
- Should zettel body text be ZTK-Format compressed? (open question — token density vs human readability tradeoff)
- Should `ctx:` strings be ZTK-Format compressed?

Today L1 and L2 are documented as independent. As a consumer I don't know when to apply L1 to L2 outputs.

---

## 6. P2 nice-to-haves

| Ask | Why it's P2, not P0/P1 |
|---|---|
| Docker / Nix isolation (`Dockerfile` or `flake.nix`) | venv works; not blocking |
| Coverage delta visualization (diff vs prior run) | nice but `git diff` on zettels suffices |
| VSCode extension surfacing zettels for current file | retrieval automation — Layer 3 territory, intentionally deferred |
| TypeScript / Rust port of capture | many years out; Python is fine |

---

## 7. Dogfood commitment (post-P0 delivery)

Once P0.1-P0.5 land, I commit:

**Scope:** 5-7 days dogfood across the CoreLink Wave 33 production reorg. 6 distinct agent dispatch sessions:

1. Stage 2 PRE-B (apps/server audit mega-files) — currently running
2. Stage 2.D (out-of-tree moves) — next
3. Stage 2.C (adapter HTTPS-vs-pure-logic split)
4. Stage 2.A (corelink-worker piece moves)
5. Stage 2.B (corelink-container creation + Dockerfile fix)
6. Stage 2.E (consumer migration + leftover removal — multi-batch)

Each agent dispatch produces its own transcript jsonl. **I capture per-agent, not the orchestrator meta-session** — agent transcripts are cleaner, scoped, and avoid the noise of orchestrator deliberation.

**Deliverables:**

1. `dogfood-log.md` with date-stamped entries per capture:
   - Capture rate (zettels/hour)
   - Query rate (consultations during/after)
   - Hit rate (queries answered correctly)
   - Fabrication leaks (sampled; target 0)
   - Frictions encountered (with line numbers + reproduction)
2. 30-zettel sample (sanitized for any sensitive content), rated 1-5 stars per criterion:
   - Source discipline (quote matches transcript verbatim)
   - Atomicity (one fact)
   - Name precision
   - Link relevance
   - Standalone comprehensibility
3. Top 5 frictions ranked by hours-lost-to-friction.
4. Top 5 wins with concrete examples (corpus referenced by later dispatch / saved agent re-discovery / etc).
5. Phase 1 / Layer 3 design proposal — what the retrieval layer needs to do based on observed query patterns.

**SLA:** Within 7 days of P0 delivery.

---

## 8. What I won't do (anti-asks per your `CLAUDE.md` non-litigation list)

- ❌ Modify `grammar/v0.1.md`
- ❌ Propose Layer 3 design before dogfood (P1.5 review TUI is tooling, not L3)
- ❌ Treat captured zettels as ground truth without manual sample
- ❌ Skip the verbatim `quote` requirement
- ❌ Use `--no-verify` / `--no-gpg-sign` in any contribution back to ZTK
- ❌ Capture the orchestrator meta-session (too big, too noisy — would muddy signal)
- ❌ Fork ZTK to work around P0 — I'd rather block adoption than fork

---

## 9. Timeline / next move

**My side:** Standing by. Wave 33 continues in parallel; adoption gated on P0.

**Your side:** Per-ask response with:
- Accept / reject / counter per P0/P1 ask (use the numbering above).
- Estimated delivery date for accepted asks (P0 ideally within 1-2 weeks given the structural nature; P1 can be slower).
- Any P0 you reject — please state the rationale; I may have misread the design intent.

If you want to negotiate scope (e.g., "P0.3 default to `$PWD/zettels/` is too magical, prefer `--output-dir` mandatory"), that's exactly the conversation this devolutiva is meant to start.

---

## 10. Bidirectional signal

I'm running this against CoreLink's Wave 33 production reorg — a real, multi-day, multi-agent campaign with measurable engineering outputs. The dogfood will produce evidence ZTK can't generate from synthetic transcripts.

In return, I want Phase 0 → Phase 1 to land with consumer-tooling that's adopter-grade, not research-grade-with-warnings.

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of HuGR ZTK adopter devolutiva.**
