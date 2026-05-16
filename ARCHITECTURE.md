# CoreLink — Architecture Overview

> **Audience:** new engineers (Day 0 – Day 30), prospective customers'
> platform/security teams, third-party auditors, advisors.
> **Status:** living document; the canonical sources of truth are the
> Level-3 specs under [`specs/03_architecture/`](./specs/03_architecture/).
> If this overview ever disagrees with those specs, the specs win and
> this file is a bug.
> **Companion diagrams:** [`docs/internal/architecture/diagrams/`](./docs/internal/architecture/diagrams/)
> (Mermaid sources; render with `mmdc` or any Mermaid live editor).

---

## 1. Purpose

**CoreLink is a shared, content-addressable cache for software builds,
package indices, container layers, and ML artifacts** — a Remote
Execution API v2 (REAPI) implementation engineered to the standard
that auditors, security-conscious enterprises, and high-throughput
CI fleets all sign off on simultaneously. Customers point Bazel,
Buck2, Pants, `sccache`, `nix`, or any REAPI-aware client at our
endpoint and immediately get cross-machine, cross-repo, cross-team
chunk-level deduplication; we ship the storage layer, the multi-tenant
isolation, the observability, the audit trail, the billing, and the
compliance posture so customers do not have to.

CoreLink commits to four invariant guarantees that everything else
in this document derives from. They are stated as RFC 2119 **MUST**s
and verified — depending on the invariant — by TLA+ model checking,
property-based tests, runtime assertions in `debug_assert!`, or all
three:

1. **Integrity (`INV-CAS-INTEGRITY`).** No byte CoreLink returns ever
   diverges from the digest the caller asked for. A poisoned cache
   would silently re-flash production binaries; we refuse to serve
   over the slightest doubt.
2. **Tenant isolation (`INV-TenantIsolation`).** Tenant A never reads,
   writes, lists, or otherwise observes the existence of tenant B's
   data — even with a compromised credential. Verified in TLA+.
3. **Confidentiality (`INV-CONF-AT-REST`, `INV-CONF-IN-FLIGHT`).**
   All blobs and metadata are encrypted at rest (AES-256-GCM,
   per-tenant DEKs, optional customer-held KEK in BYOK mode) and in
   transit (TLS 1.3, mTLS between control and data planes).
4. **Audit append-only (`INV-AUDIT-APPEND-ONLY`).** Every write,
   admin action, and key-management event is captured in an
   append-only Merkle-chained log retained ≥ 7 years with hourly
   externally-timestamped anchors.

Everything below — every crate, every SLO, every compliance
artifact — exists to enforce one or more of these four invariants
without sacrificing the speed and dedup ratio that justify using
CoreLink in the first place.

---

## 2. System context

See [`diagrams/system-context.mmd`](./docs/internal/architecture/diagrams/system-context.mmd)
for the visual. In prose:

* **Customer-side actors.** Developer / CI executor talking REAPI v2
  gRPC; customer admins using the [admin UI](./apps/admin-ui/) or
  [`corelink-cli`](./crates/corelink-cli/); auditors pulling signed
  audit-log exports and inclusion proofs; the public
  `status.corelink.humangr.com` page and its SSE/RSS subscribers.
* **Cloudflare edge (`TB-0`/`TB-1`).** TLS 1.3 termination, WAF
  managed rules, per-IP rate limit, DDoS mitigation. From there the
  request enters a Cloudflare-internal service binding — never the
  public Internet — to the control or data plane.
* **Control plane (`apps/server` + `corelink-worker`).** Workers
  handle auth, quota check, rate limiting, billing emission, admin
  API, DSR intake, and routing. Stateless; horizontally scaled by
  Cloudflare automatically.
* **Data plane (Cloudflare Container running the Rust REAPI gRPC
  server).** Heavy lifting: chunking, hashing, dedup, byte-stream
  upload/download, manifest assembly, action-cache reads. Co-located
  with R2 in-region for minimum latency.
* **Storage bindings.** R2 for blobs / action-cache entries /
  append-only audit logs (Object Lock for legal hold); D1 for hot
  metadata, quotas, multipart state, AC-meta; Neon Postgres for
  accounts, tenants, subscriptions, billing aggregates; KV for the
  read-mostly verify cache and feature flags; Durable Objects for
  rate buckets, config singletons, replica fan-out, and the rollout
  controller.
* **External sub-processors.** Clerk for primary identity, Stripe
  for metered billing, customer-controlled KMS (AWS KMS, GCP KMS,
  Azure Key Vault, HashiCorp Vault) for BYOK envelope encryption,
  PagerDuty for SEV-1 escalation, Grafana Cloud for metrics/logs/
  traces. Every sub-processor is enumerated in the public
  sub-processor list with its purpose, region, and legal basis;
  changes generate a 30-day customer notice (`corelink-privacy-sub-processor-emit`).

The boundary between the control plane (Worker) and the data plane
(Container) is deliberate. The Worker is stateless and cheap to fan
out — perfect for auth, rate-limit, billing-emission, and admin
operations that involve many small RPC hops. The Container holds the
hot loop (chunk → hash → R2 PUT/GET → verify) and stays in-region
with R2 so the bandwidth-heavy phase never traverses the public
Internet. Both planes communicate only over Cloudflare service
bindings — no public endpoint exists for the Container — so the
attack surface is the WAF, not the gRPC server.

---

## 3. Core building blocks

The Cargo workspace ships **107 crates**. The twelve below carry the
load that matters for understanding the system; the rest are adapters,
schema crates, and fuzz harnesses orbiting them.

| # | Crate | One-liner |
|---|---|---|
| 1 | [`corelink-reapi`](./crates/corelink-reapi/) | REAPI v2 server: ByteStream, ContentAddressableStorage, ActionCache, Capabilities; the wire surface. |
| 2 | [`corelink-hash`](./crates/corelink-hash/) | BLAKE3-primary, SHA-256-fallback content hashing; the integrity invariant lives here. |
| 3 | [`corelink-chunker`](./crates/corelink-chunker/) | FastCDC rolling-hash chunker (2 MiB target) for cross-file dedup. |
| 4 | [`tenant-path`](./crates/tenant-path/) | HMAC-derived per-tenant R2 prefix; sole owner of `INV-TenantIsolation`. |
| 5 | [`corelink-ac`](./crates/corelink-ac/) | Action-cache read/write semantics + AC-meta freshness rules. |
| 6 | [`corelink-audit-chain`](./crates/corelink-audit-chain/) | Append-only Merkle log, CloudEvents 1.0 leaves, Ed25519 anchoring. |
| 7 | [`corelink-byok`](./crates/corelink-byok/) | Envelope-encryption core; pluggable adapters for AWS/GCP/Azure/Vault. |
| 8 | [`corelink-quota-fsm`](./crates/corelink-quota-fsm/) | Per-tenant, per-tier quota state machine (reserve → commit → release). |
| 9 | [`corelink-billing-aggregator`](./crates/corelink-billing-aggregator/) | Usage-event fan-in, dedup, idempotent Stripe meter emission. |
| 10 | [`corelink-failover-router`](./crates/corelink-failover-router/) | Active/passive region cutover with TLA+-checked invariants. |
| 11 | [`corelink-dsr`](./crates/corelink-dsr/) | LGPD/GDPR data-subject-request orchestration + signed attestation. |
| 12 | [`corelink-slo`](./crates/corelink-slo/) | SLI emission, multi-burn-rate alerting, error-budget arithmetic. |

The next ring out — `corelink-gc`, `corelink-dedup`,
`corelink-eviction`, `corelink-r2-multipart`, `corelink-manifest`,
`corelink-ratelimit`, `corelink-abuse`, `corelink-canary`,
`corelink-supply-verify`, `corelink-deploy-verifier`,
`corelink-rollout-controller` — composes around these twelve to
deliver the production behavior described in §5–§7.

Several crates exist purely as **separation of concerns**:
`corelink-ac-schema`, `corelink-auth-schema`, `corelink-multipart-schema`
hold the canonical wire types so the producer and consumer crates
cannot drift; `corelink-cf-bindings` is the only place that touches
the Cloudflare runtime so the rest of the workspace stays portable
and unit-testable; `corelink-openapi` regenerates the public OpenAPI
spec from in-crate types so the docs site and the customer SDK
generators never lag the server. Every crate exposes a narrow public
surface and most have a `fuzz/` companion harness (39 of them at the
time of writing) that runs in CI on every PR.

---

## 4. Tenant model

Multi-tenancy is the single biggest source of catastrophic outcomes
in a shared cache. Get isolation wrong once and an attacker writes a
malicious binary that the next tenant's CI happily flashes to
production. So we treat tenancy as the **first-class boundary**:

### 4.1 HMAC prefix derivation

Every R2 key is shaped `tenant/<prefix>/<kind>/<digest>`, where
`<prefix>` is derived as:

```
prefix = base32( HKDF-Expand( HKDF-Extract( salt = TENANT_KEY,
                                            ikm  = tenant_id ),
                              info = region || op )[0..20] )
```

`TENANT_KEY` (`AST-TENANT-KEY` in the security model) lives in
Cloudflare Secrets, is rotated annually, and is **never accessible
from a Worker** — only the data plane's hashing path reads it via a
binding. Without `TENANT_KEY`, forging another tenant's prefix is
computationally infeasible (HMAC over 256 bits).

### 4.2 Two-layer enforcement

Isolation is enforced in two independent layers so a logic bug in
either still produces zero cross-tenant blast:

1. **Worker authZ.** The PAT's scope (`tenant_id`) must equal the
   `tenant_id` claimed in the request, checked constant-time against
   the Argon2id-hashed token in KV.
2. **Storage binding.** The R2 binding policy only permits reads/writes
   under a prefix derived from the request's verified `tenant_id`.

Layer 1 is fast; layer 2 is the safety net. Both must agree for the
request to land bytes in R2.

### 4.3 TLA+-checked invariant

`INV-TenantIsolation` is modeled in TLA+ (`specs/03_architecture/tla+/`)
and the model checker runs in CI on every change to `tenant-path`,
`corelink-worker`, or `corelink-reapi`. Counterexamples block merge.

### 4.4 Dual-approval for boundary mutations

Any admin action that could touch tenant boundaries — key rotation,
prefix migration, BYOK rewrap, cross-tenant audit export — requires
**dual approval** (two human operators, both with WebAuthn-bound MFA)
via `corelink-admin-api`. The approval record is itself a leaf in the
audit chain. Single-operator changes to those code paths are simply
not exposed; the API requires both signatures or it returns `403`.

See [`diagrams/tenant-isolation.mmd`](./docs/internal/architecture/diagrams/tenant-isolation.mmd).

---

## 5. Data lifecycle

A blob's life in CoreLink has five stages.

### 5.1 Ingestion

Client opens a ByteStream upload. Worker authenticates the PAT,
checks the quota state machine (`corelink-quota-fsm`), reserves the
estimated byte budget, and forwards a signed upload intent to the
Container. See [`diagrams/data-flow-write.mmd`](./docs/internal/architecture/diagrams/data-flow-write.mmd).

### 5.2 Content-addressing (CAS write)

The Container streams bytes through the FastCDC chunker, hashes each
chunk with BLAKE3, deduplicates against `blob_meta` in D1 (refcount
increment if hit, fresh PUT to R2 if miss), and writes under the
HMAC-derived tenant prefix. The DEK that protects the bytes at rest
is wrapped by either the CoreLink platform KEK or — in BYOK mode —
the customer's KEK in their own KMS. See §6 and
[`diagrams/byok-envelope.mmd`](./docs/internal/architecture/diagrams/byok-envelope.mmd).

### 5.3 Action-cache association

On `UpdateActionResult`, the action digest → outputs binding is
written to `R2 ac-<region>`, indexed in D1, and TTL'd per tier
(`ttl_for_tier`) per [SLO catalog §3.1](./specs/03_architecture/slo_catalog.md).
Action-cache reads return 404 in a **content-uniform** way
(ADR-0028) so a probing attacker cannot use response timing to learn
whether a digest exists.

### 5.4 Audit-chain emission

Every write — and every admin action, key rotation, DSR transition,
quota override, and BYOK rewrap — emits a CloudEvents 1.0 record
(ADR-0033) into `corelink-audit-chain`. Records are canonicalized,
hashed (BLAKE3), and appended as Merkle leaves. Roots are signed
Ed25519 and externally timestamped (RFC 3161) every hour.
See [`diagrams/audit-chain-merkle.mmd`](./docs/internal/architecture/diagrams/audit-chain-merkle.mmd).

### 5.5 Retention, GC, erasure

Blobs are reachable as long as some action-cache entry references
them OR they were written in the GC reachability window
(ADR-0012). `corelink-gc` runs out-of-band with a conservative
"reachability + grace" policy. Audit logs are immutable for ≥ 7
years (SOC 2). DSR erasure (`corelink-privacy-erasure-worker`) follows
LGPD Art. 18 / GDPR Art. 17 timelines, produces a signed attestation
(`corelink-erasure-attestation`), and pseudonymizes — never erases —
the audit-log subject identifier so the chain stays unbroken.
See [`diagrams/data-flow-dsr.mmd`](./docs/internal/architecture/diagrams/data-flow-dsr.mmd).

The read path mirrors the write path but with an extra step: every
bytes-returned response is re-verified against the requested digest
before leaving the Container. A mismatch refuses the read (`503`)
and emits a `P0` integrity event. See
[`diagrams/data-flow-read.mmd`](./docs/internal/architecture/diagrams/data-flow-read.mmd).

A subtle point worth highlighting: the same digest can be referenced
by many tenants because content-addressing is, by definition,
tenant-agnostic at the byte level. We resolve this with **per-tenant
reference counts** in `blob_meta`: a single physical R2 object is
deduplicated globally, but its existence in any tenant's namespace
requires an explicit reference. GC only deletes a blob when its
global refcount hits zero AND it has been unreferenced for longer
than the grace window (ADR-0012). This means dedup is silent (no
tenant can detect another tenant's writes via timing or existence
probes — ADR-0028 enforces uniform 404s) while still letting us bill
each tenant for the logical bytes they reference.

---

## 6. Trust and security model

### 6.1 Envelope encryption (BYOK)

CoreLink encrypts every blob with a per-tenant Data Encryption Key
(DEK, AES-256). The DEK is wrapped by a Key Encryption Key (KEK)
that — for Enterprise BYOK customers — lives non-exportable in the
customer's KMS. The CoreLink data plane unwraps DEKs on demand via
the customer KMS, holds the unwrapped DEK in an in-memory cache with
a 5-minute TTL, and never persists it. A KEK revocation drains
in-flight reads and purges the cache; subsequent reads must rewrap.
The AAD on every AEAD operation binds `tenant_id || digest ||
key_version || region`, so a ciphertext stolen from one slot cannot
be replayed into another.

See [`diagrams/byok-envelope.mmd`](./docs/internal/architecture/diagrams/byok-envelope.mmd)
and [`specs/03_architecture/key_management.md`](./specs/03_architecture/key_management.md).

### 6.2 Merkle-chained audit

The audit log is the substrate every compliance and forensic claim
ultimately rests on. Each event is canonicalized, hashed, and added
to an append-only Merkle accumulator per region. Hourly roots are
Ed25519-signed by a hardware-backed key and externally timestamped
(RFC 3161) so even compromise of the signing service cannot
retroactively rewrite history. Auditors and customers can request
**inclusion proofs**: given an event, the chain returns the sibling
hashes needed to recompute the signed root locally.

### 6.3 TLA+-verified invariants

The TLA+ specs under
[`specs/03_architecture/tla+/`](./specs/03_architecture/tla+/) model
the protocol-critical invariants — `INV-TenantIsolation`,
`INV-CAS-INTEGRITY`, `INV-Replication`, `INV-DualApproval`,
`INV-QuotaMonotonic`. Model checkers run in CI for every PR touching
the relevant crates; counterexamples block merge. Other invariants
are checked by property-based tests (`proptest`), constant-time
benchmarks (`corelink-client-verify`), and runtime assertions in
`debug_assert!` builds.

### 6.4 STRIDE + LINDDUN

Trust boundaries (`TB-0` Internet → CF edge; `TB-1` CF edge → CoreLink
planes; `TB-2` planes → storage; `TB-3` tenant ↔ tenant; `TB-4` admin
operator → control plane) each have a STRIDE row in
[`security_model.md §5`](./specs/03_architecture/security_model.md) and a
LINDDUN privacy-threat row in
[`privacy_model.md §4`](./specs/03_architecture/privacy_model.md). Every
mitigation is a control with a stable ID (`CTRL-*` for security,
`CTRL-PRIV-*` for privacy) cross-referenced from the compliance
matrix.

### 6.5 Supply chain (SLSA L3)

The CoreLink build pipeline is engineered to SLSA Level 3: hermetic
builds in disposable runners, signed CycloneDX SBOMs per artifact
(ADR-0014), provenance attestations with non-falsifiable build
metadata, two-person review on every PR that touches a release
script, and `corelink-supply-verify` blocking deploys whose SBOM
diff includes an unreviewed dependency. CLI releases are signed —
GPG for Linux, Apple-notarized for macOS, Authenticode for Windows
— and the verification recipe is published at the canonical
`.well-known/gpg-pubkey.asc` URL.

### 6.6 Constant-time and side-channel hygiene

PAT verification, BYOK key comparison, and audit-leaf signature
checks all use constant-time primitives. ADR-0023 documents the
specific timing-padding policy. `corelink-client-verify` ships a
property-based test suite that flags any timing distribution drift
across success/failure paths on every commit.

---

## 7. SLO catalog summary

[Full catalog](./specs/03_architecture/slo_catalog.md) has 24 SLOs across
five tiers (`free`, `solo`, `team`, `business`, `enterprise`). The
ten the on-call rotation actually wakes up for, in priority order:

1. **`SLO-CORRECT-CAS`** — 100% of CAS GETs verify client-side; any
   miss is a stop-the-line `P0`. Correctness has no budget.
2. **`SLO-CORRECT-ISO`** — 100% tenant isolation. Single confirmed
   cross-tenant read is a public incident.
3. **`SLO-AVAIL-CAS-GET`** — 99.9% (team) / 99.95% (enterprise) for
   the CAS read hot path; the product's reason to exist.
4. **`SLO-AVAIL-CP`** — 99.9% control-plane availability; auth and
   admin endpoints.
5. **`SLO-AVAIL-CAS-PUT`** — 99.9% write availability; degraded
   writes are tolerable, lost writes are not.
6. **`SLO-LAT-CAS-GET`** — p99 ≤ 300 ms (team) / 200 ms (enterprise)
   per tier interpolation in §3.1.
7. **`SLO-LAT-AC-HIT`** — p99 ≤ 50 ms; action-cache lookup latency
   is what CI users feel.
8. **`SLO-FRESH-BILLING`** — usage events land in Stripe within 15
   minutes; revenue integrity SLO.
9. **`SLO-FRESH-DSR-ERASURE`** — DSR ticket closes within
   LGPD/GDPR statutory window (15 d BR / 30 d EU).
10. **`SLO-RTO-REGION-FAILOVER`** — region cutover completes in
    ≤ 15 min p99, measured by monthly DR drill
    (see [`diagrams/region-failover.mmd`](./docs/internal/architecture/diagrams/region-failover.mmd)).

Each SLO has a multi-burn-rate alert wired through `corelink-slo`,
each alert has a runbook in
[`specs/03_architecture/runbooks/`](./specs/03_architecture/runbooks/),
and the error-budget policy (freeze deploys, escalate, etc.) is
defined in [`slo_catalog.md §5`](./specs/03_architecture/slo_catalog.md).

---

## 8. Failure-mode taxonomy

Failure modes are catalogued in
[`failure_modes.md`](./specs/03_architecture/failure_modes.md) using a
software-adapted FMEA (severity × occurrence × detectability,
RPN ≥ 60 → P0). IDs are stable and shaped `FM-NNN`. Classes
(per §2 of the spec):

* `compute` — Worker / Container / DO faults.
* `storage` — R2 / D1 / Neon / KV / DO storage.
* `network` — CF edge, inter-region, DNS.
* `dependency` — crate or sub-processor failure (Stripe, Neon, Clerk).
* `operational` — human ops, deploy regression, config change.
* `adversarial` — abuse, DoS, supply chain, credential leak.
* `data-integrity` — silent corruption, bit rot, protocol bug.
* `clock-state` — skew, leap second, NTP drift.
* `emergent` — feedback loop, cascading overload, retry storm.

Each `FM-*` cross-links to its mitigating resilience pattern in
[`resilience_patterns.md`](./specs/03_architecture/resilience_patterns.md),
its runbook stub, and the SLO it would burn. When a new failure mode
is observed in production, it is added with a `WI` and assigned an
`FM-` ID before the post-mortem is signed off.

Two operational rules shape how the catalog stays honest: occurrence
is scored under the assumption that mitigations are absent (per the
SAE J1739 FMEA original; ADR-noted reasoning is that mitigations
fail, and the un-mitigated rate is the one that matters when they
do), and any `S = 5` row is automatically promoted to at least `P1`
class regardless of RPN — severity 5 implies cross-tenant blast
radius or data loss, and those impacts mandate a runbook and an
automated test even when occurrence is rare. The chaos schedule in
`corelink-chaos-scheduler` cycles through P0/P1 failure modes on a
monthly cadence so detectability stays measured, not assumed.

---

## 9. Compliance posture

* **SOC 2.** Built to Type II from day zero: every CTRL has an evidence
  pipeline (EVT-001..EVT-040 in
  [`compliance_matrix.md §2`](./specs/03_architecture/compliance_matrix.md)).
  Type I audit targeted six months post-GA; Type II twelve to eighteen
  months after Type I (six-month observation window). Drata pulls
  evidence continuously via `corelink-drata-sync`.
* **LGPD (Brasil).** Compliance-by-design from GA day one for the
  `sam` region. Data residency is honest (blobs stay in-region;
  metadata may cross to Neon US under SCC). DSR self-service via
  `corelink-dsr` meets the 15-day Art. 18 ANPD timeline.
* **GDPR (EU).** Compliance-by-design from GA day one for the `weur`
  region. SCCs in place with every sub-processor; sub-processor list
  is published and changes trigger a 30-day notice via
  `corelink-privacy-sub-processor-emit`. Art. 17 erasure completes
  within 30 days with signed attestation.
* **ISO/IEC 27001:2022.** Targeting certification 18 months post-Type II.
  ISMS scope = CoreLink CAS/AC and supporting control plane. Annex A
  controls already mapped 1:1 to the internal `CTRL-*` catalog in
  [`compliance_matrix.md §3`](./specs/03_architecture/compliance_matrix.md).
* **PCI DSS v4.0 SAQ-A.** Self-attested as of 2026-05-15 with annual
  recertification. CoreLink is a card-not-present merchant; all CHD
  is outsourced to Stripe. Boundary diagram and recertify runbook live
  under [`specs/_compliance/`](./specs/_compliance/).

HIPAA and FedRAMP are conditional on customer demand (BAA + audit
pathway documented in
[`compliance_matrix.md §6`](./specs/03_architecture/compliance_matrix.md)).

---

## 10. Where to read next

| You are… | Start here |
|---|---|
| New engineer (Day 0) | [`docs/internal/ENGINEERING-ONBOARDING.md`](./docs/internal/ENGINEERING-ONBOARDING.md), then this file, then the spec that owns your first WI's domain. |
| Backend dev going deep | [`specs/03_architecture/data_model.md`](./specs/03_architecture/data_model.md), [`storage_semantics_matrix.md`](./specs/03_architecture/storage_semantics_matrix.md), the relevant crate's `lib.rs`. |
| Security reviewer | [`security_model.md`](./specs/03_architecture/security_model.md), [`key_management.md`](./specs/03_architecture/key_management.md), [`specs/03_architecture/tla+/`](./specs/03_architecture/tla+/). |
| Privacy / DPO | [`privacy_model.md`](./specs/03_architecture/privacy_model.md), [`compliance_matrix.md §4–§5`](./specs/03_architecture/compliance_matrix.md). |
| Auditor | [`compliance_matrix.md`](./specs/03_architecture/compliance_matrix.md), [`specs/_compliance/`](./specs/_compliance/), audit-chain inclusion-proof API in [`corelink-audit-chain`](./crates/corelink-audit-chain/). |
| SRE / on-call | [`slo_catalog.md`](./specs/03_architecture/slo_catalog.md), [`failure_modes.md`](./specs/03_architecture/failure_modes.md), [`resilience_patterns.md`](./specs/03_architecture/resilience_patterns.md), [`specs/03_architecture/runbooks/`](./specs/03_architecture/runbooks/). |
| Customer platform team | [`apps/docs/`](./apps/docs/) (public docs) and the diagram set under [`docs/internal/architecture/diagrams/`](./docs/internal/architecture/diagrams/). |
| Tech lead reviewer | [`docs/internal/TECHLEAD-CHECKLIST.md`](./docs/internal/TECHLEAD-CHECKLIST.md). |

For the full diagram index and render instructions see
[`docs/internal/architecture/README.md`](./docs/internal/architecture/README.md).
