### Fixed

- **Correct B-087 procurement questionnaire claims and guard their evidence.** Corrected duplicated CAIQ v4 and SIG Lite answers for unavailable BYOK,
  unavailable R2 Object Lock/WORM, limited SAST/fuzz cadence, absent supply-chain
  attestations, and non-production synthetic paging. Preserved the substantiated
  signed-commit control.
- Added a bounded, fail-closed semantic verifier with mutation tests. It reports
  executed DPA/SLA conflicts and external PagerDuty/prospect follow-up as owner
  actions; it does not amend instruments or claim notifications occurred.
