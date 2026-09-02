### Security

- **The two patchable `postcss-selector-parser` Dependabot alerts are remediated
  without a broad dependency refresh (B-028).** Root pnpm overrides now resolve
  the vulnerable 6.1.2 and 7.1.1 transitive paths to 6.1.3 and 7.1.3,
  respectively, and the frozen lockfile contains no vulnerable parser entry.
  B-028 remains open for the separately tracked, unpatched high `image-size`
  and `extract-zip` alerts; this does not dismiss or mask them.
