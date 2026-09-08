### Fixed

- **B-126 file-size ratchet:** the trusted Git-tree gate now measures the closed
  source census, rejects malformed/non-source baseline rows, covers fork pull
  requests without an author-association bypass, and keeps inherited baseline
  drift from becoming an excuse to grow. The API-reference renderer was split
  into a focused module and B-126 remains open while 70 oversized files are
  refactored.
