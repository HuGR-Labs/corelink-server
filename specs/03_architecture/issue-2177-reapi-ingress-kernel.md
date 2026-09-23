# Issue #2177: authenticated REAPI ingress kernel

## Frozen boundary

`ReapiIngress` is built from the D1-backed `adapter_pat::PatVerifier`, the
existing quota authority, and the already-decorated `CasReadHandler`,
`CasWriteHandler`, `AcLookupHandler`, and `AcUpdateHandler` trait objects from
`routes::build_with_factory_and_byok`. It does not create storage or mount a
network service. The normal router builder keeps its current return type; the
explicit `build_with_factory_and_byok_and_reapi_ingress` entrypoint returns the
same router with an optional kernel when production auth, quota admission, and D1 tenant-cap resolution are all
configured.

Every future cache RPC follows this order:

1. Parse one bearer authorization value without retaining or logging it.
2. Verify it through the authoritative PAT verifier and derive tenant plus
   write capability from the verified row.
3. Require an exact tenant instance name and reject reserved namespaces.
4. Enforce read/write capability.
5. Acquire the single shared quota/concurrency admission lease.
6. Obtain storage handlers from the admitted request context.

The stable status mapping is `UNAUTHENTICATED` for absent, malformed, invalid,
revoked, or scope-less credentials; `UNAVAILABLE` for verifier/admission
uncertainty; `PERMISSION_DENIED` for cross-tenant instance names and writes
without write capability; and `RESOURCE_EXHAUSTED` for quota or concurrency
exhaustion. Resource and digest validation uses one central SHA-256 helper and
one exact ByteStream blob resource-name parser.

## Dependency gate

The kernel stays unmounted. Public gRPC composition waits for the exact
Worker → Durable Object → container transport proof in #2176, followed by the
CAS, ByteStream, and ActionCache service contracts (#2178, #2181, #2182).

## Hosted acceptance

The credentialless exact-head workflow runs the kernel's Rust behavior tests
and a small mutation contract. It covers every rejection/status path, validates
that rejection happens before admission/storage access, checks canonical SHA-256
and REAPI resource names, and verifies that the router factory passes the same
decorated handler trait objects into the kernel. The workflow has read-only
repository permission and does not contact production or use PAT credentials.
