---
type: "CacheSurface"
title: "_public package-manager surfaces (npm/pip/brew/oci)"
description: "The npm/pip/brew/oci read-through cache surfaces that dedup public upstream content into the shared _public namespace while gating access by PAT and isolating private content."
source_files:
  - "crates/corelink-container/src/routes/npm.rs"
  - "crates/corelink-container/src/routes/pip.rs"
  - "crates/corelink-container/src/routes/brew.rs"
  - "crates/corelink-container/src/routes/oci.rs"
checkpoint_sha: "29e159f2b0c93913ba4c15689fa966195a56deb0"
provenance: "AUTHORED"
tags: ["surfaces", "public", "npm", "pip", "brew", "oci", "moat"]
timestamp: "2026-06-26T00:00:00Z"
---

# _public package-manager surfaces (npm/pip/brew/oci)

These four surfaces turn CoreLink into a read-through caching mirror for the public package ecosystems
— npm (registry tarballs + metadata), pip (PyPI wheels/sdists + simple index), Homebrew (bottles), and
OCI (the full Distribution Spec v1.1 registry that docker/podman/buildah/containerd/Helm speak). Public upstream bytes for **pip and brew** are stored once under the shared `_public` namespace
and deduped **cross-tenant** — the network-effect moat (`crates/corelink-container/src/routes/pip.rs:115-118`,
`crates/corelink-container/src/routes/brew.rs:85-95`). **npm and OCI are per-tenant today:** npm tarball
bytes are namespaced per-tenant (`crates/corelink-container/src/routes/npm.rs:149-177`) — only npm
*metadata* splits public/private — and OCI images are likewise stored under the per-tenant namespace,
isolated by default (`crates/corelink-container/src/routes/oci.rs:188-192`). For both, cross-tenant
public-byte dedup (npm tarballs / public base images) is a tracked, not-yet-built OPEN DECISION. They
build on the same [native CAS](/surfaces/native-cas.md) moat the first-party surfaces use.

# Role
Each surface mounts its `corelink_adapter_host::<pm>` adapter into the container router, derives the
tenant from the bearer PAT (never the path), and enforces per-operation cache scope. **pip and brew**
route immutable public bytes through the shared 2-level `MoatCache` under `PUBLIC_NAMESPACE` (cross-tenant
dedup); **npm and OCI** store bytes under their per-tenant namespace today. Mutable per-package
metadata/indexes live in per-tenant (or public-split) D1 KV stores the content-addressed moat cannot
hold.

# How it works
1. npm dedups tarball BYTES through the moat and splits package METADATA: unscoped (public) metadata
   goes to `PUBLIC_NAMESPACE`, `@scoped` (private) metadata stays per-tenant (`crates/corelink-container/src/routes/npm.rs:35-44`; `crates/corelink-container/src/routes/npm.rs:97-106`).
2. npm mounts via `nest_service("/npm", …)` with the scope/F27 gate as an OUTER layer that rewrites
   `/npm/<tenant>/<rest>` → `/npm/<rest>` before routing (`crates/corelink-container/src/routes/npm.rs:288-352`; `crates/corelink-container/src/routes/npm.rs:350`).
3. pip stores immutable public wheels/sdists under `PUBLIC_NAMESPACE` via the moat and mounts through
   `nest_service("/pip", …)` (`crates/corelink-container/src/routes/pip.rs:32-40`; `crates/corelink-container/src/routes/pip.rs:116-118`; `crates/corelink-container/src/routes/pip.rs:303-372`).
4. brew stores public bottles under `PUBLIC_NAMESPACE` keyed by `blake3(canonical-URL)` and mounts via
   `nest_service("/brew", …)` with an inner gate (its inner route is a catch-all) (`crates/corelink-container/src/routes/brew.rs:84-93`; `crates/corelink-container/src/routes/brew.rs:157-206`).
5. OCI mounts with `.merge` (NOT `nest_service`) because it has NO tenant path segment — the tenant
   rides inside the HMAC bearer minted at `/token`, so the gate does pure scope enforcement and no
   path surgery (`crates/corelink-container/src/routes/oci.rs:10-30`; `crates/corelink-container/src/routes/oci.rs:34-45`; `crates/corelink-container/src/routes/oci.rs:690-753`).
6. OCI also layers a $-ceiling + monthly request-count gate that attributes cost to the tenant
   recovered from the VERIFIED HMAC bearer, never a request header (`crates/corelink-container/src/routes/oci.rs:754-790`).

# Invariants
- Tenant identity comes from the bearer PAT (re-verified, Option B); the path `<tenant>` is NEVER trusted (`crates/corelink-container/src/routes/npm.rs:30-32`; `crates/corelink-container/src/routes/pip.rs:28-30`; `crates/corelink-container/src/routes/brew.rs:25-27`).
- Public upstream bytes are deduped cross-tenant under `PUBLIC_NAMESPACE`; the PAT gates access, the public content is shared (`crates/corelink-container/src/routes/pip.rs:32-37`; `crates/corelink-container/src/routes/brew.rs:27-30`).
- npm `@scoped` (private) packages stay in the per-tenant namespace, never `PUBLIC_NAMESPACE` (`crates/corelink-container/src/routes/npm.rs:97-106`).
- OCI uses `.merge` not `nest_service` because the first segment after `/v2/` is the OCI repo name, not a tenant — stripping it would corrupt the repo (`crates/corelink-container/src/routes/oci.rs:19-30`).
- OCI cost/request-count attribution is keyed on the tenant recovered from the verified bearer, not a (stripped, forgeable) header: `oci_bearer_tenant` calls `oci::auth::verify` and recovers the tenant (`crates/corelink-container/src/routes/oci.rs:816-826`), and `oci_quota_gate` charges that resolved tenant (`crates/corelink-container/src/routes/oci.rs:856-857`).

# Gotchas
- `_public` writes need BOTH a `tenant_storage_state` row AND a sentinel R2 prefix for byte accounting;
  brew/npm/pip pass `None` for the resolved cap and so keep the prior fail-closed-on-fresh-row posture,
  accruing against the tenant's existing stored cap (`crates/corelink-container/src/routes/brew.rs:99-104`).
- OCI failed closed historically: the Worker strips `x-corelink-tenant-id` on the OCI pass-through, so
  the old header-based charge was always empty — a total $-ceiling bypass — until the gate was rekeyed
  on the verified bearer.
- The OCI bearer→tenant attribution is a SEPARATE trust path from the header-based native planes and is
  flagged `SECURITY-REVIEW` (audit #7, pending review): a request with no resolvable verified bearer goes
  UNCOUNTED and the quota gate is fail-OPEN on reads (`crates/corelink-container/src/routes/oci.rs:810-826`).

# Citations
1. `crates/corelink-container/src/routes/npm.rs:35-44` — npm public/private metadata split.
2. `crates/corelink-container/src/routes/npm.rs:97-106` — `namespace_for_meta_key` (`@scoped`→per-tenant, else `PUBLIC_NAMESPACE`).
3. `crates/corelink-container/src/routes/npm.rs:288-352` — npm router (`nest_service` + outer gate rewrite).
4. `crates/corelink-container/src/routes/npm.rs:350` — `nest_service("/npm", …)` mount.
5. `crates/corelink-container/src/routes/npm.rs:30-32` — npm tenant-from-PAT, path never trusted.
6. `crates/corelink-container/src/routes/pip.rs:32-40` — pip wheels/sdists immutable public → `PUBLIC_NAMESPACE`.
7. `crates/corelink-container/src/routes/pip.rs:116-118` — pip moat get under `PUBLIC_NAMESPACE`.
8. `crates/corelink-container/src/routes/pip.rs:303-372` — pip router + gate.
9. `crates/corelink-container/src/routes/pip.rs:28-30` — pip tenant-from-PAT, path never trusted.
10. `crates/corelink-container/src/routes/pip.rs:32-37` — pip access-gated, content-shared model.
11. `crates/corelink-container/src/routes/brew.rs:84-93` — brew moat get under `PUBLIC_NAMESPACE`.
12. `crates/corelink-container/src/routes/brew.rs:157-206` — brew router (`nest_service` + inner gate).
13. `crates/corelink-container/src/routes/brew.rs:25-27` — brew tenant-from-PAT, path never trusted.
14. `crates/corelink-container/src/routes/brew.rs:27-30` — brew public-bottle cross-tenant dedup.
15. `crates/corelink-container/src/routes/brew.rs:99-104` — fresh-row `None`-cap posture.
16. `crates/corelink-container/src/routes/oci.rs:10-30` — OCI `.merge` (no tenant path segment) rationale.
17. `crates/corelink-container/src/routes/oci.rs:34-45` — OCI two-leg `/token` HMAC bearer auth.
18. `crates/corelink-container/src/routes/oci.rs:690-753` — OCI router (`.merge` mount).
19. `crates/corelink-container/src/routes/oci.rs:19-30` — why stripping the first segment would corrupt the repo.
20. `crates/corelink-container/src/routes/oci.rs:754-790` — OCI $-ceiling + request-count gate.
21. `crates/corelink-container/src/routes/oci.rs:816-826` — `oci_bearer_tenant` (verify HMAC bearer → recover tenant); used by `oci_quota_gate` at `crates/corelink-container/src/routes/oci.rs:856-857` — cost attribution keyed on the verified bearer, not a header.
22. `crates/corelink-container/src/routes/pip.rs:115-118` — pip wheels dedup cross-tenant under `PUBLIC_NAMESPACE`.
23. `crates/corelink-container/src/routes/brew.rs:85-95` — brew bottles dedup cross-tenant under `PUBLIC_NAMESPACE`.
24. `crates/corelink-container/src/routes/npm.rs:149-177` — `NpmMoatStore` get/put namespace npm tarball BYTES per-tenant (cross-tenant dedup is a tracked enhancement).
25. `crates/corelink-container/src/routes/oci.rs:188-192` — OCI images stored under the per-tenant namespace; cross-tenant public-image dedup is a tracked OPEN DECISION.
