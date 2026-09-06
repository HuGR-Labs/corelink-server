---
type: "ADR"
title: "ADR-S34-001 — CAS read integrity: the scrubber precedes streaming"
description: "Streaming CAS reads would remove the read-path digest re-verification, which is the only integrity coverage that exists, so the at-rest scrubber is a hard prerequisite; the read-path memory bound is a separate concern with a cheaper answer."
source_files:
  - "specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md"
source_blobs:
  - "specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md@17bf02d82ee547024214cb49ae600dee87c97500"
checkpoint_sha: "fc7ec9bb9c5d8711cabc4b93c989062e71d2955f"
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

# Addenda — BYOK, and the two coverage traps

Decision 2 ("enumerate R2 and re-hash what you find") holds only for plaintext-plan tenants. For a BYOK-`active` tenant the stored object is CIPHERTEXT and the read path decrypts BEFORE the content-hash re-verify, so a scrubber re-hashing raw bytes would emit a stream of false `CorrectnessViolation`s against intact data; the production KMS boundary is wired but owner runtime provisioning remains separate evidence, which makes this a load-bearing integration seam (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:175-202`).

Decision 4 first said the scrubber "resolves each object's BYOK plan". That was corrected: `resolve_byok` is private to `impl R2CasHandler` / `impl R2AcHandler` and is not a method on `R2S3Client` — the type a sweep actually holds — and it maps a LOGICAL digest to a physical one, the opposite of the direction a sweep travels. The revised decision asks the question ONCE PER TENANT through the public `ByokConfigCache::get` + `engagement_for` pair: `Plaintext` scrubs, `Encrypt(_)` skips the whole tenant into `skipped_encrypted`, and `FailClosed(_)` or a config-read error counts `failed` — never a silent skip, never an assumed plaintext (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:225-259`).

Enumeration must be per tenant, because an R2 key is `<region>/<tenant_prefix>/<digest>` with `tenant_prefix` a secret-keyed HMAC that cannot be inverted. That makes the tenant list load-bearing: measured against `corelink-config-prod` on 2026-08-26, `tenant` holds 262 rows against `tenant_storage_state`'s 74 and `blob_meta`'s 0, so driving the sweep off `tenant_storage_state` omits 188 of 262 tenants and still reports a clean run — the `blob_meta` silent-success shape, one table over (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:260-278`). Finally, `verify_content_hash` is to be widened to `pub(crate)` and CALLED rather than reimplemented, because a parallel hash check in the scrubber would falsify the single-enforcement-point claim this ADR rests on (`specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:279-285`).

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
8. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:175-202` — addendum 1: a BYOK-active tenant's object is ciphertext, so the re-verify must run on plaintext; the production KMS boundary preserves this invariant when owner runtime provisioning is enabled.
9. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:225-259` — addendum 2: decision 4 named an unreachable private seam; revised to one per-tenant `ByokConfigCache::get` + `engagement_for` classification with a three-way counter mapping.
10. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:260-278` — the tenant-list trap: `tenant` 262 rows vs `tenant_storage_state` 74 vs `blob_meta` 0, measured in prod.
11. `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md:279-285` — `verify_content_hash` widened to `pub(crate)` and called, never reimplemented.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
