# GO-LIVE secreview — GDPR / DSR / data-residency / compliance (read-only)

**Date:** 2026-06-23 · **Scope:** DSR erasure completeness, the F-003/F-004/F-010
erasure cluster, the Ed25519 attestation, data residency (EU un-soften + BR/APAC
roadmap), `verify-lgpd-residency.py` (F-021), and published claims vs reality.
**Method:** read the current `main` code/config against the pentest doc's RESOLVED
claims (`docs/security/2026-06-22-overnight-pentest-findings.md`). All file:line
references verified in-tree.

## Verdict (headline)

- **Is erasure complete?** **YES, for the live data planes (CAS + AC + D1 +
  Stripe), complete-by-construction.** The F-003 (AC), F-004 (cross-surface
  tombstone) and F-010 (bloom false-negative) fixes are real and sound. The one
  residual is a **latent** gap: the multipart/chunk R2 buckets
  (`corelink-chunk-*` / `corelink-manifest-*`) are NOT swept by the erase
  adapter — but that path is unwired in the container today (zero prod write
  sites), so nothing survives erasure in practice.
- **Is the residency claim honest?** **YES, now.** EU residency is REAL
  (prod-lhr binds `corelink-cas-eu`/`corelink-ac-eu`, `jurisdiction = "eu"`, EU
  S3 endpoint); US is the honest default; BR/`sam` + APAC are disclosed as
  roadmap with the explicit "Cloudflare R2 has no South-America region" caveat in
  every customer-facing doc. `verify-lgpd-residency.py` now does a REAL CF R2
  `location`-code probe and fails-loud — the tautology is gone.

## Severity tally

| Severity | Count | Items |
|----------|-------|-------|
| Critical | 0 | — |
| High     | 0 | — |
| Medium   | 1 | M-1 (chunk-bucket erase gap — latent) |
| Low      | 3 | L-1 (lhr chunk/manifest still US), L-2 (F-015 absent-header Allow), L-3 (AC reads ungated — defused by F-003) |
| Info     | 2 | I-1 (attestation sound), I-2 (no AC re-PUT tombstone, defused) |

No launch-blocking finding. The erasure + EU-residency hardening that the pentest
doc claims is, on inspection of `main`, genuinely shipped and correct-by-construction.

---

## Verified-CORRECT (the load-bearing fixes hold)

### F-003 — AC full-account erasure is now complete-by-construction ✅
`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:107-167` LISTs
`<region>/<tenant_prefix>/` across all five `corelink-ac-{sam,iad,lhr,nrt,syd}`
buckets and DELETEs every object (idempotent), and `verification_hash`
(`:207-234`) counts **actual R2 objects** (not the dead `ac_meta` index). No-TDK ⇒
`Transport` error ⇒ fail-CLOSED (`:190-194`). The old `ac_meta`-driven
`NotApplicable` short-circuit is gone. Verified: `ac_meta` still has **0
write-sites** in `crates/` (only the GC `pass_ac_meta` read-side), so the index-
independent LIST is the only correct enumeration — and that is what runs.

### F-004 — the 410 tombstone gate is centralized at the shared CAS seam ✅
`crates/corelink-container/src/routes.rs:451-470` wraps the shared
`cas_read`/`cas_write`/`cas_delete` trait objects in
`cas_erase::TombstoneGatedCasHandler` at the SAME chokepoint as
`AccountingCasHandler`, and `routes.rs:514-527` distributes **that gated handler**
to cargo, brew, npm, pip, oci, and the Bazel bridge. So every read surface
inherits the gate: a tombstoned read → NotFound (404, never serves erased bytes),
a re-PUT of a tombstoned hash → `TOMBSTONE_GONE_SENTINEL` → 410 (no resurrection),
a gate transport fault → 503 fail-CLOSED (`cas_erase.rs:891-937,983-1074`). The
native route keeps its own inline 410 for precise UX. This is the
complete-by-construction fix the doc claims — confirmed by the wiring, not just
the comment.

### F-010 — the bloom false-negative window is closed ✅
`cas_erase.rs:797-825` `reload_tenant_bloom` now SEEDS the fresh bloom from
`self.inner.list_tenant_tombstones(tenant)` (the authoritative D1 set) on every
(re)load, so a tombstone written by another instance OR by the separate
erase-route `D1TombstoneStore` is reflected within ≤ one refresh window. The
`is_tombstoned` fast path (`:830-850`) only returns `Ok(false)` when
`!just_reloaded && !contains` against a bloom that was seeded from D1. The prior
"re-seed from in-process writes only" defect is gone.

### Ed25519 attestation — cannot sign "complete" while data survives ✅
`crates/corelink-privacy-erasure-worker/src/orchestrator.rs:436-521` is the gate:
`VerifiedComplete` is emitted **iff `failed_count == 0`**, and `failed_count` is
incremented for ANY of the 12 canonical backends whose (a) prior tombstone is
missing, (b) `verification_hash` errors (`[0xff;32]`, fail), or (c)
`verification_hash != CANONICAL_EMPTY_TENANT_HASH` (`:502-505`). The R2Cas/R2Ac
`verification_hash` return the canonical-empty sentinel ONLY when a live R2 count
== 0. `attestation.rs:104-108` signs **only** on the `VerifiedComplete` arm. So
surviving CAS/AC bytes ⇒ non-canonical hash ⇒ `failed_count > 0` ⇒
`VerifiedPartial` ⇒ no signature. No-TDK ⇒ `verification_hash` Err ⇒ never
Complete. Attestation integrity holds.

### DSR migration 0069 + queue/cron ✅
`migrations/d1/0069_dsr_requested.sql` adds the durable enqueue anchor
(`dsr_id` PK, `status` CHECK `requested|verified`, SLA index) so a DSR that fails
before any tombstone is still enumerable (closes the "undetected SLA breach"
gap). `apps/signup-worker/src/webhooks/dsr_verify_cron.ts:159-254` enumerates
`dsr_requested` (+ the log as a fallback), calls `postVerify`, and flips to
`verified` **only on `verified_complete`** (`:237`) — partial/breached stay
enumerable for the next sweep. Cron is wired hourly
(`apps/signup-worker/wrangler.toml:127` `crons = ["0 * * * *"]` →
`index.ts:89-99 scheduled()`). The `dsr_requested` table is in the RETAIN-set
(erasure record survives erasure — ADR-S11-013).

### Residency — EU is REAL; verify-lgpd is honest ✅
- **prod-lhr (EU):** `wrangler.toml:754-803` sets `R2_S3_ENDPOINT` to the EU
  endpoint (`.eu.r2.cloudflarestorage.com`), `R2_CAS_BUCKET = "corelink-cas-eu"`,
  `R2_AC_BUCKET = "corelink-ac-eu"`, and binds `CAS_BUCKET`/`AC_BUCKET_LHR` to
  the `jurisdiction = "eu"` buckets. The container reads the bucket via
  `cas.rs:414 env_or("R2_CAS_BUCKET","corelink-cas-prod")` and the endpoint via
  `StorageEnv` (`storage.rs:98-106`), so **EU CAS+AC bytes physically land in
  EEUR** — not a key-prefix illusion. The container-side `residency_guard`
  (`routes/residency.rs`) is a sound backstop (weur→lhr; cross-region/unmappable
  → 409, fail-closed; zero handler I/O on reject).
- **verify-lgpd-residency.py (F-021):** `physical_region_of` (`:292-338`) now
  does a REAL `GET /accounts/{acct}/r2/buckets/{bucket}` probe, maps the
  `location` code via `R2_LOCATION_TO_MACRO` (`:94-115`), and returns `None`
  (⇒ VIOLATION, never a pass) on any unverifiable signal (`verify():389-398`).
  The self-comparing key-prefix tautology is removed. Fail-loud posture is
  unit-tested (`_self_test():403-455`).
- **Published claims:** `marketing/sales/FAQ-MASTER.md` (lines 115, 235, 243,
  283, 397), `CAIQ-V4-pre-filled.md:160` (DSP-16.1), and
  `SIG-LITE-2026-pre-filled.md:189,218,227` all state EU-live + US-default +
  BR/APAC-roadmap WITH the "R2 has no South-America region" caveat. No remaining
  false physical-residency claim for BR or APAC. The superseded
  LGPD-RESIDENCY-ATTESTATION-2026-05-15 is no longer the live assurance.

---

## Findings (residual)

### M-1 (medium, LATENT) — DSR CAS erase does NOT sweep the chunk/manifest R2 buckets
**File:** `crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:90-117,
169-179` · **Gap:** the R2Cas erase adapter deletes objects only from the single
CAS bucket (`R2_CAS_BUCKET`) and DELETEs the chunk **D1 tables** (`chunks`,
`manifest_chunks`, `multipart_sessions`, `blob_meta`, `:53,175-179`) — it never
LISTs/DELETEs from the separate `corelink-chunk-<region>` / `corelink-manifest-<region>`
R2 buckets. A real multipart implementation exists
(`crates/corelink-cas/src/multipart_schema/region.rs:70-89` resolves those exact
bucket names; `crates/corelink-r2-multipart`), and those buckets ARE bound to the
Worker (`wrangler.toml` `CHUNK_BUCKET_*` / `MANIFEST_BUCKET_*`).
**Why it's only LATENT (not a live erasure hole):** verified the container data
plane does NOT use the multipart path — `crates/corelink-container/Cargo.toml`
has **no** `corelink-r2-multipart`/`corelink-cas` multipart dep, the container
never reads `R2_CHUNK_BUCKET`/`R2_MANIFEST_BUCKET`, and there are zero
chunk-bucket write-sites in `crates/corelink-container/src/`. So today no tenant
bytes land in those buckets; the verification_hash (CAS-bucket-scoped) is
correct and the attestation stays honest. The adapter's own doc records the
assumption (`adapter_r2_cas.rs:10-14` "multipart path is NOT shipped … zero prod
write sites"). **Fix (before shipping multipart):** when the multipart/OCI-chunk
path is wired, extend `list_and_delete_cas` + `count_cas_remaining` to also sweep
`corelink-chunk-<region>`/`corelink-manifest-<region>` under the tenant prefix,
or add dedicated R2Chunk/R2Manifest erase adapters to the canonical 12 — otherwise
chunked content would survive a "complete" erasure AND the attestation would
falsely sign Complete (the verification_hash never counts those buckets). Gate the
multipart-ship PR on this.

### L-1 (low, LATENT) — prod-lhr chunk/manifest buckets are still US (ENAM), not EU
**File:** `wrangler.toml:766-767` (`R2_CHUNK_BUCKET = "corelink-chunk-lhr"`,
no jurisdiction) + `:813-843` (CHUNK_BUCKET_LHR/MANIFEST_BUCKET_LHR bind the
non-eu `corelink-chunk-lhr`/`corelink-manifest-lhr`, which per the F-013
investigation are physically ENAM). **Gap:** if the EU env ever chunks an EU
tenant's content, those chunk/manifest bytes would land in a US bucket — an
EU-bytes-in-US leak. **Why LATENT:** same as M-1 — the container does not write
the chunk plane today, so no EU bytes reach these buckets. **Fix:** when
provisioning EU multipart, create `corelink-chunk-eu`/`corelink-manifest-eu`
(jurisdiction=eu) and point the prod-lhr `R2_CHUNK_BUCKET`/bindings at them —
together with M-1. Until then, do not enable multipart on prod-lhr.

### L-2 (low, LATENT) — residency guard returns Allow on an absent header (F-015 unchanged)
**File:** `crates/corelink-container/src/routes/residency.rs:92-95` —
`residency_decision(None, _) => Allow`. A direct hit to a public regional host
(`lhr/sam/nrt/syd.corelink-api.humangr.com`) carries no
`x-corelink-primary-region`, so the guard is a no-op on that path. **Impact
today:** none material — for `lhr` an absent-header write still lands in the
correct EU bucket (the env binds corelink-cas-eu); for `sam` it lands in the US
CAS bucket, which is the honestly-disclosed reality for BR. The guard cannot
*cause* a US leak for EU because the lhr env's storage IS the EU bucket
regardless of the header. **Residual risk (future):** if an EU env is ever bound
to a non-EU bucket again, the absent-header Allow removes the backstop. **Fix
(defense-in-depth):** have the regional Worker/container DERIVE+enforce its own
region from `R2_CAS_REGION` instead of trusting absent==Allow, and/or restrict
the regional hosts to Service-Binding-only (no public custom_domain). Matches the
F-015 fix recommendation; non-blocking while EU storage == EU bucket.

### L-3 (low, mostly DEFUSED) — AC reads (`ac.rs handle_lookup`) have no erasure tombstone gate
**File:** `crates/corelink-container/src/routes/ac.rs:484` (`handle_lookup`) —
grep `tombstone|410|Gone|erase` in `ac.rs` = 0. The F-004 fix gated the CAS seam,
not the AC lookup. **Why mostly defused:** with F-003 fixed, a full-account
erasure now DELETES the AC R2 envelopes, so a post-erasure AC lookup 404s (bytes
gone) rather than serving erased content — the CK-1 "read data attested complete
but never deleted" chain is broken at the delete step. The residual is narrow: AC
has no per-digest tombstone, so a re-PUT of an AC envelope after a per-digest
erase (if such a flow existed) would not be refused the way CAS re-PUTs now are.
Today AC erasure is account-scoped (LIST+DELETE), not per-digest, so there is no
live exposure. **Fix (completeness, low priority):** thread an AC tombstone gate
(or route AC writes through a gated seam) so AC matches CAS's re-PUT refusal, if
per-digest AC erase is ever added.

---

## Notes / cross-checks

- The R2Cas/R2Ac adapters both fail-CLOSED with no TDK (cannot derive the prefix
  ⇒ cannot address ⇒ never claim success), and both count REAL R2 objects in
  `verification_hash` — so the attestation can never be signed on an unverifiable
  or undeleted state.
- The `verify-lgpd` script maps NO R2 location code to `sam` (R2 has no SA
  region), so a BR-residency tenant's objects (physically ENAM) map to `enam ≠
  sam` → VIOLATION → fail-loud. That is the HONEST outcome: the script can never
  green-light a BR physical-residency claim, which is exactly why BR is disclosed
  as roadmap. No false green.
- The 12-backend `canonical_backend_kinds()` set (`event.rs:172-178`) is fixed at
  compile time; a missing adapter for any kind errors the whole verify
  (`orchestrator.rs:442-444`) — no silent skip.
