# Precision filter — consumer run

- Audited: 16 captures
- Kept (signal): 14

## Class counts

- **signal**: 14
- **noise**: 0
- **duplicate**: 2
- **agent_meta**: 0
- **ephemeral**: 0

## Per-capture verdicts

| label | class | reason |
|---|---|---|
| `replication-region-resolver-reexports-worker-region` | duplicate | It restates the worker-region-vs-replication-region capture — that corelink_replication's Region is the same as corelink_worker's Region with only the Wnam variant. |
| `worker-region-vs-replication-region` | duplicate | It restates replication-region-resolver-reexports-worker-region and region-enum-variants-jurisdiction content about the two Region enums and their variants, materially overlapping with existing captures. |
| `authscope-variants-cacheread-cachewrite` | signal | Atomic fact about CoreLink's AuthScope enum variants and their semantics, useful in future project sessions. |
| `cargo-brew-adapter-port-error-types` | signal | Atomic fact about CoreLink's cargo/brew adapter port traits using port-local error types and &str params, useful in future project sessions. |
| `cas-read-request-principal-is-string` | signal | States a concrete, atomic fact about CoreLink's code (CasReadRequest.principal type) useful for future bridge work in this repo. |
| `corelink-adapter-host-crate-purpose` | signal | Atomic project fact about CoreLink's corelink-adapter-host crate purpose, contents, and LOC rule, distinct from other corpus captures. |
| `inmemory-cas-verifies-fake-hash` | signal | States a specific, atomic operating constraint of CoreLink's InMemoryCasHandler stub useful in future project sessions. |
| `kvbackend-rpitit-not-object-safe` | signal | States a concrete, atomic technical fact about CoreLink's KvBackend trait and its object-safety constraint useful for future project sessions. |
| `oci-blobstore-chunked-upload` | signal | Atomic, real fact about CoreLink's OCI adapter port-to-SPI mapping that is useful in future project sessions and not duplicated elsewhere in the corpus. |
| `pat-validator-authenticate-signature` | signal | Atomic, verifiable fact about a specific function signature in the CoreLink repo, useful for future project sessions and distinct from other corpus captures. |
| `patvalidator-not-debug-bound` | signal | Atomic project-specific fact that PatValidator lacks a Debug bound, forcing manual Debug impls on named tenant bridges in corelink-adapter-host. |
| `region-enum-variants-jurisdiction` | signal | Atomic, concrete fact about CoreLink's Region enum variants and the Weur→EU jurisdiction binding, useful in future project sessions. |
| `region-not-reexported-from-reapi` | signal | Concrete, atomic fact about CoreLink's module structure (where Region must be imported from), useful in future project sessions and distinct from the related re-export captures. |
| `secrecy-per-crate-not-workspace` | signal | Concrete, atomic fact about CoreLink's dependency management — secrecy 0.10 pinned per-crate rather than via workspace dependencies. |
| `secretwrap-expose-returns-str` | signal | States a concrete operating rule about CoreLink's codebase—using SecretWrap::expose() at a specific path instead of secrecy's expose_secret to avoid the dependency. |
| `stubpatvalidator-insert-signature` | signal | Records a concrete API signature for StubPatValidator::insert in the CoreLink repo, useful in future project sessions and distinct from the other corpus captures. |