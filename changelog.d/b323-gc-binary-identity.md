### Fixed

- # B-323: prevent GC binary artifact collisions

The server's production-capable GC target and the shipped in-memory GC
self-check now have distinct Cargo identities, preventing workspace builds
from overwriting the executable selected by integration tests. Subprocess test
failures also retain stderr for actionable diagnostics.
