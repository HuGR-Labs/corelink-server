# Ownership wave 005 — adapters, identity and configuration

This wave uses [WAVE_001_PLAN.md](WAVE_001_PLAN.md#frozen-artifact-contract).
Authors use source/static evidence only and create exactly four owned files.
No Cargo execution, network, provider/runtime operation, publication, source
edit, or shared registry/index/status edit is authorized.

| WP | Package | Manifest | Profile | Owned paths |
|---|---|---|---|---|
| W005-CLOUD | `corelink-adapters-cloud` | `crates/corelink-adapters-cloud/Cargo.toml` | S | `own-corelink-adapters-cloud`, `crates/corelink-adapters-cloud/` |
| W005-VAULT | `corelink-adapters-vault` | `crates/corelink-adapters-vault/Cargo.toml` | S | `own-corelink-adapters-vault`, `crates/corelink-adapters-vault/` |
| W005-CLERK | `corelink-clerk` | `crates/corelink-clerk/Cargo.toml` | H | `own-corelink-clerk`, `crates/corelink-clerk/` |
| W005-CLERKCF | `corelink-clerk-cf` | `crates/corelink-clerk-cf/Cargo.toml` | H | `own-corelink-clerk-cf`, `crates/corelink-clerk-cf/` |
| W005-CONFIGDO | `corelink-config-do` | `crates/corelink-config-do/Cargo.toml` | S | `own-corelink-config-do`, `crates/corelink-config-do/` |
| W005-WASM | `corelink-wasm` | `crates/corelink-wasm/Cargo.toml` | S | `own-corelink-wasm`, `crates/corelink-wasm/` |

## Static anchors

- **Cloud/Vault:** canonical re-export facades only. Public path ownership
  differs from provider implementation, target selection, migration and
  runtime operation. Vault enables the BYOK `vault` feature statically; that
  does not prove selection or a Vault session.
- **Clerk:** adapter/JWKS/cache/principal/config surfaces with feature gates
  `jwt-adapter`, `http-fetcher`, `test-utils`; explicitly distinguish declared
  feature, source path and native/wasm reachability. No live JWKS/JWT/identity
  validation may be claimed.
- **Clerk-CF:** Worker/wasm composition and optional feature paths are high
  complexity. Separate local binding code from `worker`, CF bindings, Clerk,
  materializer/audit/statuspage providers and actual Worker/DO/KV/D1/R2
  operation. Target declaration is not build or deployment proof.
- **Config DO:** local schema/hash/validation/store/propagation/metrics
  interfaces. In-memory CAS/audit ordering does not prove Durable Object, D1,
  Prometheus, propagation or rollback operation.
- **WASM:** publish-false cdylib wrapper over client-verify; wasm-bindgen and
  package metadata are static interfaces, not npm publication, wasm build,
  browser execution or JS compatibility proof.

Fresh cold review returns four individual verdicts and challenges every
feature/target/re-export/provider/runtime claim. The lead alone integrates,
updates shared state and controls publication.
