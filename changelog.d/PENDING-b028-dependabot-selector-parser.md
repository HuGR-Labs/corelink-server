### Security

- **The two patchable `postcss-selector-parser` Dependabot alerts are remediated
  without a broad dependency refresh (B-028).** Root pnpm overrides now resolve
  the vulnerable 6.1.2 and 7.1.1 transitive paths to 6.1.3 and 7.1.3,
  respectively, and the frozen lockfile contains no vulnerable parser entry.
  The formerly unpatched build-time `image-size` and `extract-zip` paths are
  contained by audited local substitutes under `vendor/`, while `fast-uri` and
  `qs` resolve to published patched versions. The authoritative old-main
  census remains open until the candidate is merged and GitHub's alerts are
  refreshed; the verifier rejects any false zero-census or audit-ignore masking.
