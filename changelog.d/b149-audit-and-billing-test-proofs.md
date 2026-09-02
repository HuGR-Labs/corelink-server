### Fixed

- **Hardened B-149 audit and billing ingest regression proofs.** Empty audit batches now explicitly prove they emit zero D1 statements, missing ingest-secret coverage is independent of ambient process environment, and every record-validation rejection has a focused fixture.
  The exhaustive rejection matrix also pins every stable client-visible reason code.
