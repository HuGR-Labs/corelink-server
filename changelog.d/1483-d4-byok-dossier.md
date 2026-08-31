### Fixed

- **D-4 go-live validation: the shipped binary compiles the BYOK provider whose
  own doc-comment says "Not for production".** Kept the three levels
  apart — exists / linked / reached — because a crate that compiles proves only
  the first. **Exists:** `crates/corelink-byok`, 57 files, real AWS/GCP/Azure/
  Vault providers. **Linked:** `cargo tree -p corelink-server --edges normal`
  shows it at depth 1 (control: `corelink-handler-cas` → 5 hits), so this is read
  off the binary's dependency graph, not off the crate. **Reached — and this is
  the finding:** provider selection is COMPILE-TIME
  (`byok_orchestrator.rs::build_active`), `corelink-server` declares
  `[features] default = []`, and the Dockerfile builds with **zero** occurrences
  of `--features`, so the `#[cfg(not(any(...)))]` fallback ships:
  `InMemoryFake::new()`. That type's doc-comment reads **"Not for production"**
  and describes wrapping a DEK by XOR-ing it against `IN_MEMORY_FAKE_MASK`, a
  hard-coded 32-byte constant, adding that it *"offers no cryptographic
  confidentiality"*. ⚠️ **Correcting the prevailing shorthand:** "BYOK is an
  in-memory XOR" is right about the provider but is sometimes supported by citing
  `aad_fingerprint`, which is an 8-byte AAD tamper-check used in **mock mode**,
  not the key wrap — the finding does not need it and citing it weakens the case.
  **Second, independent finding:** `tenant_byok_config` and `byok_envelope` are
  both **empty** while **262 of 262** tenants carry `byok_status = 'active'` —
  because `migrations/d1/0031` declares that column `DEFAULT 'active'`. A status
  column whose default is the affirmative value cannot distinguish "on" from
  "never asked". Dossier in `reports/go-live/D-4-byok.md`, including the four
  questions it does not decide and the cheapest next check (the boot audit line
  at `corelink.byok.orchestrator.audit` names the active provider).
