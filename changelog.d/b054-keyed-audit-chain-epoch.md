### Added

- **B-054 versioned audit-chain cutover primitives.** Existing unkeyed BLAKE3
  rows remain byte-for-byte verifiable as E0; new E1+ links use the exact
  domain-separated keyed-v2 formula through a forward-only `ChainEpoch` state
  machine. Archive lines now carry algorithm/epoch/key-id metadata and reject
  downgrade, missing/wrong keys, and unknown versions. Additive D1 migrations
  preserve historical rows and the legacy drain refuses to downgrade a v2 head.
- Sealed-row metadata is now fail-closed at every boundary: only all-NULL
  legacy, explicit `(0,0,NULL)` E0, or complete `(1,epoch>0,key_id>0)` keyed
  tuples are accepted. Archive/drain readers reject partial or negative values,
  and the sealed-tail query includes `link_key_id`.

### Open / operator action

- Production activation remains gated on the independently administered
  linearizable head witness, transactional epoch-ledger/D1 binding, secret
  custody and rotation, archive coverage, and independent verification proof.
  No key material is stored in D1, R2, source, or CI output.
