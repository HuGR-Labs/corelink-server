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
