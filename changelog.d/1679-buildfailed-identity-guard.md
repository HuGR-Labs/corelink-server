### Fixed

- **B-250 workflow identity classification no longer matches by name alone.** The
  read-only Actions diagnostic binds deleted `BuildFailed` startup failures to
  exact workflow ID and path, while active similarly named classification runs
  remain generic startup evidence; malformed, partial, or conflicting workflow
  identity fails closed.
