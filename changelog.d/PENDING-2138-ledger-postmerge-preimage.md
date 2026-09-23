### Fixed

- **Ledger post-merge snapshot validation.** Historical B-373 evidence now
  validates against its pinned commit preimage instead of treating later live
  ledger transitions as snapshot corruption.
