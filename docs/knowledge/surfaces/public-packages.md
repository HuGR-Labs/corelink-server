---
type: "CacheSurface"
title: "_public package-manager surfaces (npm/pip/brew/oci)"
description: "The npm/pip/brew/oci read-through cache surfaces that dedup public upstream content into the shared _public namespace while gating access by PAT and isolating private content."
source_files:
  - "crates/corelink-container/src/routes/npm.rs"
  - "crates/corelink-container/src/routes/pip.rs"
  - "crates/corelink-container/src/routes/brew.rs"
  - "crates/corelink-container/src/routes/oci.rs"
  - "crates/corelink-container/src/routes/public_pullthrough.rs"
  - "crates/corelink-container/src/oci_cap.rs"
  - "crates/corelink-container/src/public_base_allowlist.rs"
  - "crates/corelink-container/src/public_base_allowlist.manifest"
  - "wrangler.toml"
checkpoint_sha: "f0fe680dc251781133d48793e122d7c7da987f08"
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
DECISION. **OCI is per-tenant by default, with F3.2 flag-gated cross-tenant dedup:** the WRITE side stays
allowlist-gated — only an owner-allowlisted base LAYER digest is stored to `_public`, via the single predicate
`routes_to_public` (`crates/corelink-container/src/routes/oci.rs:289-291`) — while WP-G M2 makes the blob READ
side EXISTENCE-based: when the boot `dedup` flag is on, `get_blob` reads the shared `_public` namespace FIRST
for ANY digest (`crates/corelink-container/src/routes/oci.rs:645-663`), safe because a `sha256:`-addressed blob
in `_public` is byte-identical to any private copy. Default OFF ships the OCI path byte-identical to
per-tenant-only. They
build on the same [native CAS](/surfaces/native-cas.md) moat the first-party surfaces use.

# Role
Each surface mounts its `corelink_adapter_host::<pm>` adapter into the container router, derives the
tenant from the bearer PAT (never the path), and enforces per-operation cache scope. **pip and brew**
route immutable public bytes through the shared 2-level `MoatCache` under `PUBLIC_NAMESPACE` (cross-tenant
dedup); **npm** stores bytes per-tenant, and **OCI** stores per-tenant by default but routes an
allowlisted base LAYER to `PUBLIC_NAMESPACE` on the WRITE path when the dedup flag is on
(`crates/corelink-container/src/routes/oci.rs:289-291`). Mutable per-package
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
   path surgery (`crates/corelink-container/src/routes/oci.rs:10-30`; `crates/corelink-container/src/routes/oci.rs:34-45`; `crates/corelink-container/src/routes/oci.rs:798-898`).
6. OCI also layers a $-ceiling + monthly request-count gate that attributes cost to the tenant
   recovered from the VERIFIED HMAC bearer, never a request header — and both axes charge EVERY method
   including reads (`docker pull` GETs do real R2-GET work): the `$`-ceiling is fail-CLOSED (`402`), the
   request-count axis fail-OPEN (`429`). CF-2 corrected the gate's source comments so they now state this
   accurately — the doc + body of `oci_quota_gate` both say the `$`-ceiling is charged on reads too
   (`crates/corelink-container/src/routes/oci.rs:998-1054`).
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
   As of **WP-G M3** the shipped manifest activates **6** pins — the alpine WP-E rootfs LAYER blob
   (`crates/corelink-container/src/public_base_allowlist.manifest:87`) PLUS 5 base-image multi-arch
   INDEX digests (alpine/debian12/ubuntu24.04/node22-slim/python3.12-slim,
   `crates/corelink-container/src/public_base_allowlist.manifest:90-94`); the baked test asserts the set
   parses and `len() == 6` (`crates/corelink-container/src/public_base_allowlist.rs:163`,
   `crates/corelink-container/src/public_base_allowlist.rs:170`). LAYER pins gate the client-push WRITE
   route; INDEX pins are consumed only by the WP-G M2 server-side closure-promote (a client can never
   populate `_public` for them — they are not layer digests). Every other digest still rejects
   (fail-closed).
9. **F3.2 flag-gated `_public` routing (WP-B write gate + WP-G M2 existence read).** The boot `dedup` flag
   (`public_flags::oci_public_dedup_enabled()`) and the baked allowlist are read ONCE at the router and SHARED
   with both the blob store (`OciMoatStore::with_allowlist`) and the manifest resolver; the allowlist load is
   FAIL-CLOSED, degrading a malformed manifest to deny-all
   (`crates/corelink-container/src/routes/oci.rs:829-830`; `crates/corelink-container/src/routes/oci.rs:854-858`).
   **WP-G M3 activation:** both boot flags are now `"1"` in every prod env block —
   `OCI_PUBLIC_DEDUP_ENABLED` (`wrangler.toml:749`) and `OCI_UPSTREAM_ON_MISS` (`wrangler.toml:750`) —
   forwarded to the container through the DO env-contract (see [DO lifecycle](/planes/durable-object.md)),
   so the existence-read + upstream-on-miss closure-promote paths are LIVE in prod, not inert.
   **WRITE path (still allowlist-gated):** the predicate `routes_to_public` = `dedup && is_allowlisted(blob_key)`
   (`crates/corelink-container/src/routes/oci.rs:289-291`) decides `finalize_upload`'s namespace — an
   allowlisted layer lands under `PUBLIC_NAMESPACE` with an uncapped `Some(0)` quota-seed, everything else
   under the per-tenant namespace with the resolved cap
   (`crates/corelink-container/src/routes/oci.rs:592-596`). **READ path (M2 = EXISTENCE, not allowlist):**
   `get_blob` reads the shared `_public` namespace FIRST for ANY blob key whenever `dedup` is on, then falls
   back to the per-tenant namespace on a miss (no upstream fetch — read pull-through promotion is the
   resolver's job) (`crates/corelink-container/src/routes/oci.rs:645-663`). This is SAFE because an OCI blob
   is `sha256:`-addressed, so a `_public` copy of a digest is byte-identical to any private copy (nothing to
   leak); admission to `_public` stays WRITE-gated (client push via `routes_to_public`; resolver closure-promote
   from an allowlisted ROOT) and revocation still filters on read via `MoatCache::get`'s `public_blocklist`.
   The M2 read REPLACES the earlier inc6 allowlist-on-read so a transitively-promoted child layer (config/layer
   of an allowlisted base, not itself allowlisted) serves cross-tenant. The write-time `verify_against_bytes`
   stays fail-closed on the public path.

# Invariants
- Tenant identity comes from the bearer PAT (re-verified, Option B); the path `<tenant>` is NEVER trusted. The executed enforcers: `PipPatResolver::resolve` / `BrewPatResolver::resolve` derive the tenant by calling `verify(pat_plaintext)` (`crates/corelink-container/src/routes/pip.rs:243-244`; `crates/corelink-container/src/routes/brew.rs:118-119`), and the gate REWRITES the wire path to strip the leading `<tenant>` segment before the adapter sees it (`crates/corelink-container/src/routes/pip.rs:523-528`; `crates/corelink-container/src/routes/brew.rs:336`) (`crates/corelink-container/src/routes/npm.rs:30-32`).
- A PAT verifier LOAD SHED is 503, never 401, on every one of these surfaces. `VerifyError::Backend` — which covers both a D1 fault and an Argon2id permit-pool shed — maps to the adapters' `VerifierOverloaded` variant (`crates/corelink-container/src/routes/pip.rs:246`; `crates/corelink-container/src/routes/npm.rs:250`). Two reasons it cannot be an `Auth`/401: the verifier reached NO verdict on the credential, and the container sheds row-FOUND and row-NOT-FOUND requests identically (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`), a symmetry that is only observable end-to-end if the HTTP status matches on both. `routes/oci.rs` is out of that scope — it is the one Worker pass-through with no edge gating and is already uniformly flat.
- Public upstream bytes are deduped cross-tenant under `PUBLIC_NAMESPACE`; the PAT gates access, the public content is shared (`crates/corelink-container/src/routes/pip.rs:32-37`; `crates/corelink-container/src/routes/brew.rs:27-30`).
- npm `@scoped` (private) packages stay in the per-tenant namespace, never `PUBLIC_NAMESPACE` (`crates/corelink-container/src/routes/npm.rs:97-106`).
- OCI uses `.merge` not `nest_service` because the first segment after `/v2/` is the OCI repo name, not a tenant — stripping it would corrupt the repo (`crates/corelink-container/src/routes/oci.rs:19-30`).
- OCI cost/request-count attribution is keyed on the tenant recovered from the verified bearer, not a (stripped, forgeable) header: `oci_bearer_tenant` calls `oci::auth::verify` and recovers the tenant (`crates/corelink-container/src/routes/oci.rs:975-987`), and `oci_quota_gate` charges that resolved tenant (`crates/corelink-container/src/routes/oci.rs:1035-1039`).
- **The `_public` WRITE is allowlist-gated and the M2 READ is existence-based, but they cannot split-brain: the read is a strict SUPERSET of the write.** Only a `dedup && is_allowlisted` hit reaches `PUBLIC_NAMESPACE` with the uncapped `Some(0)` quota-seed on write (`routes_to_public`, `crates/corelink-container/src/routes/oci.rs:289-291`; `crates/corelink-container/src/routes/oci.rs:592-596`), so a non-allowlisted client push (including every manifest/config object, whose digest is never a layer-blob allowlist entry) stays per-tenant and quota-charged. The read reads `_public` FIRST by EXISTENCE for any digest under `dedup` (`crates/corelink-container/src/routes/oci.rs:645-663`): anything the write path (or the resolver's closure-promote) placed in `_public` is always found there, and content-addressing makes serving it leak-free — so a by-digest read never disagrees with where the bytes actually live.
- Eligibility for `_public` is a SERVER-owned, owner-gated, digest-pinned decision — never client-asserted. The trust root is baked into the binary (no runtime mutation path can widen it) and fail-closes on any tag, so only immutable `sha256:` digests the owner committed can ever be allowlisted (`crates/corelink-container/src/public_base_allowlist.rs:108`); as of WP-G M3 the shipped manifest carries 6 active pins — the alpine LAYER blob plus 5 base-image INDEX digests (`crates/corelink-container/src/public_base_allowlist.manifest:87`, `crates/corelink-container/src/public_base_allowlist.manifest:90-94`; loaded by `crates/corelink-container/src/public_base_allowlist.rs:54`).

# Gotchas
- `_public` writes need BOTH a `tenant_storage_state` row AND a sentinel R2 prefix for byte accounting;
  brew/npm/pip pass `None` for the resolved cap and so keep the prior fail-closed-on-fresh-row posture,
  accruing against the tenant's existing stored cap (`crates/corelink-container/src/routes/brew.rs:99-104`).
- OCI failed closed historically: the Worker strips `x-corelink-tenant-id` on the OCI pass-through, so
  the old header-based charge was always empty — a total $-ceiling bypass — until the gate was rekeyed
  on the verified bearer.
- The OCI bearer→tenant attribution is a SEPARATE trust path from the header-based native planes and is
  flagged `SECURITY-REVIEW` (audit #7, pending review): a request with NO resolvable verified bearer goes
  entirely UNCOUNTED on both axes (`crates/corelink-container/src/routes/oci.rs:975-987`). **But the two
  metering axes behave DIFFERENTLY once a tenant IS resolved — do not lump them together as "fail-open on
  reads":** the `$`-ceiling is charged on EVERY method INCLUDING reads and is fail-CLOSED (`402` over
  ceiling) — `docker pull` GET/HEAD of manifests/blobs is real billable R2-GET work, so the old
  read-path carve-out (PR #318) that let an authenticated tenant pull unlimited blobs without hitting
  their ceiling is CLOSED (rt-nuclear cycle-2 #3) (`crates/corelink-container/src/routes/oci.rs:1027-1039`).
  Only the monthly **request-count** axis is fail-OPEN (it is an availability/SLO limiter, `429` over),
  and it too now counts reads (`crates/corelink-container/src/routes/oci.rs:1040-1051`). CF-2 also CORRECTED
  the gate's source comments: the old `SECURITY-REVIEW` comment that read "the quota gate is fail-OPEN on
  reads" (a pre-cycle-2 carry-over) is gone — the comment now states the `$`-ceiling is charged fail-CLOSED
  on reads too and only the request-count axis is fail-OPEN, so the doc no longer contradicts the code
  (`crates/corelink-container/src/routes/oci.rs:989-997`).

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
18. `crates/corelink-container/src/routes/oci.rs:798-898` — OCI router (`.merge` mount). The router reads the dedup flag + loads the owner allowlist ONCE and SHARES both with the blob store and the resolver (`crates/corelink-container/src/routes/oci.rs:829-830`), then conditionally builds an `UpstreamManifestResolver` and hands it to the adapter (`crates/corelink-container/src/routes/oci.rs:841-852`, `crates/corelink-container/src/routes/oci.rs:881`): WP-G (M1) manifest upstream-on-miss, built ONLY when the boot flag `crate::public_flags::oci_upstream_on_miss()` is ON and the shared SSRF-safe upstream client builds, else `None`. Default OFF ⇒ the manifest handlers 404 a KV miss exactly as before (byte-identical). It shares the moat + manifest KV + fail-closed cap resolver; under WP-G M2 (`dedup` on) a by-digest resolve reads `_public` cross-tenant and an allowlisted ROOT closure-promotes into `_public` (see cite 32) — so, unlike M1, the resolver IS a `_public` writer, gated to owner-allowlisted roots.
19. `crates/corelink-container/src/routes/oci.rs:19-30` — why stripping the first segment would corrupt the repo.
20. `crates/corelink-container/src/routes/oci.rs:998-1054` — `oci_quota_gate`: the `$`-ceiling charged on EVERY method incl. reads (fail-CLOSED `402`) + the request-count axis (fail-OPEN `429`); both count GET/HEAD pulls. The previous write-only `$`-ceiling carve-out (PR #318) is closed (rt-nuclear cycle-2 #3); CF-2 corrected the doc/body comments to match.
21. `crates/corelink-container/src/routes/oci.rs:975-987` — `oci_bearer_tenant` (verify HMAC bearer → recover tenant); used by `oci_quota_gate` at `crates/corelink-container/src/routes/oci.rs:1035-1039` — cost attribution keyed on the verified bearer, not a header.
22. `crates/corelink-container/src/routes/pip.rs:115-118` — pip wheels dedup cross-tenant under `PUBLIC_NAMESPACE`.
23. `crates/corelink-container/src/routes/brew.rs:85-95` — brew bottles dedup cross-tenant under `PUBLIC_NAMESPACE`.
24. `crates/corelink-container/src/routes/npm.rs:149-177` — `NpmMoatStore` get/put namespace npm tarball BYTES per-tenant (cross-tenant dedup is a tracked enhancement).
25. `crates/corelink-container/src/routes/oci.rs:289-291` — `OciMoatStore::routes_to_public` = the `dedup && is_allowlisted` predicate that gates the WRITE path (`finalize_upload`); M2's read path (`get_blob`) instead reads `_public` by existence under the `dedup` flag (cite 28).
26. `crates/corelink-container/src/routes/oci.rs:829-830` — the router reads the boot `dedup` flag (`public_flags::oci_public_dedup_enabled()`) and loads the baked allowlist fail-closed (malformed → deny-all via `unwrap_or_default()`) ONCE, then passes both to `OciMoatStore::with_allowlist` (`crates/corelink-container/src/routes/oci.rs:854-858`); `OciMoatStore::new` is now `#[cfg(test)]`.
27. `crates/corelink-container/src/routes/oci.rs:592-596` — `finalize_upload` routes an allowlisted layer to `PUBLIC_NAMESPACE` (uncapped `Some(0)`), else per-tenant with the resolved cap.
28. `crates/corelink-container/src/routes/oci.rs:645-663` — WP-G M2: `get_blob` reads the shared `_public` namespace FIRST by EXISTENCE (any blob key) when the `dedup` flag is on, then falls back to the per-tenant namespace on a miss (no upstream fetch — read pull-through promotion is the resolver's job). Safe because an OCI blob is `sha256:`-addressed, so a `_public` copy is byte-identical to any private copy; admission to `_public` stays WRITE-gated (cite 25) and revocation still filters via `MoatCache::get`'s `public_blocklist`.
29. `crates/corelink-container/src/oci_cap.rs:63-82` — `tier_to_cap_bytes`: container-side port of the Worker `QUOTAS` per-tier storage cap for the OCI `/token` mint (unknown tier → `free`, never unlimited).
30. `crates/corelink-container/src/public_base_allowlist.rs:54` — `PublicBaseAllowlist::from_baked_manifest` loads + validates the container-baked owner-curated allowlist (WP-G M3: 6 active pins).
30b. `crates/corelink-container/src/public_base_allowlist.manifest:87` — the alpine WP-E rootfs LAYER blob pin; `crates/corelink-container/src/public_base_allowlist.manifest:90-94` — the 5 base-image multi-arch INDEX digests (alpine/debian12/ubuntu24.04/node22-slim/python3.12-slim) consumed by the WP-G M2 closure-promote. 6 active pins total.
30c. `crates/corelink-container/src/public_base_allowlist.rs:163` — the baked test `baked_manifest_has_m3_pins_active`; `crates/corelink-container/src/public_base_allowlist.rs:170` — asserts `len() == 6`.
30d. `wrangler.toml:749` (`OCI_PUBLIC_DEDUP_ENABLED = "1"`) + `wrangler.toml:750` (`OCI_UPSTREAM_ON_MISS = "1"`) — both keystone flags active in prod (all 5 prod env blocks); M3 activation.
31. `crates/corelink-container/src/public_base_allowlist.rs:86` — `is_allowlisted(digest)` membership test the increment-6 `_public` router gates on.
32. `crates/corelink-container/src/routes/public_pullthrough.rs:653` — `UpstreamManifestResolver::resolve_on_miss` (impl `ManifestResolver`): the OCI manifest+blob upstream-on-miss resolver wired conditionally into the router (cite 18). Rate-limited per-tenant, single-flight coalesced, and digest-verified before caching (`crates/corelink-container/src/routes/public_pullthrough.rs:722-726`) — a manifest that fails `verify_against_bytes` is dropped (`Ok(None)`), never persisted; every step fail-opens to `Ok(None)` so the handler 404s the miss. Constructed by `UpstreamManifestResolver::new` (`crates/corelink-container/src/routes/public_pullthrough.rs:193-199`), which returns `None` when the fixed-upstream client cannot be built (flag then inert). Reuses the ONE audited SSRF/token client shared with the `_public` mirror. Under WP-G M2 (`dedup` on), a by-DIGEST reference reads `_public` by existence first (`crates/corelink-container/src/routes/public_pullthrough.rs:676-699`) and, when the digest is an allowlisted ROOT, promotes its full transitive closure into `_public` (`promote_public_closure`) — so this path IS a `_public` writer, but ONLY for owner-allowlisted roots; every `_public` write is digest-verified fail-closed and a TAG reference never reads or writes `_public`.
32. `crates/corelink-container/src/public_base_allowlist.rs:108` — `validate_digest` fail-closes on any non-`sha256:` entry (tags BANNED — digest-pinned trust root).
