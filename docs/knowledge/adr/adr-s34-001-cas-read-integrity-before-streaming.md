---
type: "ADR"
title: "ADR-S34-001 — CAS read integrity: the scrubber precedes streaming"
description: "Streaming CAS reads would remove the read-path digest re-verification, which is the only integrity coverage that exists, so the at-rest scrubber is a hard prerequisite; the read-path memory bound is a separate concern with a cheaper answer."
source_files:
  - "specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md"
checkpoint_sha: "2c0c92004dc041b5752dfd111bbd6c2d544bfbbe"
provenance: "AUTHORED"
tags: ["adr", "cas", "integrity", "streaming", "scrubber", "availability", "read-path"]
timestamp: "2026-08-26T00:00:00Z"
---

# ADR-S34-001 — CAS read integrity: the scrubber precedes streaming

Streaming CAS reads and a background integrity scrubber were being planned as two independent improvements. They are one ordered pair: the CAS read path re-verifies the content hash today, that check is the ONLY integrity coverage in the system, and streaming cannot preserve it. So the scrubber is a hard prerequisite, and the memory argument that motivated streaming turns out to be a separate concern with a cheaper answer already used elsewhere in the repo.

# Context

`R2CasHandler::read` re-hashes the bytes R2 returned and compares them against the requested digest before serving, emitting `AuditEventKind::CorrectnessViolation` and never a hit on mismatch (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:38-40`). Its purpose is stated in the code: it catches R2 bitrot, storage-tier tampering, and historically mis-keyed blobs — failure modes below the API that no write-path validation can see, because they occur after the write succeeded. `verify_content_hash` calls itself the single enforcement point for content-addressing on the durable path, covering the native CAS route, the Bazel REAPI v2 bridge and sccache (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:42-47`).

This is easy to grep past: the CAS *route* contains only `is_canonical_digest`, which validates that a digest is 64 hex characters and says nothing about content, so reading the route layer alone yields the confident and wrong conclusion that reads are unverified (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:49-53`). Nothing else covers integrity: no cron, job or sweep reads stored objects to check them, which makes the read-path check a sampling function driven by traffic where a cold object is never verified at all (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:55-67`).

# Decision

Three decisions. **The scrubber is a hard prerequisite for streaming**, because streaming does not weaken the read-path check — it removes it, taking the system from "every served object is verified" to "no object is ever verified" with no interval of overlap (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:69-84`). **The scrubber enumerates R2 directly and must not key on `blob_meta`**, which is empty in production and written by no code, so a scrubber built on it would enumerate zero objects and report success; `R2S3Client::list_objects_page` already provides cursor-driven, bounded enumeration, and the scrubber must report objects EXAMINED rather than only failures found (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:86-105`). **The read-path memory bound is a separate concern and not an argument for streaming**: this repo already answers it by reserving a concurrency permit before the body is buffered, on Turbo GET and PUT (per-tenant plus process-wide) and on CAS batch read — while the single CAS GET has neither (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:107-130`).

# Consequences

Streaming is blocked on the scrubber, which is a real and deliberate schedule cost. The scrubber's coverage metric is load-bearing rather than decorative: without an objects-examined counter, a scrubber that silently enumerates nothing is indistinguishable from a healthy one. When streaming does land it must state in its own ADR what replaced the read-path check, and accept that per-object verification moves from "on every read" to "whenever the scrubber last reached this key" (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:132-146`). The missing guard on `handle_read` is tracked separately; its severity depends on container sharding, which is one Durable Object per tenant and would make an unbounded read burst self-inflicted rather than cross-tenant — a reading the ADR explicitly marks as unproven (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:141-146`).

# Citations

1. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:38-47` — the read path re-verifies today: `R2CasHandler::read` re-hashes against the requested digest, and `verify_content_hash` is the single enforcement point across native CAS, Bazel REAPI v2 and sccache.
2. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:49-53` — why the route layer misleads: `is_canonical_digest` validates digest SHAPE, not content.
3. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:55-67` — no at-rest coverage exists, so verification is traffic-driven sampling and cold objects are never checked.
4. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:69-84` — decision 1: streaming removes rather than weakens the check, so the scrubber ships first.
5. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:86-105` — decision 2: enumerate R2 via `list_objects_page`, never `blob_meta`; report objects examined.
6. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:107-130` — decision 3: the pre-body concurrency permit is the existing, integrity-free answer to read-path heap, and the single CAS GET is the one surface without it.
7. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:148-165` — alternatives rejected, including verify-while-streaming (the bytes have already reached the client) and driving the scrubber from `blob_meta`.
