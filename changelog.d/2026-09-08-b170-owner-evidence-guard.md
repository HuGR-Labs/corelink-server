### Fixed

- **B-170 now fails closed on unsafe owner-evidence paths.** The dedicated guard
  rejects symlinks, directories, empty files, and packet marker drift; missing
  evidence remains the truthful `open` state, while three present artifacts
  deliberately require manual owner validation. No legal, PagerDuty, or
  notification result is inferred from repository file presence.
