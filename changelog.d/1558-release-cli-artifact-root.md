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

- **CLI releases are now staged privately until the complete signer/SLSA chain
  proves them.** A canonical manifest binds the tag, source SHA, filenames and
  digests before signing; reusable Linux, Windows and macOS lanes verify that
  identity and their signer credentials before transforming an archive. A
  missing signing identity leaves the release draft instead of publishing
  unsigned assets, while successful lanes produce a final manifest that SLSA
  verifies before the release is made public.
