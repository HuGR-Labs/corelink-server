# Precision filter — consumer run

- Audited: 16 captures
- Kept (signal): 15

## Class counts

- **signal**: 15
- **noise**: 0
- **duplicate**: 1
- **agent_meta**: 0
- **ephemeral**: 0

## Per-capture verdicts

| label | class | reason |
|---|---|---|
| `corelink-audit-emitter-sole-workspace-trait` | duplicate | It restates corelink-audit-emitter-fail-closed's core claim that AuditEmitter is the sole sync fail-closed workspace trait consumed by adapters, materially overlapping rather than adding atomic new fact. |
| `corelink-adapter-inline-ports-mandate` | signal | States a specific architectural rule for CoreLink adapters (inline-ports mandate, no workspace SPI imports) useful in future project sessions. |
| `corelink-adapter-test-floor-10` | signal | Records a concrete CoreLink-specific operating rule (≥10-test SEAL gate floor for adapter crates) useful in future project sessions. |
| `corelink-audit-before-mutation` | signal | States a concrete operating rule specific to CoreLink adapters (audit event before every state mutation), useful in future project sessions. |
| `corelink-audit-emitter-fail-closed` | signal | States a real project-specific design fact—that CoreLink's AuditEmitter trait is the canonical chokepoint enforcing the audit-fail-closed contract—distinct from the sole-workspace-trait capture. |
| `corelink-axum-server-body-via-bodyext` | signal | Records a CoreLink-specific implementation decision to use http_body_util::BodyExt over futures_util for request body collection in its axum adapter servers. |
| `corelink-axum-version-alignment` | signal | States a concrete dependency-pinning decision specific to CoreLink's stack (axum 0.7 aligned with tonic 0.12/hyper 1 and specific feature flags). |
| `corelink-cargo-adapter-no-upstream-fetch` | signal | States a concrete architectural fact about CoreLink's cargo (sccache) adapter lacking an upstream fetch path, useful in future project sessions. |
| `corelink-loc-cap-l210` | signal | States a specific CoreLink project rule (L2.10) capping .rs files at 500 LOC, which is a real atomic fact about this repo's operating conventions. |
| `corelink-no-must-use-on-router` | signal | A concrete CoreLink-specific anti-pattern rule about avoiding redundant #[must_use] on axum Router-returning functions to pass clippy under -D warnings. |
| `corelink-pat-constant-time-verify` | signal | States a concrete operating rule of CoreLink—that its adapters verify PATs via subtle::ConstantTimeEq by reference to avoid timing side-channels—which is project-specific and atomic. |
| `corelink-reqwest-rustls-config` | signal | States a concrete, atomic fact about CoreLink's actual workspace dependency configuration (reqwest using rustls-tls backend), useful in future project sessions. |
| `corelink-rust-safety-conventions` | signal | States concrete CoreLink-specific Rust safety conventions (non_exhaustive, forbid unsafe, no panics, SecretString+ConstantTimeEq) that are operating rules for this repo. |
| `corelink-subtle-choice-unwrap-u8` | signal | Records a concrete project-specific API detail about CoreLink's pinned subtle crate version (using .unwrap_u8() for ConstantTimeEq's Choice), useful in future sessions and atomic. |
| `corelink-unwrap-or-allowed-vs-bare-unwrap` | signal | States a specific operating rule of CoreLink's src/ code (unwrap_or allowed, bare unwrap/panic confined to test blocks), useful in future project sessions. |
| `corelink-workspace-spi-traits-fictional` | signal | Records a concrete fact about CoreLink's workspace—that named SPI traits don't exist/aren't needed by adapters and caused a v1 cargo packet to halt pre-mutation—useful in future project sessions. |