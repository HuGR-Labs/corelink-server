### Fixed

- **B-268 restores the customer-handler request split.** The keys response
  module now imports the shared `ByokStatus` type from its overview sibling,
  restoring workspace build and Clippy resolution without duplicating the type.
