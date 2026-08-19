---
type: "CacheSurface"
title: "_public package-manager surfaces (npm/pip/brew/oci)"
description: "The npm/pip/brew/oci read-through cache surfaces that dedup public upstream content into the shared _public namespace while gating access by PAT and isolating private content."
source_files:
  - "crates/corelink-container/src/routes/npm.rs"
  - "crates/corelink-container/src/routes/pip.rs"
  - "crates/corelink-container/src/routes/brew.rs"
  - "crates/corelink-container/src/routes/oci.rs"
  - "crates/corelink-container/src/oci_cap.rs"
  - "crates/corelink-container/src/public_base_allowlist.rs"
checkpoint_sha: "9748ac8843a2f4ed34539fae54d91be049a56083"
provenance: "AUTHORED"
tags: ["surfaces", "public", "npm", "pip", "brew", "oci", "moat"]
timestamp: "2026-06-26T00:00:00Z"
---

# _public package-manager surfaces (npm/pip/brew/oci)

These four surfaces turn CoreLink into a read-through caching mirror for the public package ecosystems
— npm (registry tarballs + metadata), pip (PyPI wheels/sdists + simple index), Homebrew (bottles), and
OCI (the full Distribution Spec v1.1 registry that docker/podman/buildah/containerd/Helm speak). Public upstream bytes for **pip and brew** are stored once under the shared `_public` namespace
and deduped **cross-tenant** — the network-effect moat (`crates/corelink-container/src/routes/pip.rs:115-118`,
`crates/corelink-container/src/routes/brew.rs:85-95`). **npm is per-tenant today** (npm tarball
bytes are namespaced per-tenant — `crates/corelink-container/src/routes/npm.rs:149-177` — only npm
*metadata* splits public/private) and its cross-tenant tarball dedup is a tracked, not-yet-built OPEN
DECISION. **OCI is per-tenant by default, with F3.2 increment-6 flag-gated cross-tenant dedup for
allowlisted base LAYER blobs:** when the boot flag is on AND a layer digest is allowlisted, that blob's
put/get is routed to `_public` via a single predicate (`crates/corelink-container/src/routes/oci.rs:286-288`);
default OFF ships the OCI path byte-identical to per-tenant-only. They
build on the same [native CAS](/surfaces/native-cas.md) moat the first-party surfaces use.

# Role
Each surface mounts its `corelink_adapter_host::<pm>` adapter into the container router, derives the
tenant from the bearer PAT (never the path), and enforces per-operation cache scope. **pip and brew**
route immutable public bytes through the shared 2-level `MoatCache` under `PUBLIC_NAMESPACE` (cross-tenant
dedup); **npm** stores bytes per-tenant, and **OCI** stores per-tenant by default but routes an
allowlisted base LAYER to `PUBLIC_NAMESPACE` when the inc6 dedup flag is on
(`crates/corelink-container/src/routes/oci.rs:286-288`). Mutable per-package
metadata/indexes live in per-tenant (or public-split) D1 KV stores the content-addressed moat cannot
hold.

# How it works
1. npm dedups tarball BYTES through the moat and splits package METADATA: unscoped (public) metadata
   goes to `PUBLIC_NAMESPACE`, `@scoped` (private) metadata stays per-tenant (`crates/corelink-container/src/routes/npm.rs:35-44`; `crates/corelink-container/src/routes/npm.rs:97-106`).
2. npm mounts via `nest_service("/npm", …)` with the scope/F27 gate as an OUTER layer that rewrites
   `/npm/<tenant>/<rest>` → `/npm/<rest>` before routing (`crates/corelink-container/src/routes/npm.rs:298-362`; `crates/corelink-container/src/routes/npm.rs:360`).
3. pip stores immutable public wheels/sdists under `PUBLIC_NAMESPACE` via the moat and mounts through
   `nest_service("/pip", …)` (`crates/corelink-container/src/routes/pip.rs:32-40`; `crates/corelink-container/src/routes/pip.rs:116-118`; `crates/corelink-container/src/routes/pip.rs:313-382`).
4. brew stores public bottles under `PUBLIC_NAMESPACE` keyed by `blake3(canonical-URL)` and mounts via
   `nest_service("/brew", …)` with an inner gate (its inner route is a catch-all) (`crates/corelink-container/src/routes/brew.rs:84-93`; `crates/corelink-container/src/routes/brew.rs:163-212`).
5. OCI mounts with `.merge` (NOT `nest_service`) because it has NO tenant path segment — the tenant
   rides inside the HMAC bearer minted at `/token`, so the gate does pure scope enforcement and no
   path surgery (`crates/corelink-container/src/routes/oci.rs:10-30`; `crates/corelink-container/src/routes/oci.rs:34-45`; `crates/corelink-container/src/routes/oci.rs:784-850`).
6. OCI also layers a $-ceiling + monthly request-count gate that attributes cost to the tenant
   recovered from the VERIFIED HMAC bearer, never a request header — and both axes charge EVERY method
   including reads (`docker pull` GETs do real R2-GET work): the `$`-ceiling is fail-CLOSED (`402`), the
   request-count axis fail-OPEN (`429`). CF-2 corrected the gate's source comments so they now state this
   accurately — the doc + body of `oci_quota_gate` both say the `$`-ceiling is charged on reads too
   (`crates/corelink-container/src/routes/oci.rs:950-1006`).
7. Because the Worker forwards `/v2/*` + `/token` RAW (it never sets the storage-cap header for OCI), the
   container resolves the tenant's per-tier storage cap itself at the `/token` mint: `tier_to_cap_bytes`
   ports the Worker's `QUOTAS` table (finite caps per tier, `Some(0)` only for `enterprise`, unknown tier →
   the `free` cap, never unlimited) so the OCI bearer carries the same cap the native plane would resolve
   (`crates/corelink-container/src/oci_cap.rs:63-82`).
8. **F3.2 increment 3 — the public-base allowlist trust root (inert).** Which upstream OCI base-layer
   digests may EVER enter the cross-tenant `_public` namespace is governed by an owner-curated manifest
   BAKED INTO the container binary (`include_str!`), loaded + validated by `PublicBaseAllowlist::from_baked_manifest`
   (`crates/corelink-container/src/public_base_allowlist.rs:54`). `is_allowlisted(digest)` is a constant-set
   membership test the increment-6 `OciMoatStore` router consults before routing a blob to
   `_public` (`crates/corelink-container/src/public_base_allowlist.rs:86`). The loader is FAIL-CLOSED on any
   non-digest entry — a tag has no `sha256:` prefix / wrong length / non-lowercase-hex and is rejected, so a
   mutable tag can never widen the shared namespace (`crates/corelink-container/src/public_base_allowlist.rs:108`).
   As of WP-E Roll-1 the shipped manifest activates exactly the alpine pin (debian stays commented); every other digest still rejects (fail-closed).
9. **F3.2 increment 6 — flag-gated `_public` routing (WP-B).** `OciMoatStore` takes a boot `dedup` flag
   (prod injects `public_flags::oci_public_dedup_enabled()`; the store loads the baked allowlist FAIL-CLOSED,
   degrading a malformed manifest to deny-all) (`crates/corelink-container/src/routes/oci.rs:257-262`). A
   SINGLE predicate `routes_to_public` = `dedup && is_allowlisted(blob_key)` decides routing and is applied
   IDENTICALLY on both paths (`crates/corelink-container/src/routes/oci.rs:286-288`): on write,
   `finalize_upload` persists an allowlisted layer under `PUBLIC_NAMESPACE` with an uncapped `Some(0)`
   quota-seed, everything else under the per-tenant namespace with the resolved cap
   (`crates/corelink-container/src/routes/oci.rs:588-593`); on read, `get_blob` reads `_public` first and
   falls back to the per-tenant namespace on a miss (no upstream fetch — that is WP-G)
   (`crates/corelink-container/src/routes/oci.rs:631-641`). It adds NO new `_public` writer (it only ROUTES
   the existing tenant write) and keeps the write-time `verify_against_bytes` fail-closed on the public path.

# Invariants
- Tenant identity comes from the bearer PAT (re-verified, Option B); the path `<tenant>` is NEVER trusted. The executed enforcers: `PipPatResolver::resolve` / `BrewPatResolver::resolve` derive the tenant by calling `verify(pat_plaintext)` (`crates/corelink-container/src/routes/pip.rs:243-244`; `crates/corelink-container/src/routes/brew.rs:118-119`), and the gate REWRITES the wire path to strip the leading `<tenant>` segment before the adapter sees it (`crates/corelink-container/src/routes/pip.rs:523-528`; `crates/corelink-container/src/routes/brew.rs:336`) (`crates/corelink-container/src/routes/npm.rs:30-32`).
- A PAT verifier LOAD SHED is 503, never 401, on every one of these surfaces. `VerifyError::Backend` — which covers both a D1 fault and an Argon2id permit-pool shed — maps to the adapters' `VerifierOverloaded` variant (`crates/corelink-container/src/routes/pip.rs:246`; `crates/corelink-container/src/routes/npm.rs:250`). Two reasons it cannot be an `Auth`/401: the verifier reached NO verdict on the credential, and the container sheds row-FOUND and row-NOT-FOUND requests identically (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`), a symmetry that is only observable end-to-end if the HTTP status matches on both. `routes/oci.rs` is out of that scope — it is the one Worker pass-through with no edge gating and is already uniformly flat.
- Public upstream bytes are deduped cross-tenant under `PUBLIC_NAMESPACE`; the PAT gates access, the public content is shared (`crates/corelink-container/src/routes/pip.rs:32-37`; `crates/corelink-container/src/routes/brew.rs:27-30`).
- npm `@scoped` (private) packages stay in the per-tenant namespace, never `PUBLIC_NAMESPACE` (`crates/corelink-container/src/routes/npm.rs:97-106`).
- OCI uses `.merge` not `nest_service` because the first segment after `/v2/` is the OCI repo name, not a tenant — stripping it would corrupt the repo (`crates/corelink-container/src/routes/oci.rs:19-30`).
- OCI cost/request-count attribution is keyed on the tenant recovered from the verified bearer, not a (stripped, forgeable) header: `oci_bearer_tenant` calls `oci::auth::verify` and recovers the tenant (`crates/corelink-container/src/routes/oci.rs:927-939`), and `oci_quota_gate` charges that resolved tenant (`crates/corelink-container/src/routes/oci.rs:962-966`).
- **inc6 `_public` routing uses ONE predicate on BOTH the write and read path, so the write namespace always equals the read namespace (no split-brain), and only a `dedup && is_allowlisted` hit reaches the uncapped `Some(0)` quota-seed** — a non-allowlisted blob (including every manifest/config object, whose digest is never a layer-blob allowlist entry) stays per-tenant and quota-charged (`crates/corelink-container/src/routes/oci.rs:286-288`; `crates/corelink-container/src/routes/oci.rs:588-593`; `crates/corelink-container/src/routes/oci.rs:631-641`).
- Eligibility for `_public` is a SERVER-owned, owner-gated, digest-pinned decision — never client-asserted. The trust root is baked into the binary (no runtime mutation path can widen it) and fail-closes on any tag, so only immutable `sha256:` digests the owner committed can ever be allowlisted (`crates/corelink-container/src/public_base_allowlist.rs:108`); as of WP-E Roll-1 the shipped manifest carries exactly the alpine pin (`crates/corelink-container/src/public_base_allowlist.rs:54`).

# Gotchas
- `_public` writes need BOTH a `tenant_storage_state` row AND a sentinel R2 prefix for byte accounting;
  brew/npm/pip pass `None` for the resolved cap and so keep the prior fail-closed-on-fresh-row posture,
  accruing against the tenant's existing stored cap (`crates/corelink-container/src/routes/brew.rs:99-104`).
- OCI failed closed historically: the Worker strips `x-corelink-tenant-id` on the OCI pass-through, so
  the old header-based charge was always empty — a total $-ceiling bypass — until the gate was rekeyed
  on the verified bearer.
- The OCI bearer→tenant attribution is a SEPARATE trust path from the header-based native planes and is
  flagged `SECURITY-REVIEW` (audit #7, pending review): a request with NO resolvable verified bearer goes
  entirely UNCOUNTED on both axes (`crates/corelink-container/src/routes/oci.rs:927-939`). **But the two
  metering axes behave DIFFERENTLY once a tenant IS resolved — do not lump them together as "fail-open on
  reads":** the `$`-ceiling is charged on EVERY method INCLUDING reads and is fail-CLOSED (`402` over
  ceiling) — `docker pull` GET/HEAD of manifests/blobs is real billable R2-GET work, so the old
  read-path carve-out (PR #318) that let an authenticated tenant pull unlimited blobs without hitting
  their ceiling is CLOSED (rt-nuclear cycle-2 #3) (`crates/corelink-container/src/routes/oci.rs:979-991`).
  Only the monthly **request-count** axis is fail-OPEN (it is an availability/SLO limiter, `429` over),
  and it too now counts reads (`crates/corelink-container/src/routes/oci.rs:992-1003`). CF-2 also CORRECTED
  the gate's source comments: the old `SECURITY-REVIEW` comment that read "the quota gate is fail-OPEN on
  reads" (a pre-cycle-2 carry-over) is gone — the comment now states the `$`-ceiling is charged fail-CLOSED
  on reads too and only the request-count axis is fail-OPEN, so the doc no longer contradicts the code
  (`crates/corelink-container/src/routes/oci.rs:941-949`).

# Citations
1. `crates/corelink-container/src/routes/npm.rs:35-44` — npm public/private metadata split.
2. `crates/corelink-container/src/routes/npm.rs:97-106` — `namespace_for_meta_key` (`@scoped`→per-tenant, else `PUBLIC_NAMESPACE`).
3. `crates/corelink-container/src/routes/npm.rs:298-362` — npm router (`nest_service` + outer gate rewrite).
4. `crates/corelink-container/src/routes/npm.rs:360` — `nest_service("/npm", …)` mount.
5. `crates/corelink-container/src/routes/npm.rs:30-32` — npm tenant-from-PAT, path never trusted.
6. `crates/corelink-container/src/routes/pip.rs:32-40` — pip wheels/sdists immutable public → `PUBLIC_NAMESPACE`.
7. `crates/corelink-container/src/routes/pip.rs:116-118` — pip moat get under `PUBLIC_NAMESPACE`.
8. `crates/corelink-container/src/routes/pip.rs:313-382` — pip router + gate.
9. `crates/corelink-container/src/routes/pip.rs:243-244` — `PipPatResolver::resolve` derives the tenant from the verified PAT (executed); the wire `<tenant>` is path-stripped at `crates/corelink-container/src/routes/pip.rs:523-528`.
10. `crates/corelink-container/src/routes/pip.rs:32-37` — pip access-gated, content-shared model.
11. `crates/corelink-container/src/routes/brew.rs:84-93` — brew moat get under `PUBLIC_NAMESPACE`.
12. `crates/corelink-container/src/routes/brew.rs:163-212` — brew router (`nest_service` + inner gate).
13. `crates/corelink-container/src/routes/brew.rs:118-119` — `BrewPatResolver::resolve` derives the tenant from the verified PAT (executed); the wire `<tenant>` is path-stripped at `crates/corelink-container/src/routes/brew.rs:336`.
14. `crates/corelink-container/src/routes/brew.rs:27-30` — brew public-bottle cross-tenant dedup.
15. `crates/corelink-container/src/routes/brew.rs:99-104` — fresh-row `None`-cap posture.
16. `crates/corelink-container/src/routes/oci.rs:10-30` — OCI `.merge` (no tenant path segment) rationale.
17. `crates/corelink-container/src/routes/oci.rs:34-45` — OCI two-leg `/token` HMAC bearer auth.
18. `crates/corelink-container/src/routes/oci.rs:784-850` — OCI router (`.merge` mount).
19. `crates/corelink-container/src/routes/oci.rs:19-30` — why stripping the first segment would corrupt the repo.
20. `crates/corelink-container/src/routes/oci.rs:950-1006` — `oci_quota_gate`: the `$`-ceiling charged on EVERY method incl. reads (fail-CLOSED `402`) + the request-count axis (fail-OPEN `429`); both count GET/HEAD pulls. The previous write-only `$`-ceiling carve-out (PR #318) is closed (rt-nuclear cycle-2 #3); CF-2 corrected the doc/body comments to match.
21. `crates/corelink-container/src/routes/oci.rs:927-939` — `oci_bearer_tenant` (verify HMAC bearer → recover tenant); used by `oci_quota_gate` at `crates/corelink-container/src/routes/oci.rs:962-966` — cost attribution keyed on the verified bearer, not a header.
22. `crates/corelink-container/src/routes/pip.rs:115-118` — pip wheels dedup cross-tenant under `PUBLIC_NAMESPACE`.
23. `crates/corelink-container/src/routes/brew.rs:85-95` — brew bottles dedup cross-tenant under `PUBLIC_NAMESPACE`.
24. `crates/corelink-container/src/routes/npm.rs:149-177` — `NpmMoatStore` get/put namespace npm tarball BYTES per-tenant (cross-tenant dedup is a tracked enhancement).
25. `crates/corelink-container/src/routes/oci.rs:286-288` — `OciMoatStore::routes_to_public` = the SINGLE `dedup && is_allowlisted` predicate applied identically on the write and read path (inc6 flag-gated `_public` routing).
26. `crates/corelink-container/src/routes/oci.rs:257-262` — `OciMoatStore::new` takes the boot `dedup` flag and loads the baked allowlist fail-closed (malformed → deny-all).
27. `crates/corelink-container/src/routes/oci.rs:588-593` — `finalize_upload` routes an allowlisted layer to `PUBLIC_NAMESPACE` (uncapped `Some(0)`), else per-tenant with the resolved cap.
28. `crates/corelink-container/src/routes/oci.rs:631-641` — `get_blob` reads `_public` first for an allowlisted digest, then falls back to the per-tenant namespace on a miss (no upstream fetch).
29. `crates/corelink-container/src/oci_cap.rs:63-82` — `tier_to_cap_bytes`: container-side port of the Worker `QUOTAS` per-tier storage cap for the OCI `/token` mint (unknown tier → `free`, never unlimited).
30. `crates/corelink-container/src/public_base_allowlist.rs:54` — `PublicBaseAllowlist::from_baked_manifest` loads + validates the container-baked owner-curated allowlist (WP-E Roll-1: one active pin, alpine).
31. `crates/corelink-container/src/public_base_allowlist.rs:86` — `is_allowlisted(digest)` membership test the increment-6 `_public` router gates on.
32. `crates/corelink-container/src/public_base_allowlist.rs:108` — `validate_digest` fail-closes on any non-`sha256:` entry (tags BANNED — digest-pinned trust root).
