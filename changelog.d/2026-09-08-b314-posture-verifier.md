### Fixed

- **B-314's open gate now follows the current Sigstore disclosure shape.** The
  posture check folds Markdown line wrapping and accepts the measured
  no-certificate/entry result after the retired OCI lane review, while still
  rejecting restored customer-data claims through its mutation suite. This
  keeps the Legal/DPO transfer-table decision pending and does not close it.
