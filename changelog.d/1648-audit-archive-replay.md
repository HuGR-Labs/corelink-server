### Changed

- Add a deterministic, read-only replay verifier for the B-063 audit archive
  partition incident (#1648). Archive row selection now has a stable ID tie
  break for duplicate sequence numbers, and watermark updates assert the
  selected tenant and region.
