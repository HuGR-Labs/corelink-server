### Added

- **B-165 bounded latency monitor.** The read-only probe and fail-closed TSV
  verifier retain refusal/control measurements on a six-hour schedule. A
  refusal-only run exits as `partial/open`; closure requires complete served
  populations for two distinct customer tenants and never accepts credentials
  or log bodies in the evidence artifact.
