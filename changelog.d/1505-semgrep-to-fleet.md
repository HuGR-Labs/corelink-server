### Changed

- **`semgrep.yml` runs on the ephemeral Cloudflare fabric (`runs-on: corelink`)**
  instead of the owner's Mac. The label is the only line of code this changes,
  and it is measured: the `corelink` label is carried by 32 `cf-runner-*`
  runners, and its intersection with the `mac` label is empty.

- **The comment block above that line was rewritten, because in a one-line PR the
  comment IS the deliverable and the previous one was wrong.** It claimed the
  lane's 388-run / 0-success record was entirely `ubuntu-latest` asking for a
  hosted box "that billing will not grant". Refuted by execution, and the
  distinction matters — the false version predicts a lane that goes green on
  repoint, the true one predicts a lane that fails on its first dispatch:

  - Run `31365335113` (2026-08-10) **did get** a hosted runner
    (`runner_name: "GitHub Actions 1000003886"`, labels `[ubuntu-latest]`), ran
    6m35s, and failed *inside the scan*:
    `Ran 474 rules on 6345 files: 4228 findings (4228 blocking)` — with
    `--error`, exit 1. Same signature in `29481995796` and `30801766782`. A
    second, independent failure follows: the SARIF upload needs Advanced
    Security enabled for code scanning, and it is not.
  - `ubuntu-latest` only entered this file on 2026-06-19 (`3b21a297`, #389), so
    **346 of the 388 runs predate the label** — that majority failed on the dead
    `[self-hosted, Linux, X64]` label and the placeholder
    `returntocorp/semgrep@sha256:000…0` image. Two historical causes, not one.
  - The temporal qualifier "every dispatch since 2026-08-24" does not hold
    either: there have been **zero runs since 2026-08-10**, and the `on:` block
    is `workflow_dispatch` only (the `schedule:` has been commented out since
    2026-08-10). The claim that this trades a dead lane for live Mac load is
    likewise overstated — with no cron, *neither* destination generates
    automatic load.

  Kept from the original: the `pip3` correction (the install step builds its own
  `python3 -m venv .semgrep-venv` and never calls a host `pip3`) and the
  destination rationale, both of which measure out.

- **The step comment 20 lines below no longer contradicts that correction.** It
  still said Semgrep was "installed via the host pip3 into a throwaway venv" —
  the exact claim the block above refutes, in the same file, in a PR whose
  deliverable IS the comment. Rewritten off the `run:` block: the host `python3`
  is used for one thing (`python3 -m venv .semgrep-venv`) and every install goes
  through that venv's own pip.

- **`B-110.verify` no longer breaks on this change.** Its ratchet pinned the
  literal `runs-on: [self-hosted, mac, corelink-builder]` in `semgrep.yml`, so
  this PR turned the item **DRIFTED** — and not visibly here, because
  `backlog-verify`'s `pull_request.paths` does not include
  `.github/workflows/**`: the red would have landed on the next PR to touch
  `BACKLOG.md`. The ratchet now measures the item's actual claim ("this lane is
  not on the GitHub-hosted runner that billing blocks") instead of one specific
  self-hosted destination, with the two anti-vacuity guards a negative predicate
  requires: no code-level `runs-on:` line at all, and more than one, both fail.

  Repointing `runs-on:` is necessary and **not sufficient** — the first green
  dispatch still needs the 4228 blocking findings triaged and an `--error` policy
  decision. Tracked as **B-139**, so that WP-CI does not close declaring this
  lane fixed.
