# CoreLink backlog orchestration checkpoint — R2

Saved: 2026-09-01, America/Sao_Paulo

## Mandate and hard rules

- Root is the orchestrator and the only actor authorized to commit final integration,
  push, create/update PRs, review remote state, merge, and perform repo hygiene.
- Delegated work uses GPT-5.6 Luna/Terra, one exclusive worktree per lane, with
  disjoint file ownership. Every WP is a contract with scope, invariants,
  completeness criteria, DOD, quality standards, and mutation teeth.
- `BACKLOG.md` is the SSOT. Current measured population: 168 total, 107 open,
  60 done, 1 parked. Fetch `origin/main` before every restack/push/merge.
- Normal path remains `bash scripts/pre-merge-gate-check.sh --merge <PR>`. While
  GitHub Actions is unavailable, the owner explicitly authorized a documented
  exception: exact-SHA local CI, cold review, distinct external status, normal
  squash merge with `--match-head-commit`, and post-merge tree equality proof.
- `CLAUDE.md` requires DCO plus `Co-Authored-By: Claude …`; PR bodies end with
  the Generated-with footer. Add Codex coauthor only in addition to Claude.
- Root checkout is stale/diverged and user-owned; never edit it. Use worktrees.
- Do not touch sibling repositories/processes. No broad Docker prune. At most two
  heavy Rust lanes; `CARGO_BUILD_JOBS=4`.

## Canonical base and merges

- `origin/main = 76fa37de9bcf12b5e562edce129ad1328f9f8be2`.
- Merged this resumed session through the mandatory gate:
  - #1486 -> `dcd794cfc...`
  - #1499 -> `229fb8d69...`
  - #1513 -> `76fa37de9...`
- Earlier #1493 was already merged.

## GitHub Actions state and direct-CI override

- Actions is not fixed. Account payment `07W6UGAB`, US$47, Visa 0005 was
  declined on 2026-09-01; budget is the original US$47 hard stop.
- Builder Macs are online and fabric health is 200. The failure occurs before
  jobs/check-runs exist, so it is not a CoreLink runner execution failure.
- #1562 head `26833ba84d5b4916fd946814a012f772f8f1dc1e` triggered runs
  33576717372, 33576717701, 33576718150, 33576716399 and 33576715026;
  all ended `startup_failure`, zero checks/jobs.
- GitHub Actions remains unavailable, but it is no longer a merge blocker. The
  owner explicitly directed root to run CI locally and merge without Actions.
  Never forge `dco`/`gitleaks` contexts or use blind `--admin`; record an honest
  `external-ci/corelink-local` status and bind every merge to tested base/head/tree.

## Published PR queue

- #1495 runner-fleet truth: `80a7a622`, cold LAND, current-main, clean/mergeable,
  CI absent after replacing stale runner/image claims.
- #1508 B126 pip tests: `9aa88175`, exact current-main restack,
  clean/mergeable, CI absent.
- #1535 B133 CI trust boundary: `bed7eb84`, cold LAND, clean/mergeable, CI absent.
- #1534 B094 Buck2 claim scrub: `122e6a65`, cold LAND after R8 adversarial
  subject/negation review, current-main and clean/mergeable; CI absent.
- #1550 certified backlog WP ledger: `2c74b8fc`, cold LAND, exact 107/107,
  clean/mergeable, CI absent.
- #1553 B161 Homebrew auth: `a1993730`, cold LAND with real Vitest 5/5,
  clean/mergeable, CI absent.
- #1561 B160 PAT issue limiter: `ccc986c2`, exact patch restacked/squashed on
  current main, cold LAND, clean/mergeable, CI absent.
- #1562 B159 sccache DELETE contract: `26833ba84`, tree cold-LAND, current-main,
  DCO + Claude/Codex trailers + PR footer, clean/mergeable, startup failures.
- #1563 B163 fail-closed curl recipe: `ca2b8da4`, exact merge-tree projection of
  reviewed R3, clean/mergeable; three official runs startup-failed with zero jobs.
- #1564 B140 structural runner YAML parser: `12391ccb`, cold LAND, current-main,
  clean/mergeable; official CI is expected to startup-fail under the same blocker.
- #1534/#1538/#1544/#1553/#1558/#1485 must not be updated until their current
  repair receives a fresh cold LAND.

## Active integration/review loops

- Ledger R10 `2c74b8fc`: cold LAND and remote #1550 updated; it is now the
  certified 107/107 planning source, pending official CI.
- B094 R8 `122e6a65`: final cold LAND; #1534 remote updated.
- B088 R3 `c505cff8`: corrected residual Semgrep claims and expanded verifier;
  fresh independent cold review active. It is two commits and must be squashed
  onto current main after LAND.
- B169/B170 gitleaks R3 `180f1ef1`: still misses arbitrary quoted keys and
  multiline/folded/continued assignments; PyYAML is unprovisioned in the real
  lane. R4 active.
- B112 R5 `22ef914c`: immutable automatic signer relay, exact SLSA subjects and
  safe env transfer; final cold review active.
- B166 R8 `5d1b22bf`: complete 32-document trust population and localized signing
  checker; final cold review active.
- B096 R3 review found manifest PUT is tenant-scoped and only blob bytes can be
  index-shaped, plus a stale six-pin comment. R4 active; Cargo remains explicitly
  inconclusive due host contention.
- B140 R10 `12391ccb`: final cold LAND and published as #1564.
- #1485/B074 R4 `8c2cf1f3`: visible ASCII 0x21..0x7e, 173 Worker tests; cold review
  active, followed by a one-commit current-main squash if LAND.
- #1488 R4 is awaiting cold review after a reproducible 214-job census; B149 is
  recorded as a DRIFTED blocker rather than hidden.
- #1495 has cold LAND and remote update at `80a7a622`.
- Stale non-landable #1480 and stacked #1483 were closed without deleting their
  branches; both can be reopened only after current-main, ID-backed evidence.
- #1554 B101 review found DD-037 wrongly declined despite B093, and 89-vs-110
  contract drift; repair active.
- #1557 B091 review found checksum not verified against Cargo.lock and missing
  Cargo.lock/Cargo.toml/SBOM trigger paths; repair active.
- #1555/#1559/#1560 cold reviews are active. #1556 still needs a slot.

## Backlog dependency roots

- B056/B077 -> B057 -> B078 -> B093.
- B067 -> B073/B074/B081 -> B082.
- B091/B112 release/SBOM spine.
- #1539 -> #1542 API spine.
- B160 before B157/B158/B162/B164/B165.
- WP140 -> WP142/WP150; WP146 -> WP155; #1497 is the OKF spine.
- Owner blockers: B008, B012, B013, B032, B035, B086, B089, B097, B110,
  B111, B154.

## Next root actions

1. Collect every active repair/review verdict; recycle each completed thread.
2. For LAND trees, restack/squash exactly once onto fetched current main and add
   DCO + Claude trailer; root force-with-lease updates or opens the PR.
3. Update #1550 only after ledger R10 receives cold LAND, then use its certified
   dependency graph for the next disjoint implementation wave.
4. Drain the ready queue with exact-SHA local CI, cold review, a distinct external
   status, normal CAS-pinned squash merge, and tree equality verification.
5. When billing is fixed, run canary -> builder -> corelink as a restoration test;
   Actions is a convenience lane, not the sole merge authorization path.

The legacy report-only gate still refuses absent Actions checks. Do not weaken or
impersonate it; use the owner-authorized, documented local-CI exception above.

## 2026-09-01 R3 addendum — local CI and OKF Codex bot

- New canonical main: `bf3f411d1e6aa71d51719e8f0a5f9c34702b8669`.
- PR #1565 migrated OKF autoreconcile from Claude to Codex and was squash-merged
  after local CI/cold LAND as `bf3f411d1e6aa71d51719e8f0a5f9c34702b8669`.
- Tested PR head: `de501464e7aaaf5bd01f898be0e4a0bbf31c06d1`;
  tested/merged tree equality: `1dc8d11c0fd54ee4be2cc4f7def99fda22cd373b`.
- The workflow remains dispatch-only and now pins
  `openai/codex-action@86365089...`, Codex CLI `0.152.1`, model
  `gpt-5.6-luna`, effort `high`, and `permission-profile: :workspace`.
- `OPENAI_API_KEY` and `OKF_BOT_PAT` are not provisioned. Until Actions returns,
  use `scripts/okf-reconcile-local.sh` with the existing ChatGPT login.
- Manual OKF activation on base `76fa37de...` returned `stale_count=0`; no model
  call and no empty reconcile PR were created.
- Final OKF evidence: 165 concepts, 1 deferred, 0 stale/drift; 84/84 mutation
  fixtures; actionlint, 463/463 pins, shell safety, secrets `code_only=0`,
  changelog, DCO and gitleaks green.
- `scripts/ci.sh --validators-only` was 14/15. The only failure,
  `validate-sub-processors`, reproduces unchanged on detached `origin/main`:
  Resend, Sentry, Plausible and Better Stack still have pending legal-review
  evidence. This is a real baseline debt, not a regression or a waived green.
- #1495 head `80a7a622...` already passed its targeted local gates on old base
  `76fa37de...`; because main moved, rebase/restack and rerun against `bf3f411d...`
  before merging. Re-evaluate every other ready PR the same way, serially.

## Persistent plan

The concise plan is `/Users/gustavoschneiter/.codex/plans/corelink-backlog-orchestrator.md`
and was updated during this turn with the 107-open census and current queue.

## 2026-09-02 R4 addendum — delivery anchor after throughput failure

The owner identified a real orchestration failure: after roughly twelve hours and
very high token expenditure, delivered backlog movement remained below 30%. The
cause was not a lack of safe parallel work. Root repeatedly recertified, recounted
and investigated infrastructure while allowing the serial merge queue to throttle
disjoint authorship.

The durable anchor is now in `CLAUDE.md` under **Delivery anchor — throughput is a
correctness property**. Its non-negotiable accounting and scheduling rules are:

- only merge plus backlog state transition is delivered progress;
- root drains a cold-LAND candidate through all existing required gates before
  unrelated planning/inventory;
- merge order is serial, but disjoint authorship remains fanned out;
- PRs are stacked on the last certified candidate tree; after a parent squash,
  byte-identical child trees reuse heavy tree-bound CI and rerun only head-bound
  gates, while any tree change forces full CI again;
- agents run focal/mutation and commit-bound checks; root runs universal and heavy
  path-equivalent suites once on the cumulative stacked bundle tree;
- at least half of available agent slots produce new implementation when the
  backlog has that much disjoint executable work;
- reviewed contracts in a pending planning PR may be consumed read-only against
  current `main`; an unmerged ledger alone cannot idle the fleet;
- two consecutive zero-delivery checkpoints while a LAND candidate existed are a
  hard orchestration failure.

Measured checkpoint after applying the anchor at
`main@201af23f810e5b7dce8a74ba5000698a194ef4c9`:

- exact `BACKLOG.md` population after #1564: 168 items, 61 done, 106 open and
  1 parked; stage breakdowns are deliberately not frozen here because they must
  be re-derived after each merge rather than copied forward;
- #1565 and #1495 merged under exact-SHA local CI; #1495 merge is `7cfecaa4811...`
  and its merged tree equals the tested tree;
- #1564 was reanchored at `54ed6d3f22bf8d215d85f65ddd44cb53e3c53468`,
  received exact-SHA cold LAND and merged as `201af23f810e5b7dce8a74ba5000698a194ef4c9`;
  merged tree `517080192dcc20ff66ca9be307acf9edcea665c4` exactly equals the tested tree;
- 18 subagent lanes are active: eight new authors, seven cold reviews, B-074
  repair, B-112 integration prep and #1535/B-133 completion.

Next root action is integration, not another census: publish this anchor, then
consume the next LAND return while author lanes remain active.
