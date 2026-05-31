---
id: "OSS-STRATEGY"
type: "strategy_decision"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-30"
owner: "Gustavo Schneiter"
related: ["docs/POSITIONING.md", "docs/internal/OSS-VS-CLOSED-MATRIX.md"]
---

# CoreLink — OSS Strategy Decision

> **Pre-launch decision doc.** Written so the founder never has to
> panic-decide mid-launch which crates ship open. Every table here is a
> decision, not a proposal. Change it deliberately, not reactively.

Strategic framing lives in [POSITIONING.md](./POSITIONING.md). Read
that first if you need context on why SMB self-serve + R2 zero-egress
is the chosen play.

---

## TL;DR

Five bullets. Commit these to memory before any public launch action.

1. **Client-side primitives are open.** Hash verifier, PAT format, audit
   proof verifier, WASM bundle, rate-limit headers, OpenAPI contract,
   CLI, FFI wrappers — all MIT OR Apache-2.0.
2. **The server is closed.** Everything that runs inside Cloudflare
   Workers / Durable Objects / Containers stays proprietary. No
   self-hosting path is ever offered or implied.
3. **Bridges are hybrid.** Bazel and Turborepo bridge traits (protocol
   shape) are open; the production wiring (request routing, auth
   injection, R2 materializer) stays closed.
4. **Opening primitives drives distribution.** A developer who audits
   the BLAKE3 verifier, trust its PAT signature, and ports the audit
   proof to their own tooling is a developer who trusts CoreLink. Trust
   converts to paying customers.
5. **Post-GA, codebase splits into two repos.** `humangr-labs/corelink`
   (public, OSS crates) and `humangr-labs/corelink-server` (private,
   server). Until then: one monorepo, per-crate license tags.

---

## The Strategic Frame

CoreLink's go-to-market is **developer community + content + OSS, not
enterprise sales calls** (see [POSITIONING.md §Distribution
risks](./POSITIONING.md)). That means the product must earn GitHub
stars, HN upvotes, and `cargo add` adoption before it earns
subscriptions.

Open-sourcing the right primitives solves three problems at once:

| Problem | OSS lever |
|---|---|
| Invisible to SMB dev who hasn't heard of CoreLink | Open CLI + BLAKE3 verifier → GitHub discoverability |
| Dev won't adopt a black-box PAT format | Open PAT verifier → PAT becomes a community standard, not a vendor lock-in |
| Enterprise buyer suspects audit-chain is marketing | Open audit proof verifier → "bring your own BLAKE3" reproducibility kills the objection |
| Competing with larger incumbents who have brand | OSS contributions signal maturity + good faith |

The rule: **open what a developer must trust; close what a competitor
could host.** A competititor who extracts the BLAKE3 verifier gains
nothing — they cannot run the service. A competitor who extracts the
replication coordinator + BYOK orchestrator + billing pipeline can
stand up a competing managed service for free.

---

## What Is Open (and Why)

All open crates are dual-licensed **MIT OR Apache-2.0** and publishable
to crates.io. See also the per-crate `license` field in each
`Cargo.toml`.

| Crate / Component | Why Open |
|---|---|
| `corelink-hash` | BLAKE3 wrapper + `VerifiedBody`. Developers who integrate CoreLink verify blobs client-side; auditors re-hash exports offline. Inspectability is the trust foundation. |
| `corelink-client-verify` | Audit-chain proof verifier. Let anyone reproduce chain-root from raw events without CoreLink infrastructure. Kills "trust us" objections from enterprise buyers. |
| `corelink-pat` (format + Argon2id verify) | PAT token format and signature verification. Auditable security primitive. Customers inspect exactly how their tokens are issued and validated. |
| `corelink-audit-chain` (schema/traits only) | The Merkle leaf schema and `AuditEntry` type. Closed: server-side append impl and retention engine. Open: the shape that lets external tools parse exports. |
| `corelink-wasm` | WASM bundle for browser-side audit-chain verification. Enables third-party dashboards, compliance tooling, and browser-side proofs. |
| `corelink-rate-headers` | 429 response shape + parser. Developers building SDKs in other languages need this to handle backpressure correctly. |
| `tenant-path` | Pure HMAC prefix primitive. Its security invariant (`INV-TenantIsolation`) is only credible if customers can reproduce it. No secrets, no server state. |
| `corelink-reapi` (wire types) | REAPI v2 wire surface (types and shapes, not handlers). Bazel/Turbo integrators need the protocol definition to write their own clients. |
| Client SDK (TypeScript/Go/Python) | Self-serve onboarding requires an SDK that works out of the box. Open SDK = community contributions to language support + lowers onboarding friction below 5 min. |
| `bazel-remote-compat` shim (planned) | Migration on-ramp for users coming from the open-source `bazel-remote` project. Lowers switching cost, drives adoption without a sales call. |
| OpenAPI contract | The published API contract. External tooling (Postman, SDK generators, API explorers) depends on it. Closing it would be counterproductive. |

**Distribution logic**: each open crate solves a specific trust or
discoverability gap. None of them let a competitor replicate the
service. The `corelink-hash` repo README is a search-engine entry
point for "BLAKE3 Rust artifact verification."

---

## What Is Proprietary (and Why)

Proprietary means: source stays in `humangr-labs/corelink-server`
(private). No `crates.io` publish. No `git clone` access for
customers.

| Category | Crates | Why Closed |
|---|---|---|
| **Multi-region HA** | `corelink-replication`, `corelink-replication-coordinator`, `corelink-failover-router`, `corelink-region`, `corelink-replica-worker` | The HA story is a differentiator. Competitors can't replicate the R2 zero-egress + DO coordinator pattern without the wiring. Opening it gives a well-funded competitor the blueprint for free. |
| **BYOK orchestrator** | `corelink-byok`, `corelink-rotation-adapters`, `corelink-adapters-vault`, `corelink-adapters-cloud` | Key rotation machinery across four KMS providers is months of hardening. Opening it invites CVEs to be discovered in production by bad actors before the vendor. |
| **Billing pipeline** | `corelink-billing`, `corelink-billing-aggregator`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-billing-stripe-materializer`, `corelink-billing-emit`, `corelink-billing-stripe-traits`, `corelink-tier-selection` | Pricing logic + Stripe webhook orchestration is operational IP. Exposing aggregation heuristics enables abuse. |
| **Privacy / compliance workflows** | `corelink-dpa-acceptance`, `corelink-dsr`, `corelink-privacy`, `corelink-privacy-erasure-worker`, `corelink-privacy-pseudonymize`, `corelink-erasure-attestation`, `corelink-dual-approval` | DPA + DSR + erasure attestation workflows encode legal interpretations and process decisions. These are competitive differentiators in regulated verticals. |
| **Auth / session** | `corelink-auth`, `corelink-clerk`, `corelink-clerk-cf`, `corelink-signup` | Server-side session validation and signup flow. Opening rate-limit bypass paths or session forgery surfaces is a security liability. |
| **Deployment code** | `corelink-worker`, `corelink-container`, `corelink-cf-bindings`, `corelink-config-do` | Cloudflare Worker + Container + DO wiring is infrastructure-specific. Opening it provides a self-hosting blueprint that contradicts the SaaS positioning. |
| **Admin UI** | `apps/admin-ui` + `corelink-handler-admin` | Internal ops surface. Not a customer-facing artifact. Opening it leaks internal playbook. |
| **Ops / observability** | `corelink-slo`, `corelink-telemetry`, `corelink-tracing`, `corelink-chaos-scheduler`, `corelink-runbook-tracker`, `corelink-analytics` | Ops heuristics + chaos schedules + SLO budgets are operational configuration, not library code. |
| **Server-side handlers** | `corelink-handler-cas`, `corelink-handler-ac`, `corelink-handler-customer`, `corelink-ac`, `corelink-gc`, `corelink-eviction` | Request dispatch, GC eviction logic, and AC namespace management are server business logic. |
| **Integrations** | `corelink-slack-real`, `corelink-stripe-real`, `corelink-statuspage-real`, `corelink-dt-webhook`, `corelink-terraform-drift-consumer` | Real-provider glue code contains configuration patterns that would expose operational topology. |
| **PAT (server issuance)** | Issuance half of `corelink-pat` stays closed; verify half is open | Issuance logic + rate limits is where abuse prevention lives. |

**The line**: any crate that, if extracted, would let someone stand up
a competing service or materially reduce CoreLink's operational
advantage stays closed.

---

## What Is Hybrid

| Component | Open part | Closed part |
|---|---|---|
| **Bazel bridge** (`corelink-bazel-bridge`) | REAPI v2 wire traits, request/response shapes | Production routing, auth injection, quota enforcement, R2 materializer |
| **Turborepo bridge** (`corelink-turbo-bridge`) | Turbo v8 protocol trait definitions | Production wiring, cache hit/miss telemetry, tenant scope injection |
| **Audit chain** (`corelink-audit-chain`) | `AuditEntry` schema, leaf hash format, proof verifier | Server-side Merkle append, root signing, hourly RFC 3161 timestamps, retention engine |
| **Billing traits** (`corelink-billing-stripe-traits`) | Stripe event shape types (already isolated in `-traits` crate) | Aggregation heuristics, reconcile engine, materializer |

The traits split is already reflected in the crate decomposition (Wave
33-36 reorg). The `-traits` crates carry `license = "MIT OR Apache-2.0"`;
the `-materializer` / `-coordinator` counterparts stay closed.

---

## Repository Structure

### Today (pre-GA)

One monorepo: `humangr-labs/corelink-server` (private). All crates,
open and closed, live together. OSS-flagged crates carry
`license = "MIT OR Apache-2.0"` in `Cargo.toml` and are independently
publishable to crates.io with `cargo publish -p <crate>`.

**Advantage**: no split-brain, no cross-repo dependency management
during early iteration.

### Post-GA (R-8 launch window, T-7d)

| Repo | Visibility | Contents |
|---|---|---|
| `humangr-labs/corelink` | **PUBLIC** | OSS crates + client SDK + CLI + OpenAPI + customer-facing docs + contribution guide |
| `humangr-labs/corelink-server` | **PRIVATE** | Closed server crates + deployment scripts + compliance docs + internal runbooks |

The public repo will vendor-copy or path-dep the few open crates that
the server also depends on. No circular dependency. Migration tracked as
`WI-OSS-SPLIT-2026` (see `docs/internal/OSS-VS-CLOSED-MATRIX.md`).

**Do not split early.** Managing two repos before GA full adds merge
overhead with zero user-facing benefit.

---

## Roadmap

| Phase | When | What ships |
|---|---|---|
| **Pre-launch** (now) | Before first public HN/Twitter post | `corelink-hash`, `corelink-client-verify`, `corelink-rate-headers`, `tenant-path` published to crates.io. README links point to live crates. |
| **At-launch** | Same day as public announcement | `corelink-pat` (verify half), `corelink-wasm`, OpenAPI YAML published to `humangr-labs/corelink` public repo. CLI `brew` tap + binaries. |
| **Post-launch T+30d** | After first lighthouse customers | TypeScript SDK open-sourced (extracted from internal monorepo). `bazel-remote-compat` shim released. Bazel/Turbo bridge traits open. |
| **GA Full (R-8)** | Per ROADMAP-TO-GA.md wave schedule | Monorepo split into public + private repos. Go + Python FFI wrappers open. Full CONTRIBUTING.md + `good-first-issue` backlog live. |

---

## Contributor Guide

Full contributor guide: [`CONTRIBUTING.md`](../CONTRIBUTING.md).

Key requirements:
- DCO sign-off required on all commits (`git commit -s`).
- Two-person review on release scripts.
- Conventional commits enforced via CI.
- `good first issue` label on GitHub for first-PR scope (half-day target).

---

## Anti-Patterns to Avoid

| Anti-pattern | Why it's fatal |
|---|---|
| **"Open the engine, sell support"** | CoreLink's moat is operational (R2 zero-egress + CF edge). Opening the server lets AWS/GCP host it cheaper than CoreLink can. The moment that happens, pricing advantage inverts. |
| **"Open-core trap": fake open, real lock-in** | Licensing the client SDK under AGPL or BSL to prevent forks destroys developer trust. MIT/Apache-2.0 is the only viable license for community adoption. Don't play games. |
| **Announcing OSS before crates.io publish** | Saying "we're open source!" and pointing to a private repo, or to a public repo with zero commits, triggers immediate HN scepticism. Publish first, announce second. |
| **Opening ratelimit or billing heuristics** | These are the abuse-prevention and monetization levers. Once published, they're permanently public. Bad actors study them; competitors copy them. |
| **Premature repo split** | Two repos before GA means two CI pipelines, two sets of dependency bumps, two merge queues for a solo founder. Don't split until R-8. |
| **"Self-hosting docs" anywhere in the public repo** | CoreLink is a SaaS, not a self-hosted product (see POSITIONING.md anti-reductions table). A `docker-compose.yml` in the public repo permanently anchors the wrong positioning expectation. |

---

## Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-15 | 13 OSS crates identified, license tags applied | DEBT-002 closure; see `docs/internal/OSS-VS-CLOSED-MATRIX.md` |
| 2026-05-30 | OSS_STRATEGY.md written as founder decision doc | Pre-launch gate: explicit record before public announcement |
| 2026-05-31 | DEBT-002 reopened; 4 pre-launch crates re-tagged + CI guard added; `corelink-audit` reclassified closed | Reorg had silently wiped all OSS license tags (0 of 13 tagged). See `specs/_audits/2026-05-31-oss-split-prep.md` + matrix Reconciliation section |
