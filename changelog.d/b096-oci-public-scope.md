### Changed

- **B-096 reconciles the network-effect claim with the implementation's real boundary.**
  Public pip wheels/sdists and Homebrew bottles share bytes cross-tenant; npm shares only
  unscoped metadata while tarballs remain per-tenant; OCI admits six owner-pinned digests
  (one layer plus five multi-arch image indexes) with verified blob upload/finalize writes
  and resolver closure promotion. Index-shaped blob bytes may use that digest-only path;
  manifest/index PUT remains tenant-scoped `ManifestKvStore` and never writes `_public`.
  Native CAS and private artifacts retain HMAC-derived tenant prefixes.
