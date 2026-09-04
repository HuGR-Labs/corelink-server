### Security

- **B-161: pin authenticated Homebrew downloads to CoreLink without upstream fallback.**
  The four published locale pages now document the bearer token contract with
  `HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1`, preventing a mirror failure from
  sending a CoreLink credential to `ghcr.io`; the focused five-test Vitest
  suite verifies exactly four locale pages and kills three mutations: missing
  the pin, exporting the token outside the safe block, and restoring the
  legacy `HOMEBREW_BOTTLE_DOMAIN` assignment.
