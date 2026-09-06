### B-079 — tier Max and unknown-tier rate resolution

- Confirms the published Max contract: Business bucket at 1,000 RPS / 5,000 burst,
  matching the $149 pricing card.
- Makes unknown enum variants use the same Team default as unknown billing labels
  (200 RPS / 1,000 burst): availability-safe for a paid tenant without silently
  granting Enterprise capacity.
- Adds a fail-closed structural verifier with numeric source comparison and
  adversarial mutation checks.
