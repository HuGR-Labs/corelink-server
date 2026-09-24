---
type: "AuthMechanism"
title: "Native HMAC fast-reject PAT gate"
description: "The container's native cache surfaces verify PATs through a bounded HMAC and possession gate with a short-lived cache."
source_files:
  - "crates/corelink-container/src/native_pat_gate.rs"
source_blobs:
  - "crates/corelink-container/src/native_pat_gate.rs@cd9bcc9414778fbac1e47efe7bf89e6c712a0ffa"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "pat", "security", "hot-path", "native-plane"]
timestamp: "2026-06-26T00:00:00Z"
---

# Native HMAC fast-reject PAT gate

Native cache routes call `NativePatGate` to validate the bearer and ensure it belongs to the tenant supplied by the trusted Worker boundary. The gate fingerprints the token, checks its short-lived verification cache, and on a miss invokes the shared `PatVerifier`, which performs the HMAC fast-reject, D1 row check, Argon2id proof, and scope gate (`crates/corelink-container/src/native_pat_gate.rs:159-259`).

The five-second verification cache stores the resolved tenant and write capability. Cache hits still check the requested tenant and reject a read-only PAT on a write operation. The bounded TTL limits the revocation window; a cache miss re-runs the verifier (`crates/corelink-container/src/native_pat_gate.rs:57-70`; `crates/corelink-container/src/native_pat_gate.rs:200-289`).

# Invariants

- A forged token, unknown row, expired or revoked PAT, wrong tenant, or insufficient scope is denied by the shared verifier or gate (`crates/corelink-container/src/native_pat_gate.rs:159-259`).
- A cached result cannot cross tenant boundaries or grant write access to a read-only PAT (`crates/corelink-container/src/native_pat_gate.rs:184-259`).
- Cached verification expires after the fixed five-second lifetime (`crates/corelink-container/src/native_pat_gate.rs:57-70`; `crates/corelink-container/src/native_pat_gate.rs:262-289`).

# Citations

1. `crates/corelink-container/src/native_pat_gate.rs:159-259` — public read/write verification and the shared verifier call.
2. `crates/corelink-container/src/native_pat_gate.rs:57-70` — fixed five-second cache TTL and bounded revocation window.
3. `crates/corelink-container/src/native_pat_gate.rs:184-259` — cached tenant and write-capability checks.
4. `crates/corelink-container/src/native_pat_gate.rs:262-289` — cache expiry and insertion.
