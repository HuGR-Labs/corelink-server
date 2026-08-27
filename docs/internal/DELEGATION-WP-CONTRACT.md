# Delegation WP contract

How work is handed to a non-Claude implementation model (today: OpenRouter
`minimax/minimax-m3:free`, `nvidia/nemotron-3-ultra-550b-a55b:free`) driven by
`scratchpad/oxa` — and what the tech lead owes the WP **before** it is handed over.

Companion to [TECHLEAD-CHECKLIST.md](TECHLEAD-CHECKLIST.md) and
[CODE-REVIEW-CHECKLIST.md](CODE-REVIEW-CHECKLIST.md). Those govern our own PRs;
this one governs delegated ones, where the failure modes are different.

## The axiom

> **Execution is delegated. Judgment is not.**
> The delegate's output must be *what the WP ordered* — no more, no less.
> Anything in the diff that the WP did not order is a **spec defect**, logged as
> such, even when the extra code is harmless.

The corollary is the whole point: **if the delegate had to decide something, the
WP was unfinished when it was sent.**

## Before you write the WP: read the seams it touches

Not the file — the **contract**. Specifically:

1. The **test harness** the WP's tests will run against. Shared doubles carry
   doctrine. `worker/tests/d1_batch_mock.ts` deliberately resolves `batch`
   *through* each statement's own `first()` so the serial and batched paths can
   never answer differently — a WP that says "assert the read count" without
   naming that fact produces an assertion that **cannot pass**. (2026-08-26, WP-E.)
2. The **frozen contract** being mirrored, quoted by symbol
   (e.g. `worker/src/lib/internal_auth.ts::resolveConsumerKey`, all four arms).
3. The **gates** the PR will meet (below), so the WP can pre-empt them.
4. Whether the surface has **real callers**. Measure it (`git grep` across every
   consuming repo) — blast radius drives the rollout shape, and an assumed one is
   how a "phase 2 could stop the money path" claim gets made about an endpoint
   with zero code callers. (2026-08-26, WP-B.)

## WP structure — all seven sections are mandatory

A WP missing any section is not ready to send.

### 1. Files you may edit
An explicit allowlist, enforced by the driver (`OXA_FILES`). The driver cannot
create files — **pre-create** any new test file yourself with a placeholder line.

### 2. Read these first
Exact paths, and for each, *what to take from it*. "Read the file" is not a
briefing.

### 3. The change — decided, not described
Say what the code must become. If there is an obvious-but-wrong alternative,
**name it and forbid it with the reason**: without that, the model will do the
obvious thing.
> "Do NOT change the port to `&[u8]` — both impls consume the owned `Vec`
> downstream, so a borrow relocates the same copy and saves nothing."
> (WP-F: obeyed.)

### 4. Invariants — what must not move
Order-of-evaluation, fail-closed arms, audit-before-write ordering, status codes,
precedence of rejection. State that these are **behaviour**, not implementation
detail, or they will be "cleaned up".

### 5. Quality standards
- No new dependencies. No reformatting untouched lines. No CHANGELOG edits
  (the lead owns the entry).
- Rust: `cargo clippy --all-targets -- -D warnings` and `cargo fmt` clean.
- TypeScript: strict, `exactOptionalPropertyTypes` — no `foo: undefined` shapes.
- **No new exported symbols** unless the WP names them. An export is API surface;
  adding one with no consumer is out-of-spec. (WP-B: caught and reverted.)
- Tests assert **counts and outcomes, never wall-clock**. A timing assertion on
  the self-hosted fleet is a flake generator.

### 6. The closed-world clause — verbatim
> Do not add anything not listed in this WP. If you believe something else is
> required, STOP and report it in your card instead of doing it.

### 7. Return card
A fixed shape, and: **"Do not claim you ran anything — you cannot. I run it."**
The driver has no shell; a delegate that reports test results is fabricating.

## Definition of Done — the lead's, not the delegate's

A WP is done when **the lead has personally observed** each of:

1. **Diff read line by line** against the WP. Every hunk traces to an ordered
   item; anything else is reverted or explicitly adopted as a logged amendment.
2. **Citations verified.** Every symbol/path the delegate cites in a comment
   exists. Fabricated citations in a security comment are debt.
3. **Suite green**, run by the lead.
4. **Mutation-verified**: revert the fix, keep the test — the new cases go **red**,
   and red *for the stated reason*. `expected 200 to be 503` is proof; a test that
   passes both ways is decoration.
5. **Gates**: `[Unreleased]` CHANGELOG entry (`fix:`/`feat:`), `Signed-off-by:`
   (DCO), secrets-matrix row + `secrets-checklist-verify.sh` OK +
   `validate_secrets_matrix.py` `code_only=0` when a secret is introduced, OKF
   re-anchor **after** the final rebase.
6. **Merged only via** `bash scripts/pre-merge-gate-check.sh --merge <PR>`.
   Never `&& gh pr merge` — the pipeline's exit status is `tail`'s.

## Completeness criteria — when the WP set is finished

Not "the PRs are open". A campaign is complete when:

- every confirmed finding is either **fixed** or **declined with a price**
  (record the cost and the reason — e.g. perf F6 declined because
  `worker/src/index.ts` is cited by 10 OKF concepts and the re-anchor cost dwarfs
  a sub-millisecond win);
- every refuted finding is recorded as refuted **with the evidence**, so it is not
  re-litigated;
- the deployables are actually **deployed** and proven from the destination
  (bundle read back per region, running image tag from the Containers API,
  `/_health/container` per region) — a green workflow is not the proof;
- no orphan worktrees, branches, or scratch files remain.

## Verification asymmetry (why the lead runs everything)

The delegate cannot execute. Every "it works" from it is inference. Worse, the
signals *we* read can also lie:

- `cargo clippy` printing `Finished in 1.25s` with no `Checking <crate>` line
  compiled **nothing** — a cache hit, not an approval. Force a re-check.
- A shell pipeline's exit code is the **last** command's. A wait-loop ending in
  `grep -c` reports failure precisely when the count is zero, i.e. when
  everything is clean.
- A file containing NUL bytes is **binary to git**: `git diff` shows
  `Bin 57665 -> 68826 bytes` and the review sees nothing.

**Silence is not success. A green with no evidence of work performed is
unverified** — whether it came from an agent or from a tool.

## Model notes

Both models above support tool use and 1M context at $0. Free tier means no
fallback and 429s retried on the same model; set `OXA_MAX_TURNS` generously.
Run each WP in its **own git worktree** cut from `origin/main` — never the main
clone (worktrees share the clone, and `.env.local` is gitignored so a fresh
worktree cannot reach it).
