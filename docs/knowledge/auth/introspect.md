---
type: "AuthMechanism"
title: "Introspection endpoint (runners fabric authz)"
description: "The internal introspection endpoint verifies a PAT and returns its tenant, plan, and runner entitlement to the compute fabric."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect/part-00-01.rs"
  - "crates/corelink-container/src/routes/auth_introspect/part-00.rs"
source_blobs:
  - "crates/corelink-container/src/routes/auth_introspect/part-00-01.rs@0648b938a24681c83a9c0c4af68fa8f2386c6471"
  - "crates/corelink-container/src/routes/auth_introspect/part-00.rs@62d7d3db5ba1af3f33aea46919a092e52d228f0b"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "pat", "introspect", "runners", "fabric"]
timestamp: "2026-06-26T00:00:00Z"
---

# Introspection endpoint (runners fabric authz)

The runners fabric sends a PAT to the internal `POST /internal/v1/auth/introspect` endpoint rather than implementing credential verification itself. The handler checks the internal consumer key before parsing the body, then calls the shared container PAT verifier. Invalid credentials return a uniform invalid response; verifier backend faults return 503 (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:86-145`).

For a valid PAT, the endpoint resolves the tenant's plan and runner entitlement with separate D1 lookups. The runner concurrency and monthly vCPU-hour cap come from `runners_entitlement`, independently of the plan tier. A missing entitlement produces no runner cap; a malformed stored value or D1 fault fails closed (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:35-85`; `crates/corelink-container/src/routes/auth_introspect/part-00.rs:320-452`).

# Invariants

- Authentication runs before body parsing, and no PAT is verified for an unauthenticated caller (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:103-151`).
- The response tenant comes from the verified PAT. Plan and runner entitlement remain independent reads (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:103-145`).
- D1 faults and out-of-contract entitlement values return an error instead of inventing an entitlement (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:320-452`).

# Citations

1. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:86-145` — endpoint route and authenticated request handling, including shared PAT verification and response mapping.
2. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:35-85` — plan lookup and validation.
3. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:320-452` — runner entitlement lookup and fail-closed decoding.
