# CoreLink Glossary — 50 terms you'll hear in Week 1

> Print this. Tape it to your monitor.
> Order: roughly *most-frequent in standup* first.
> If a term you heard isn't here, open a PR.

## Product / domain

1. **CoreLink** — the product. Multi-tenant content-addressable build cache.
2. **REAPI** — Remote Execution API (v2, Bazel-defined). The gRPC surface we implement.
3. **CAS** — Content-Addressable Storage. Blobs keyed by their digest.
4. **AC** — Action Cache. Action-result records keyed by a canonical action hash.
5. **BYOK** — Bring Your Own Key. Customer-managed encryption keys (KMS).
6. **DSR** — Data Subject Request (GDPR Art. 15–22). Access / export / erasure.
7. **DPA** — Data Processing Addendum. The legal contract for processor-controller relationships.
8. **GA** — General Availability. Target: 2026-08-31 (Full).
9. **PRR** — Production Readiness Review. Gating doc per sprint.
10. **Lighthouse customer** — a pre-GA design partner that attested to the product before launch.

## Crypto / keys

11. **BLAKE3** — primary CAS digest algorithm (faster than SHA-256, tree-friendly).
12. **SHA-256** — fallback CAS digest, mandatory for REAPI compatibility.
13. **KEK** — Key Encryption Key. The customer-held key (in their KMS).
14. **DEK** — Data Encryption Key. Generated per-blob, wrapped by the KEK.
15. **CMK** — Customer Master Key. Same role as KEK in some provider docs.
16. **Envelope encryption** — the KEK-wraps-DEK-wraps-blob pattern.
17. **Kill switch** — customer-initiated KEK disable; in-flight DEKs expire within the cache TTL bound.
18. **Erasure attestation** — Ed25519-signed proof that erasure happened.

## Audit chain

19. **Audit chain** — append-only Merkle-rooted log of state-changing operations.
20. **JCS** — JSON Canonicalization Scheme (RFC 8785). Deterministic leaf serialization.
21. **Merkle tree / RFC 6962** — the chain construction; supports inclusion proofs.
22. **Chain head** — the current root hash, published daily.
23. **Leaf** — a single JCS-canonicalized audit event.
24. **Inclusion proof** — Merkle path proving a leaf is in the tree at a given head.
25. **Audit-fail-CLOSED** — invariant: never return user-visible success until the leaf is durable.

## Tenancy / regions

26. **Tenant prefix** — the per-tenant key namespace; derived in `corelink-tenant-prefix`.
27. **Tenant isolation** — TLA+ invariant: no cross-tenant read or write.
28. **Region** — one of four enumerated values: WNAM, ENAM, WEUR, SAM.
29. **Residency** — structural enforcement that data lives in the region the tenant chose.
30. **CMK rotation** — periodic re-wrap of DEKs under a new KEK version.

## Engineering / process

31. **Sprint contract** — the `_spec_contract.md` per sprint pinning frontmatter + INV-XXX conventions.
32. **WI** — Work Item (e.g. `WI-S15-006`). The unit of sprint deliverable.
33. **INV** — Invariant (e.g. `INV-CAS-014`). A claim model-checked or property-tested.
34. **ADR** — Architecture Decision Record. Lives under `specs/03_architecture/adrs/`.
35. **FM** — Failure Mode (e.g. `FM-SIGNUP-FAILED`). Catalogued in `failure_modes.md`.
36. **RB** — Runbook (e.g. `RB-WEBHOOK-DLQ-REPLAY`). Operational procedure.
37. **SLO** — Service Level Objective. Catalogued in `slo_catalog.md`.
38. **CAP** — Capability (e.g. `CAP-GA-004`). Customer-facing capability identifier.
39. **SEAL** — sprint review verdict; "SEAL APPROVED" means the spec is locked.
40. **/techlead** — the PR review protocol orchestrator runs before merge.

## Charter constraints

41. **Trait-abstraction-defer** — pattern: every crate's public API starts as a trait with an `InMemoryFake`.
42. **`#[non_exhaustive]`** — required on every public enum / optional-field struct.
43. **PROPTEST_CASES** — runtime function (not const) controlling proptest iteration counts.
44. **Charter audit** — periodic CI pass verifying the above constraints.

## Billing

45. **Meter** — a Stripe usage record; we emit `bytes_stored`, `bytes_served`, etc.
46. **Idempotency key** — the deterministic input that makes Stripe webhook retries safe.
47. **Reconciliation drift** — emitted-vs-Stripe-recorded usage delta; > 0.5% is P1.
48. **Tier selector** — request-time logic mapping customer → free / pro / business / enterprise.

## Ops

49. **PagerDuty observer** — Week 1–2 on-call shadow role; receives pages but doesn't ack.
50. **Synthetic page drill** — weekly mock incident to exercise on-call response under timer.

---

If you encountered a term in week 1 that isn't here, the simplest
first PR is adding it.
