# CAA-360 wave-2 — SOTA hardening design (lead-pinned, no tradeoffs)

The architecture decisions the owner delegated ("true SOTA, no tradeoffs/gambiarra/bypass").
Lead-pinned here so the wave-2 WP agents transcribe, not design.

## 1. CAS TRUE residency (F7 — the maximal fix, not the per-env minimum)

**Decision:** route every CAS object to the R2 bucket of the **owning tenant's
`primary_region`** (from D1 / the value the residency triggers already enforce as
`blob_meta.region`), NOT the serving env's region. Per-env was the audit's *minimum*
("mirror AC"); per-tenant is correct because the trigger
`trg_blob_meta_region_match_insert` already pins `blob_meta.region == tenant.primary_region`
— so the STORAGE region must equal it too, or storage and metadata diverge (a EU tenant whose
request lands on the IAD env would otherwise write EU-tagged bytes into the US bucket).

**Why no dedup tradeoff:** CAS keys are `<region>/<tenant_prefix(TDK)>/<digest>` — already
**per-tenant-prefixed**, so there is NO cross-tenant dedup to lose. Per-region routing only
changes the leading region segment per tenant. Public deterministic deps (cross-tenant shared)
stay in a **separate global namespace** (`corelink-cas-prod`) — public data has no residency duty.

**Canonical `primary_region` → R2 region map (lead-pinned):**
| tenant.primary_region | R2 region / bucket    | rationale |
|---|---|---|
| `enam` (east N.America) | `iad` / corelink-cas-iad | US |
| `wnam` (west N.America) | `iad` / corelink-cas-iad | US (only US R2 region today) |
| `weur` (west Europe)    | `lhr` / corelink-cas-lhr | EU |
| `afr` (Africa)          | `lhr` / corelink-cas-lhr | nearest jurisdiction w/ a bucket (documented default) |
| `sam` (S.America)       | `sam` / corelink-cas-sam | SA |
| `apac` (Asia-Pacific)   | `nrt` / corelink-cas-nrt | Asia (syd reserved for a future Oceania split) |

Buckets `corelink-cas-{iad,lhr,sam,nrt,syd}` are PROVISIONED (2026-06-13). The container has
the S3 creds for all regions (it can write cross-region), so a geo-mismatched request still
writes to the tenant's region (correct, at the cost of one cross-region S3 hop — acceptable;
correctness > the rare-case latency).

**Implementation (wave-2 WP):** a `cas_region_for_primary_region(&str) -> &str` map + the CAS
write/read path resolves the tenant's `primary_region` (it already has it on the request path —
the same value the trigger checks) and selects the bucket via the map. Mirror onto AC for
consistency if cheap; if AC's per-env model is load-bearing elsewhere, scope AC to a follow-up
and do CAS now. Add an invariant test: a tenant in `weur` writes to corelink-cas-lhr, never iad.

## 2. F4 — internal-auth hardening (all three, SOTA)

**Root cause:** ONE shared, internet-reachable secret (`CORELINK_INTERNAL_AUTH_KEY`) gates all
of `/_internal/*` (mint, dsr, cas-erase). A leak = any-tenant admin-PAT minting.

**Decision (3 parts):**
1. **Remove the public `/_internal/*` route — Service-Binding-only.** signup-worker → this
   Worker internal calls move to a CF **Service Binding** (worker-to-worker, never traverses
   the public internet) instead of a public `fetch(corelink-api/_internal/...)`. After this,
   `/_internal/*` is NOT publicly routable. (The runners introspect at `/internal/v1/auth/
   introspect` stays public — it's FABRIC-gated + is the only legit external internal caller.)
   This is the strongest control (eliminates the attack surface, not just narrows it).
2. **Per-consumer secrets.** Split `CORELINK_INTERNAL_AUTH_KEY` into purpose-scoped secrets:
   `CORELINK_INTERNAL_MINT_KEY` (pat/mint), keep `CORELINK_INTERNAL_AUTH_KEY` for the rest, and
   the onboarding/tier-select path gets its own. Tight blast radius per leak.
3. **Per-tenant authz on mint.** `/_internal/pat/mint` must verify the caller is authorized for
   the *specific* tenant it mints for (not just "holds the shared secret").

**Sequencing note:** Service-Binding (part 1) touches wrangler.toml (`[[services]]`) +
signup-worker's call site + the Worker's route handling — coordinate so the binding is live
BEFORE the public route is removed (no internal-call outage window).

## 3. F3 — in-container PAT re-verify (defense-in-depth, yes)

**Decision:** wire `adapter_pat::PatVerifier` onto the native CAS/AC/Bazel/Turbo plane so the
CONTAINER independently re-verifies the PAT (not merely trusting the Worker-injected,
header-stripped tenant). The Worker stays the first gate; the container becomes a second,
independent authority — two independent failures required to cross a tenant boundary. Resolve
the tenant from the re-verified PAT and assert it matches the Worker-injected `x-corelink-
tenant-id`; mismatch → reject (this also closes any future Worker-side regression).

---

These three ship in **one wave-2 container rebuild** (after wave-1 integrates). The
Service-Binding config + per-consumer secret provisioning are lead/infra steps around the wave.
