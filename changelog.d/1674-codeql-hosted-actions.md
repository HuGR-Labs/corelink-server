Fixed: restore CodeQL pull-request, nightly, and manual scans on GitHub-hosted
`ubuntu-latest` runners now that the canonical repository is public. The Rust,
JavaScript/TypeScript, and Python matrix remains SHA-pinned, and SARIF upload
failures now fail the job instead of being treated as a healthy scan.
