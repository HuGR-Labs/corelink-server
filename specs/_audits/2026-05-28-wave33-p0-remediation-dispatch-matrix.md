---
id: "DISPATCH-MATRIX-W33-P0-REMEDIATION-2026-05-28"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-28"
updated: "2026-05-28"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dispatch", "p0-remediation", "data-plane", "wave-33", "wiring", "sota"]
references:
  - "specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md"
  - "ARCHITECTURE.md"
  - "crates/corelink-container/src/main.rs"
  - "crates/corelink-container/src/routes.rs"
  - "worker/src/durable_object.ts"
  - "wrangler.toml"
---

# P0 Remediation Wave — wire the data plane (6-agent dispatch matrix)

> **Mandate (2026-05-28):** prepare + organize the P0-remediation wave
> for 6 agents. In parallel, build the black-box E2E user-journey suite
> (dispatched separately, agent `afaaa12b`).
>
> **Goal:** flip the 12-model audit verdict from DO-NOT-SHIP to a
> functioning data plane that the E2E user-journey suite (the real ship
> gate) passes GREEN.

## §0 The honest shape: this is a COUPLED refactor, not 6 disjoint tasks

The P0s all live on ONE request path: Worker → Durable Object →
Container → R2/D1/KV. The files (`worker/src/{index,durable_object}.ts`,
`crates/corelink-container/src/{main.rs,routes.rs,routes/*.rs}`,
`wrangler.toml`) are tightly coupled. Per the techlead §0.6 conflict
protocol, forcing 6 fully-parallel agents here = MUTATION-CONFLICT hell.

Instead: **Phase 1 = 4 parallel WPs on disjoint files; Phase 2 = 2
sequential WPs that integrate (they depend on Phase-1 outputs at build
time).** Up to 6 agents engage, but Phase 2 waits for Phase 1.

## §0.1 Architecture is already decided (ARCHITECTURE.md §"data plane")

- **Control plane (Worker):** auth, quota, rate-limit, billing, admin,
  routing. Stateless.
- **Data plane (Container, native Rust in Firecracker):** the hot loop —
  chunk → hash → **R2 PUT/GET** → verify; AC reads; manifest assembly.
  "Co-located with R2 in-region."

So the container DOES own R2 I/O. The audit's P0-4/P0-5 (no bindings +
`enableInternet:false`) are resolved by: **the native container reaches
R2 via the S3-compatible API over egress** (CF `cf_r2.rs`/`r2_real.rs`
adapters are `#[cfg(target_arch="wasm32")]` = Worker-only; the container
needs an S3 client, not a CF binding).

### DECISION-GATE-1 (Owner sign-off needed before Phase 1 dispatch)
How does the native container reach R2?
- **Option A — R2 S3 API + egress (matches ARCHITECTURE.md).** Set
  `enableInternet:true` (scoped), add R2 S3 access-key/secret as
  container secrets, use an S3 client (aws-sdk-s3 against the R2 S3
  endpoint). Container does PUT/GET directly. Lowest-latency, matches doc.
- **Option B — Worker-proxied storage.** Container stays no-egress;
  Worker (which has R2 bindings) does the byte I/O; container does pure
  compute (hash/chunk/dedup) and hands offsets back. Contradicts
  ARCHITECTURE.md's "Container holds the hot loop / co-located with R2,"
  re-introduces the bandwidth hop the doc explicitly avoids.
- **Recommendation: Option A** (it's the documented design; B re-adds the
  public-internet bandwidth traversal the architecture forbids).

Until DG-1 is resolved, WP-S1 (storage adapter) cannot start.

## §1 WP decomposition

```
PHASE 1 — parallel (4 agents, disjoint files)
  WP-S1  Container R2/D1/KV storage adapters (S3 client + D1/KV access for native)
          — owns: crates/corelink-container/src/storage/ (NEW), routes/*.rs build_handler swaps
          — BLOCKED until DECISION-GATE-1
  WP-A1  Worker/DO auth: PAT validation against D1 PAT store on the live path
          — owns: worker/src/index.ts (auth section), DO auth hook
  WP-T1  Worker/DO tenant routing: derive DO id from validated tenant (kill _pending_auth)
          — owns: worker/src/index.ts (routing section) + durable_object.ts tenant resolve
          — coordinates with WP-A1 (both worker/src/, different functions; union-merge)
  WP-C1  wrangler.toml: container R2-S3 creds + enableInternet + storage bindings + BYOK egress
          — owns: wrangler.toml [[env.prod.containers]] + secrets-checklist rows

PHASE 2 — sequential (2 agents, integrate Phase-1 outputs)
  WP-I1  Container listener + protocol fix: bind composed router to an HTTP listener;
          add HTTP /_health on the served port; align DO probe target (P0-1 + P0-6)
          — owns: crates/corelink-container/src/main.rs + routes.rs factory selection
          — DEPENDS on WP-S1 (real handlers must exist to wire the factory)
  WP-X1  End-to-end integration + real-smoke: run scripts/smoke-prod-corelink.sh
          (CAS PUT/GET checks 3/4/19/20) against a local full-stack (wrangler dev +
          container); make the E2E user-journey suite (agent afaaa12b) go GREEN;
          re-deploy + re-run prod smoke
          — DEPENDS on WP-S1 + WP-A1 + WP-T1 + WP-C1 + WP-I1 all merged
```

## §2 Conflict map

```
                worker/index.ts  worker/DO.ts  container/main.rs  container/routes/  container/storage/  wrangler.toml
WP-S1 storage                                                     █(build_handler)   █new                
WP-A1 auth      █(auth fn)                                                                                
WP-T1 tenant    █(route fn)      █(tenant)                                                                
WP-C1 config                                                                                              █
WP-I1 listener                                 █                  ░(factory wire)                         
WP-X1 integ     (read-only verify across all)
```

- WP-A1 + WP-T1 both edit `worker/src/index.ts` (different functions:
  auth vs routing). UNION-RESOLVABLE — or run sequentially if the
  functions interleave. Pre-dispatch: split index.ts edits by named
  function so they don't overlap.
- WP-S1 (routes/build_handler) + WP-I1 (routes.rs factory) both touch
  the routes layer → WP-I1 is Phase 2 (after WP-S1) by design.
- WP-C1 owns wrangler.toml alone.

## §3 Per-WP contracts (to be refined per the SOTA packet protocol at dispatch)

Each WP, when dispatched, gets the full §0 master constraints (pre-flight,
charter, commit policy) + pre-computed inputs. Summary scope here; the
orchestrator computes LOC tables + exact snippets per WP before dispatch
(lesson from the v1→v2 matrix: no template-grade prompts).

- **WP-S1** — Implement a native-container storage layer: S3 client
  (R2 endpoint) for blob PUT/GET; D1 HTTP API client for metadata; KV
  for negative-cache. Replace `InMemoryCasHandler`/`InMemoryAcHandler`/
  `InMemoryAdminHandler` (`routes/{cas,ac,admin}.rs:build_handler`) with
  real-backed handlers selected at runtime (env/feature). Gate: existing
  route unit tests + a new storage round-trip test (against a local
  MinIO/R2-sim or `#[ignore]`-gated live).
- **WP-A1** — Wire PAT validation onto the live path. The middleware
  exists (`corelink-worker/src/middleware/auth.rs`, `corelink-pat`). Put
  it where requests actually flow (DO ingress or Worker pre-forward):
  parse PAT → HMAC fast-fail → D1 lookup → Argon2id verify → resolve
  tenant. Reject any token not in the D1 PAT store (kills "any 32-256
  char string accepted"). Also fix P1-4 (constant-time token_id compare).
- **WP-T1** — Replace `tenantKey="_pending_auth"` with the validated
  tenant from WP-A1; `idFromName(real_tenant)`; resolve `tenantId` in the
  DO (currently hardcoded null). Validate URL-path tenant matches PAT
  tenant before forward (P1-2 path-spoof). Coordinate index.ts edits with
  WP-A1.
- **WP-C1** — `wrangler.toml`: `[[env.prod.containers]]` add R2-S3 creds
  refs + `enableInternet:true` (scoped) per DG-1 Option A; add any
  container storage bindings; add secrets-checklist rows for the new R2
  S3 access-key/secret (matrix drift gate). `cpu_ms` 30→100 (Sonnet P1-3).
- **WP-I1 (Phase 2)** — `main.rs`: bind `build_with_factory(real_factory)`
  to an axum HTTP listener on the served port; add HTTP `GET /_health`
  there; OR add `tonic-web`/`accept_http1` to the gRPC server. Align the
  DO health-probe target (`durable_object.ts:509`) + protocol. This
  closes P0-1 + P0-6 + P0-3-protocol.
- **WP-X1 (Phase 2)** — Stand up a local full-stack (`wrangler dev` +
  local container), run `scripts/smoke-prod-corelink.sh` (CAS PUT/GET
  checks 3/4/19/20), drive the E2E user-journey suite (agent afaaa12b) to
  GREEN, then re-deploy prod + re-run. This WP OWNS the ship-gate
  verification.

## §4 P1 bugs folded into the wave (fix while wiring)

- P1-1 BYOK non-canonical AAD (`serde_jcs`) → assign to WP-S1 or a sub-task
  (BYOK is data-plane crypto; fix before the data plane carries real DEKs).
- P1-2 Svix webhook replay window → standalone quick fix (signup-worker),
  can ride WP-A1 or a 7th micro-WP.
- P1-3 audit-verifier constant-time → quick fix, ride WP-X1's hardening.
- P1-5..P1-8 rate-limit/quota/index/body-limit → after the data plane is
  live (they only matter once requests flow).

## §5 Dispatch order + the E2E gate

1. **Owner resolves DECISION-GATE-1** (R2 access topology).
2. Dispatch Phase 1: WP-S1, WP-A1, WP-T1, WP-C1 (4 parallel; A1/T1
   index.ts edits pre-split by function).
3. Merge Phase 1 (L0-L7 each). Resolve A1/T1 union on index.ts.
4. Dispatch Phase 2: WP-I1 (after S1 merged) → then WP-X1.
5. **Ship gate = the E2E user-journey suite (agent afaaa12b) GREEN +
   `smoke-prod-corelink.sh` CAS checks GREEN against the deployed binary.**
   NOT `/health`. The audit exists because `/health`-green shipped a shell.

## §6 What this wave does NOT do
- New features. This is purely "make the deployed thing actually serve the
  product the libraries already implement."
- Re-architecture. The Worker/Container split stands (ARCHITECTURE.md).

## §7 DCO
DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
