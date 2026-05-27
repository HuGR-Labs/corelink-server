# corelink cargo/brew adapter port error types
ops: x
vars: CARGO=corelink-adapter-cargo  BREW=corelink-adapter-brew

CARGO + BREW port traits use port-local `CasError`/`TenantResolveError`;
x crate-level `CargoAdapterError`/`BrewAdapterError`.

params: `&str`; x `&TenantId`; x `&Digest`.

refs: [[cas-read-request-principal-is-string]] [[inmemory-cas-verifies-fake-hash]]