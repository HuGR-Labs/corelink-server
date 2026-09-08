### Fixed

- **B-077 HOLD — close request-envelope gaps in native CAS.** Batch-read now
  reserves its 10 MiB request body, bounded parser metadata, and streamed 8 MiB
  response inside the shared capacity envelope that also covers BYOK's three
  read copies. Batch-write reserves manifest entries/strings alongside its body
  and payload; checked line/hash/count parsing rejects oversized metadata before
  another entry is retained. Authenticated tenant identifiers are bounded before
  route guards clone them.
- The per-tenant `CasReadSlot` is held through single-GET and batch-read body
  consumption/drop, so a slow response cannot multiply retained object bytes
  beyond the fairness guard. Tests cover stream lifetime, parser amplification,
  over-cap mutations, and the shared envelope arithmetic.

### Verification boundary

- Focal/static verification only; no global CI, production probe, or claim that
  the B-077 capacity acceptance state is live beyond this repository commit.
