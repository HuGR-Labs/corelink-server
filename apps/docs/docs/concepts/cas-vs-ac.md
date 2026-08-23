---
id: cas-vs-ac
title: Content-Addressable Storage and Action Cache
sidebar_position: 1
description: How CAS and AC differ, when each is used, and how Bazel uses both together.
---

# Content-Addressable Storage and Action Cache

CoreLink exposes two distinct caches that work together. Most users only think about one — the one that stores their build outputs — but understanding both is worth the 5-minute read.

## Content-Addressable Storage (CAS)

CAS stores arbitrary blobs keyed by their BLAKE3 digest. The key *is* the digest: there is no separate filename, version tag, or metadata required.

```
key:   e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85   (BLAKE3, 64 lowercase hex)
value: <bytes>
```

Properties:

- **Immutable**: once a blob is stored at a given digest, the content at that digest never changes.
- **Deduplicated**: if two tenants (or two CI jobs) upload the same bytes, the storage layer stores one copy. Both tenants pay for access, not for duplicate storage.
- **Verifiable**: the client computing `blake3(downloaded_bytes)` will always match the key used to retrieve it — the write path re-hashes the uploaded bytes and rejects a mismatched claim with `422`.

### CAS in the REAPI context

In the [Remote Execution API](https://github.com/bazelbuild/remote-apis), CAS is used for:

- Source file contents (inputs to actions)
- Compiled object files and final artifacts (outputs of actions)
- `Directory` proto messages that describe the input tree

Bazel uploads inputs to CAS before dispatching a remote action. The executor reads inputs from CAS, runs the action, and uploads outputs back to CAS.

### CAS HTTP endpoints

```
PUT /v1/cas/<tenant_id>/<blake3>   body: raw bytes  → 201 (fresh) / 200 (idempotent) + bare digest as the response body
GET /v1/cas/<tenant_id>/<blake3>                    → 200 + raw bytes
```

See the [HTTP API reference](../api/http.md) for full details.

## Action Cache (AC)

The Action Cache maps an **action digest** to an **action result**. An action digest is the SHA-256 of a serialized `Action` proto — it encodes the command, input tree, and platform properties deterministically. The action result records the output digests, exit code, and timing.

```
key:   sha256(<Action proto>)
value: ActionResult { output_files: [...], exit_code: 0, ... }
```

Properties:

- **Skips redundant work**: if `action_digest` is in AC, the build tool fetches the cached outputs from CAS and skips re-running the action.
- **Tenant-scoped**: AC entries from one tenant are never visible to another.
- **Invalidated by any input change**: because the key is the digest of inputs + command, any change to source files, flags, or the toolchain produces a different key — the cache misses cleanly.

### When Bazel uses AC

```
build tool
  1. Compute action_digest = sha256(Action{command, inputs, platform})
  2. GET /ac/<tenant>/action_digest  → HIT: fetch outputs from CAS, done
                                     → MISS: run action locally (or remotely)
  3. On success: PUT /ac/<tenant>/action_digest  → store result
                 PUT /v1/cas/<tenant>/<output_hash> → store each output
```

### Diagram: CAS + AC together

```
        ┌─────────────────────────────────────────────────┐
        │                   Bazel client                   │
        └───┬─────────────────────────────────┬───────────┘
            │ 1. check AC                      │ 3. PUT outputs to CAS
            ▼                                  ▼
     ┌─────────────┐                   ┌────────────────┐
     │ Action Cache│   2. AC miss →    │ Build executor │
     │  (AC)       │   run action      │ (local or RE)  │
     └─────────────┘                   └────────────────┘
            │ 4. write result back              │
            └──────────────────────────────────►│
                                                │ 5. read outputs from CAS
                                                ▼
                                       ┌────────────────┐
                                       │      CAS        │
                                       └────────────────┘
```

## Comparison

| | CAS | Action Cache |
|---|---|---|
| Key | BLAKE3 of content | SHA-256 of Action proto |
| Value | Raw bytes | ActionResult (output digests, exit code) |
| Immutable | Yes | Yes (entries are not updated, only written once) |
| Used for | Blobs (files, protos) | Build action memoization |
| Available without REAPI | Yes (REST API) | Via REAPI gRPC only |
| Turborepo | Not directly (Turbo uses its own artifact format) | Turborepo `TURBO_API` maps to this concept |

## What Turborepo calls "remote cache"

Turborepo does not expose CAS/AC as distinct concepts. Its remote cache API is a simplified HTTP protocol where task outputs are stored by a hash of the task inputs. CoreLink exposes a Turborepo-compatible endpoint — see [Turborepo integration](../integrations/turborepo.md).
