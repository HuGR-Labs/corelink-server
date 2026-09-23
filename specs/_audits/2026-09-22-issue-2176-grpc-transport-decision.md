---
id: "I2176-GRPC-TRANSPORT-DECISION"
type: "architecture-decision"
doc_status: "ACTIVE"
audit_status: "OPEN"
version: "1.0.0"
created: "2026-09-22"
updated: "2026-09-22"
owner: "CoreLink server"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["issue-2176", "grpc", "cloudflare-workers", "fail-closed"]
---

# Issue #2176 — Worker → DO → Container gRPC transport decision

## Decision

**BLOCKED — fail closed.** `corelink-api.humangr.com` has no public gRPC or
REAPI mount. No service-mount child of #2067 may add one until the exact
Worker → Durable Object → Container transport is proven on a protected,
nonproduction Cloudflare deployment.

The present Worker path is a Fetch proxy, not a byte-transparent HTTP/2 path:
`worker/src/durable_object_probes.ts::proxyToContainer` constructs a new
`Request` and sends it with `TcpPort.fetch()`. Its public interface has no
HTTP version, HTTP/2 stream, trailer, or peer-cancellation control. A source
test can prove that this shape is unchanged; it cannot prove HTTP/2 framing or
gRPC terminal trailers survive a deployed provider hop.

Cloudflare's current public documentation makes the distinction decisive:

1. [Workers protocols](https://developers.cloudflare.com/workers/reference/protocols/)
   documents a `fetch()` handler for inbound HTTP/HTTPS and says inbound direct
   TCP is coming soon.
2. [Cloudflare's gRPC announcement](https://blog.cloudflare.com/grpc-workers/)
   says Fetch does not expose HTTP/2 stream control, describes socket piping
   through Worker → DO → `getTcpPort().connect()`, and states that this is a
   **private beta**.
3. [The Container API](https://developers.cloudflare.com/durable-objects/api/container/)
   documents both `getTcpPort().fetch()` and raw `getTcpPort().connect()`.

The repository has neither the private-beta socket handler/API types nor an
approved Spectrum ingress. The ordinary public gRPC network setting is not a
substitute: it protects a TLS+HTTP/2 origin endpoint, while this deployment
terminates at a Worker Fetch handler. Enabling that setting, using HTTP/1,
gRPC-Web, REST transcoding, a local proxy, or a cache fallback would not meet
this contract.

## Frozen transport contract

The only admissible future topology is:

```text
standard gRPC client (TLS + HTTP/2)
  → approved private-beta/Spectrum socket ingress
  → Worker connect(socket)
  → tenant-selected Durable Object connect(socket)
  → Container.getTcpPort(50051).connect(...)
  → tonic gRPC diagnostic service
```

Every segment MUST stream raw bytes bidirectionally. The diagnostic service is
non-secret and non-mutating. It must provide one unary RPC and one
bidirectional-streaming RPC, return an opaque response metadata marker, and
finish with a nonzero-free `grpc-status: 0` trailer. The client deadline must
cancel the in-flight stream at the container; a terminal cancellation is not a
successful response.

## Acceptance evidence and Definition of Done

A future implementation may open public ingress only after one GitHub-hosted,
protected-environment receipt names the exact deployed Worker and container
SHAs, endpoint host, provider feature state, and standard gRPC client version.
It must contain no credentials, request payloads, PAT values, or token-derived
material. The receipt must prove all of these against that exact deployment:

| Case | Required observation |
| --- | --- |
| Unary framing | exact request and response bytes round trip under `application/grpc`; ALPN is HTTP/2. |
| Metadata | `authorization` and a binary metadata value reach the container unchanged; no value is emitted in logs or artifacts. |
| Response metadata | the opaque diagnostic marker reaches the client. |
| Trailers | the client observes terminal `grpc-status: 0` and the expected diagnostic trailer. |
| Deadline | an expired client deadline reaches the container as cancellation and produces no success receipt. |
| Streaming | ordered bidirectional messages survive both directions without buffering the full stream. |
| Negative HTTP/1 | an HTTP/1-only listener/composition is rejected before any REAPI service is mounted. |

The hosted verifier for this decision must reject a deleted private-beta
qualification, a Fetch-only claim, any gRPC public route, and a workflow that
does not check out the exact pull-request head without credentials. That is the
complete nonproduction proof available without provider mutation or secrets;
it is evidence of a block, not evidence that gRPC is served.

## Invariants

- No public gRPC/REAPI endpoint, Buck2 runtime dispatch, cache write, or
  endpoint claim is enabled by this issue.
- No REST, HTTP/1, gRPC-Web, local proxy, local cache fallback, or protocol
  conversion is accepted as transport proof.
- No PAT or authorization value is logged, persisted, placed in an artifact,
  or used as a fixture.
- Provider uncertainty, absent private-beta access, a missing receipt, a
  missing trailer, altered metadata, an HTTP/1 downgrade, or failed streaming
  leaves ingress absent.

## Re-open condition

The owner must first authorize the Cloudflare private-beta/Spectrum provider
change and a protected nonproduction deployment. Then a focused follow-up may
implement the socket-only diagnostic path and collect the receipt above. Until
then #2176 is resolved by this fail-closed architecture decision and continues
to block #2067 service mounting.
