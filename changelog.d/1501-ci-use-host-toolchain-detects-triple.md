### Fixed

- **`scripts/ci-use-host-toolchain.sh` assumed macOS and then blamed the runner for
  the triple it had invented itself.**
  The script resolved the host triple as
  `HOST_TRIPLE="${HOST_TRIPLE:-x86_64-apple-darwin}"` — a default from when the
  only self-hosted fleet was the owner's Macs. The first caller to run on the
  Linux fabric (`runs-on: corelink`) therefore looked for
  `/home/runner/.rustup/toolchains/1.91.1-x86_64-apple-darwin` and failed with
  *"the workspace-pinned toolchain … is not installed on this runner host"*,
  which reads as a defect in the host when the host was never asked for anything
  it could have. An error that accuses the wrong place costs more than a raw one.
  The triple is now detected from `uname -s` / `uname -m`, with the `HOST_TRIPLE`
  environment override still honoured (the fleet documentation already told
  callers to set it). The mapping is per-OS rather than one table because
  `uname -m` reports `arm64` on Apple Silicon and `aarch64` on Linux for the same
  architecture; an unrecognised OS fails loudly asking for an explicit
  `HOST_TRIPLE` instead of guessing. Behaviour on the Mac fleet is unchanged —
  Darwin + x86_64 resolves to exactly the previous default — and this was checked
  in both directions: detection finds the toolchain on a Darwin host, while
  forcing `HOST_TRIPLE=x86_64-unknown-linux-gnu` there still fails as it should.
  Same class as the hand-written `nightly-x86_64-apple-darwin` `$GITHUB_PATH`
  lines catalogued by WP-CI, but hidden inside a shared script, which is why a
  sweep for `apple-darwin` across `.github/workflows/` did not find it.

- **`corelink-reapi::mutants-nightly` was about to reach the fleet with no
  `timeout-minutes`.** It would have inherited GitHub's 360-minute default — the
  exact hazard `pr-gate` was given a 25-minute bound for in the same file. It now
  carries an explicit 180-minute ceiling, documented in-file as *derived, not
  measured*: the job has never executed (it is `if: github.event_name ==
  'schedule'` and this workflow's cron was removed on 2026-08-02), so there is no
  median to take. Half the inherited default caps a wedged box at 3 h instead of
  6 h; if a real run ever hits it, the answer is to shard the mutant set, not to
  raise the ceiling.

### Changed

- **`sbom.yml` was pulled out of the G-A2 batch and left on the Mac fleet.**
  Its `Install CycloneDX CLI` step selects the download with
  `case "$(uname -s)/$(uname -m)"` and has arms for `Darwin/arm64` and
  `Darwin/x86_64` only; the `*)` arm exits 1, and the Linux digest was previously
  removed from the `env:` block as a placeholder that pinned nothing. Moving the
  five jobs to `runs-on: corelink` (Ubuntu 24.04 / Firecracker) would have made
  that fall-through a *certain* failure — and `sbom.yml` fires only on
  `release: published` + `workflow_dispatch`, so no PR would ever have shown it.
  It would have surfaced on release day, with `sbom-tsa-attest` and
  `sbom-release-upload` chained behind it by `needs:`, i.e. no SBOM in the
  release assets. The migration needs a real `cyclonedx-linux-x64` SHA-256 pin,
  `sha256sum` instead of `shasum -a 256` (the fleet has coreutils, not the macOS
  tool), and a green `workflow_dispatch` as proof — which is a PR of its own, not
  a line in a six-file batch.
