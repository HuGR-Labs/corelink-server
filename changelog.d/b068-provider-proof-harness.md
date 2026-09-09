### Fixed

- **B-068 provider proofs use the migration-owned `blob_meta` table and require a
  dedicated staging R2 bucket.** Live R2 probes also clean up their exact test keys
  before asserting results.
