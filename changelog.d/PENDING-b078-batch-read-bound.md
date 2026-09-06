### Fixed

- Bound native CAS `batch-read` fan-out and enforce the 8 MiB read ceiling before
  storage bodies are collected, returning `413 batch_too_large` for over-cap
  objects or aggregate responses.
