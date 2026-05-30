---
id: "ADR-MULTI-REGION-V1"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-30"
updated: "2026-05-30"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "multi-region", "containers", "cloudflare", "wrangler", "r2", "iad", "sam", "lhr", "nrt", "syd", "deploy"]
---

# ADR-MULTI-REGION-V1 — Multi-Region Container Deployments v1 (Approach A: Per-Region Worker Envs)

## Status

ACTIVE — implemented 2026-05-30, closes #377 phase 1.

## Context

CoreLink production runs a single Cloudflare Container app (`corelink-prod-corelinkserver-prod`)
pinned to IAD. Five R2 bucket sets are provisioned (`iad`, `sam`, `lhr`, `nrt`, `syd`) but only
IAD is exercised. The other four regions have empty AC, chunk, and manifest buckets.

Container region selection is **per-deploy**, not per-request: `corelink-container` reads
`R2_AC_BUCKET` and `R2_AC_REGION` at startup (confirmed by A5 agent, commit `c47eb806`).
A single container instance serves exactly one region. To exercise SAM / LHR / NRT / SYD, four
additional container applications must be deployed, each with the correct regional bucket env vars.

**Region taxonomy divergence** (open debt): `corelink-region/src/region.rs` defines a 4-value
enum using CF location-hint names (`wnam`, `enam`, `weur`, `sam`). Prod R2 buckets use CF
airport codes (`iad`, `sam`, `lhr`, `nrt`, `syd`) — 5 regions, not 4. LHR, NRT, SYD are not
in the Rust enum. This divergence is noted but NOT resolved in this ADR; it is tracked as a
separate debt item (see §Open Issues).

## Decision

**Approach A (chosen for v1): per-region wrangler.toml env blocks + per-region container apps.**

Deploy four new wrangler.toml environment blocks (`[env.prod-sam]`, `[env.prod-lhr]`,
`[env.prod-nrt]`, `[env.prod-syd]`), each targeting:

- A distinct worker name: `corelink-prod-<region>` (e.g. `corelink-prod-sam`)
- A distinct CF Container app: `corelink-prod-<region>-corelinkserver-prod`
- The same container image tag (`:6b816156-r2`) as IAD — no code change in the Rust binary
- Region-specific `R2_AC_BUCKET`, `R2_AC_REGION`, `R2_CHUNK_BUCKET`, `R2_CHUNK_REGION` vars
- A regional route: `<region>.corelink-api.humangr.com/*`
- The single global CAS bucket (`corelink-cas-prod`, IAD) — CAS is content-addressed and
  globally deduped by hash; a per-region CAS bucket is a follow-up decision
- The shared global D1, KV, and Clerk bindings (same IDs as `[env.prod]`)

Each regional worker env's `CORELINK_SERVER` DO binding (same class name, different CF env)
creates a **separate DO namespace** in CF infrastructure — tenant DO state is isolated per region.

Existing IAD tenants (`primary_region = 'enam'`) are unaffected: they continue to use
`corelink-api.humangr.com` → `corelink-prod` → `corelink-prod-corelinkserver-prod`.

New tenants arriving at a non-IAD colo can be directed by the signup-worker to the correct
regional subdomain (`sam.corelink-api.humangr.com`, etc.) based on `CF-Ray` colo at signup.
This per-tenant routing at signup is a follow-up (see §Open Issues / Approach B).

## Region → Bucket Mapping

| wrangler env | Worker name | Route | R2_AC_BUCKET | R2_AC_REGION | R2_CHUNK_BUCKET | R2_CHUNK_REGION | Container app |
|---|---|---|---|---|---|---|---|
| `prod` (IAD, existing) | `corelink-prod` | `corelink-api.humangr.com/*` | `corelink-ac-iad` | `iad` | `corelink-chunk-iad` | `iad` | `corelink-prod-corelinkserver-prod` |
| `prod-sam` | `corelink-prod-sam` | `sam.corelink-api.humangr.com/*` | `corelink-ac-sam` | `sam` | `corelink-chunk-sam` | `sam` | `corelink-prod-sam-corelinkserver-prod` |
| `prod-lhr` | `corelink-prod-lhr` | `lhr.corelink-api.humangr.com/*` | `corelink-ac-lhr` | `lhr` | `corelink-chunk-lhr` | `lhr` | `corelink-prod-lhr-corelinkserver-prod` |
| `prod-nrt` | `corelink-prod-nrt` | `nrt.corelink-api.humangr.com/*` | `corelink-ac-nrt` | `nrt` | `corelink-chunk-nrt` | `nrt` | `corelink-prod-nrt-corelinkserver-prod` |
| `prod-syd` | `corelink-prod-syd` | `syd.corelink-api.humangr.com/*` | `corelink-ac-syd` | `syd` | `corelink-chunk-syd` | `syd` | `corelink-prod-syd-corelinkserver-prod` |

CAS bucket is `corelink-cas-prod` (IAD-only, globally shared) for ALL regions.

## Worker Code Changes

Two minimal TS changes (< 30 LOC combined):

1. **`worker/src/index.ts` `Env` interface**: add optional fields
   `R2_AC_BUCKET`, `R2_AC_REGION`, `R2_CHUNK_BUCKET`, `R2_CHUNK_REGION`.
   These are non-secret vars set in each regional env block's `vars`.

2. **`worker/src/durable_object.ts` `container.start({ env })`**: forward the four new vars
   to the container process. Absent (IAD default) → container falls back to `corelink-ac-iad`
   / `iad` / `corelink-chunk-iad` / `iad` (existing default in the Rust binary).

No changes to the container Rust binary (`crates/corelink-container`) are required — it already
reads these four env vars at startup.

## Image Tagging

The same compiled image (`corelink-prod-corelinkserver-prod:6b816156-r2`) is tagged and pushed
to four additional CF Container registry paths:

```
registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-prod-sam-corelinkserver-prod:6b816156-r2
registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-prod-lhr-corelinkserver-prod:6b816156-r2
registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-prod-nrt-corelinkserver-prod:6b816156-r2
registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-prod-syd-corelinkserver-prod:6b816156-r2
```

CF Container registry naming convention: `<worker-name>-<class-name>-<env>`, lowercased.

## Migration Path

- **IAD (existing)**: no change. 11 live instances on `:6b816156-r2` remain untouched.
- **New SAM/LHR/NRT/SYD tenants**: directed to `<region>.corelink-api.humangr.com` by the
  signup-worker using CF-Ray colo code at signup time (follow-up: see §Open Issues).
- **Existing tenants with `primary_region != 'enam'`**: currently none in prod (all backfilled
  to `enam`). Transition path: re-signups or manual admin migration.
- **BYOK tenants**: BYOK vault config is region-agnostic at this layer (SAM containers talk to
  the same BYOK-KMS endpoints); no BYOK-specific changes needed.

## Roll-Forward

1. `wrangler deploy --env prod-<region>` for each of sam/lhr/nrt/syd
2. Verify `wrangler containers list` shows 5 applications
3. Probe `https://<region>.corelink-api.humangr.com/_health/container` → `{"status":"ok","storage":"r2"}`
4. Wire signup-worker to route new tenants to the correct regional subdomain (follow-up)

## Rollback

For each new region: `wrangler delete --env prod-<region>` removes the worker + associated DO
namespace. The regional R2 buckets are NOT deleted (rollback is stateless at the bucket layer).
IAD is unaffected by any regional rollback.

## Rejected Alternative: Approach B — Per-Request Region Routing in Container

Container reads tenant ID per gRPC request → looks up `tenant.primary_region` in D1 → picks
the correct regional R2 bucket on each read/write.

**Why deferred:**
- Requires significant container-side refactor in `crates/corelink-container/src/routes/cas.rs`
  and `routes/ac.rs`: handler construction becomes per-request lazy rather than per-startup eager.
- Per-request D1 lookup adds ~5-20ms latency on every object operation in a hot path.
- `corelink-region::Region` enum does not include LHR/NRT/SYD — enum extension required.
- Single container binary would hold 5 simultaneous R2 client connections.

**When to revisit Approach B:**
- When traffic patterns show >10% of requests would benefit from single-worker multi-region routing
- When the Region enum divergence is resolved
- When per-request D1 latency budget is confirmed acceptable under load

## Open Issues

1. **Region enum divergence**: `region.rs` has `wnam/enam/weur/sam` (location hints); prod
   buckets use `iad/sam/lhr/nrt/syd` (airport codes). Resolution: add `Lhr`, `Nrt`, `Syd`
   variants to `Region` enum OR rename buckets to match hint names. Tracked separately.

2. **Per-tenant routing at signup**: `signup-worker/regionFromColo` returns CF colo code.
   Map colo → regional subdomain at signup so new tenants are automatically routed to the
   correct region. Blocked by DNS: `sam/lhr/nrt/syd.corelink-api.humangr.com` CNAMEs needed.

3. **CAS multi-region**: today `corelink-cas-prod` is IAD-only. Separate per-region CAS
   buckets (or CAS replication) needed for latency-sensitive non-IAD tenants. Deferred.

4. **Manifest buckets**: `corelink-manifest-{iad,sam,lhr,nrt,syd}` exist in the account but
   are not wired in any Rust crate. Document purpose or remove.

5. **WEUR compliance**: LHR maps to `weur` (EU). `DoJurisdiction::Eu` MUST be enforced for
   the LHR DO namespace (Schrems II / GDPR Art. 46). Tracked by ADR-S14-002. The LHR env
   block is deployed here; DO jurisdiction enforcement is a Cloudflare-platform concern
   (set via account API, not wrangler.toml). Follow-up required.

## Consequences

- 5 container applications in CF account (was 1): IAD + SAM + LHR + NRT + SYD
- 4 additional wrangler deploys required on each release (coordinated via CI matrix)
- Per-region secrets must be provisioned separately: `wrangler secret put --env prod-<region>`
  for `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`, `CF_API_TOKEN`,
  `PAT_SIGNING_KEY`, `CORELINK_INTERNAL_AUTH_KEY`, `CLOUDFLARE_ACCOUNT_ID`
- Regional subdomains require DNS CNAMEs (Phase G equivalent per region)
- Operational cost: ~5x container instances at steady state if all regions fully loaded;
  CF Containers billing is per-instance-hour, not per-region-deployment
