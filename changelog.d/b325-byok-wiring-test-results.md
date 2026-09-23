### Fixed

- # B-325: propagate BYOK wiring-test failures

  The BYOK revocation wiring tests now return setup, cycle and status-read
  failures through their test results instead of panicking with `expect()`, so
  strict all-targets Clippy can inspect the complete integration-test target.
