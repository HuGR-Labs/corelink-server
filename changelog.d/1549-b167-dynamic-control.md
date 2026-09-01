### Fixed

- **B-167's canonical-ID proof no longer collides with the next real backlog item.**
  The verifier now derives an absent canonical probe from the live maximum ID,
  while retaining mutation-tested rejection of zero-padded aliases.
