### Fixed

- **The CLI release root now exposes the artifacts its consumers actually
  request.** `release-cli` verifies the pinned Zig and `cargo-zigbuild` versions
  on every Linux/Windows leg and creates an isolated wrapper cache, so a stale
  shared zig wrapper cannot be mistaken for a source-build failure. Each leg now
  publishes both the raw installer binary and the matching `.tar.gz`/`.zip`
  signing input, each with a SHA-256 digest. Before succeeding, the release job
  downloads the public release root again and verifies `checksums.txt` against
  those uploaded bytes. The cross-repository token remains confined to the
  publication job; compilation does not receive it.
