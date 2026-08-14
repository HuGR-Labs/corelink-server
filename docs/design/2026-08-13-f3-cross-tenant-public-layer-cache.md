# F3.2 — Cross-tenant public layer cache (the network-effect jaw-drop)

**Status:** REVIEWED 2026-08-13 → DO NOT BUILD AS SCOPED → **RE-SCOPED 2026-08-13:
BUILD as the pull-through mirror model (gated mini-campaign).** Adversarial review (2
independent lenses) found a wrong value premise + a critical un-erasability landmine
in the *cache-write-hook* framing; the re-scope at the bottom pivots to the registry
pull-through mirror that BLOCKER-2 itself points to, which fixes the value premise (BLOCKER-2) and closes client-side
poisoning by watching the PULL path; a 2nd review (below) corrects the
"dissolves all four" over-claim — B1+B4 remain prerequisites. See "Audit review" then "RE-SCOPE" below. The
original design + review follow unchanged for the record; the RE-SCOPE supersedes
both recommendations.
**Author:** engineering (Claude), 2026-08-13.
**Precedes:** any implementation PR. This doc exists to be reviewed, not merged as fact.

## Audit review (2026-08-13) — findings, worst-first

Two independent adversarial reviewers attacked this design against the real code
seams. Verdict: **the two "blocking controls" are stated but the three hardest
seams are open questions or one-liners, and the headline value targets a blob flow
that never passes the hook.** Do not implement until every BLOCKER below is closed.

- **BLOCKER-1 (CRITICAL) — `_public` is physically un-erasable; misclassification =
  permanent breach with a FALSE "erased" audit record.** The erase seam keys off the
  DSR tenant's HMAC prefix (`cas_erase.rs:1377,1407`); `_public` lives under the
  separate sentinel-UUID prefix (`r2_s3.rs:869,877`) that no tenant erase ever
  reaches. A Control-1 miss that routes a private blob to `_public` → the tenant's
  DSR deletes nothing, upserts a 410 tombstone (read path 410s, audit says erased),
  but the bytes persist and keep serving every other tenant. There is **zero** code
  path to delete or revoke a `_public` blob. `_public` must gain an erase/revocation
  path BEFORE it can hold anything.
- **BLOCKER-2 (HIGH) — the value premise is largely wrong ("born warm" over-claim).**
  BuildKit fetches `FROM ubuntu:24.04` base layers via the **image-pull / registry**
  path; CoreLink's cache surface stores build-cache manifests + `RUN`-step outputs
  that reference base layers by **digest pointer**, not by re-uploading base bytes.
  So allowlisted base blobs rarely transit the cache-write hook (step 2) — the
  storage-dedup mostly never fires, and importing a base into the CAS does NOT make
  BuildKit skip re-pulling it. To actually capture the base-layer network effect,
  CoreLink would have to be a **registry pull-through mirror** for base images (a
  different, larger feature the design explicitly excludes) — NOT a build-cache
  backend hook. Confirm empirically that base bytes transit the hook at all before
  building anything on this premise.
- **BLOCKER-3 (HIGH) — allowlist-by-tag IS the poisoning vector; no revocation.**
  Resolving mutable tags (`ubuntu:24.04`) to "whatever digest they point at now" and
  auto-allowlisting reopens exactly the hole content-addressing closes: a compromised
  upstream's malicious digest gets promoted to `_public` cross-tenant. Trust root must
  be **digest-pinned + owner-gated**, never tag-resolved; and combined with BLOCKER-1
  a poisoned entry is un-revocable.
- **BLOCKER-4 (HIGH) — byte-accounting cannot express "write but don't charge."**
  `AccountingCasHandler::write` charges `req.tenant` unconditionally
  (`byte_accounting.rs:776,816`); there is no shared/unowned mode. Charging `_public`
  → unseeded row → 503 fail-closed, or a finite shared cap → cache-fill DoS across ALL
  tenants; routing after accounting → first-writer pays + free-riders + phantom
  release on their GC/erase (`_public` has no owner). Needs a defined shared-blob
  ownership/accounting model first.

Non-blocking but must fold into the threat model:

- **GAP-A — read-path re-hash does not cover the OCI surface.** The self-healing
  re-hash-on-read (`adapter_cache.rs:254-265`) runs only on `MoatCache.get`
  (brew/pip/npm). OCI reads go through `BlobStore` directly with **no** re-hash;
  F3.2 routing OCI blobs to `_public` would ship the shared surface with strictly
  weaker integrity than the paths it is modeled on. Port the re-hash onto the OCI
  `_public` read path — mandatory.
- **GAP-B — routing-by-digest must be ordered strictly AFTER content verification.**
  The client supplies the claimed digest; if `_public` routing happens before the
  CAS content==hash check, a client writes arbitrary bytes under an allowlisted
  digest = the poisoning the design claims to prevent. State the ordering explicitly.
- **GAP-C — the "no mutable `_public` share exists" claim is already false.** npm
  package **metadata** JSON is served cross-tenant under `_public`
  (`routes/npm.rs:101-106`), mutable + not content-addressed (server-fetched from
  upstream, so not client-injectable, but it IS the "shared mutable manifest" surface
  threat #3 lists as out-of-scope). Acknowledge it.
- **GAP-D — "public upstream" ≠ "no personal data."** A public base can legitimately
  embed personal data/secrets; combined with BLOCKER-1 that becomes permanently
  un-erasable and outside DSR reach. `_public`'s GDPR-out-of-scope assumption is
  unjustified.
- **GAP-E — allowlist/populate ordering: it MUST be populate-then-allowlist, never
  the reverse (TOCTOU).** (3rd-lens review, verified.) If a digest is added to the
  allowlist BEFORE the server's own mirror has populated `_public` for it, a window
  opens where the FIRST writer of that `_public` blob is a *client* pushing the
  allowlisted digest — so the client, not the server, populates the shared blob.
  Content-addressing keeps the bytes correct (H(B)=D pins B), but it destroys the
  provenance invariant Control 1 rests on ("`_public` was written by the server from
  a trusted upstream"), and any verify-vs-place ordering gap (GAP-B) then serves
  transient unverified bytes cross-tenant. Constraint: the allowlist entry for a
  digest goes live ONLY after the server has itself populated `_public[digest]` from
  the trusted upstream; a client push of an allowlisted-but-unresident digest must
  dedup-or-reject, never populate. (Distinct from GAP-B, which is verify-vs-route
  ordering within one write; this is allowlist-vs-populate ordering across the
  control plane.)
- **GAP-F — dedup timing side-channel (LOW impact, but a real design tension).**
  (3rd-lens review, verified.) A cross-tenant dedup store leaks residency by
  timing: a tenant times its own push — fast = the blob is already in `_public`
  (deduped), slow = full upload — and thereby probes which digests are resident.
  It is passive and bypasses both controls (it only reads dedup state, never
  writes, never needs misclassification). **Bounded impact:** `_public` holds only
  PUBLIC base layers whose digests are already public and are typically always
  resident, so the reconnaissance value is low (it does NOT reach private layers —
  those stay tenant-prefixed, never in `_public`). The tension: the only clean fix
  is constant-time dedup (always run the full upload flow), which **defeats the very
  speed win** F3.2 exists for — so a residency side-channel is largely inherent to
  cross-tenant dedup. Record it and accept-or-mitigate explicitly at re-scope; do
  not silently ship a timing oracle.

**Revised recommendation:** F3.1 (private) stays shipped. F3.2 is **not buildable as
scoped** — it needs (a) a `_public` erase/revocation path, (b) a digest-pinned +
owner-gated allowlist trust root, (c) a shared-blob accounting/ownership model, and
(d) empirical confirmation that base-layer bytes transit the cache-write hook at all
(BLOCKER-2 suggests they do not — in which case the whole approach should pivot to a
registry pull-through mirror, or be dropped). Re-scope before any implementation.

## Why this doc is gated

F3.1 (private, per-tenant BuildKit layer cache) is LIVE and proven — a customer's
build layers persist and reuse across their own CI runs, isolated by the per-tenant
HMAC blob prefix (`r2_s3.rs:871 tenant_prefix`). That win required **no new code**.

F3.2 is different: it deliberately makes some content **cross-tenant shared**. It
touches the single most security-critical invariant in the system — tenant
isolation — which has a long adversarial red-team history. It MUST NOT ship on
"faz tudo" momentum. This doc states the value, the threat model, and the two
controls precisely so the owner (and ideally the audit team) can sign off on the
shape before a line is written.

## The value (what "jaw-drop" actually means, honestly)

A multi-tenant content-addressed cache has a network effect: the more tenants use
it, the fuller it is, the faster+cheaper builds are for everyone — IF public
content is stored once and shared.

The realistic shareable surface is **public base image layers**, not
customer-derived layers. When tenant A and tenant B both `FROM ubuntu:24.04`, the
base layers have byte-identical content (same digests). Today, per-tenant HMAC
prefixing stores those identical bytes **twice** (once per tenant). Sharing them:

- **Storage COGS:** one copy of every popular public base, not one-per-tenant.
- **Cold-build speed:** a brand-new tenant's first build imports the public base
  layers already resident — "born warm."

Customer-derived layers (the output of *their* `RUN` steps) stay **private**. The
network effect is real but bounded to public inputs — claiming more would be a lie.

## Threat model (why content-addressing alone is NOT enough)

1. **First-writer poisoning of the url→hash map.** The moat has two levels
   (`adapter_cache.rs`): level-1 blobs are keyed by content-hash (forging bytes
   for a target digest is infeasible — safe). Level-2 is a *mutable* map
   `(namespace, url_hash) → content_hash`. If a malicious tenant can write a
   `_public` map entry, they repoint a well-known key at their own (validly
   hashed but malicious) blob. Every other tenant then resolves the poisoned
   content. **This is the real attack — the mapping, not the bytes.**
2. **Cross-tenant private leak.** If classification is wrong and a
   customer-derived (private) layer lands in `_public`, another tenant reads it.
   A build layer can contain secrets baked into the image, proprietary source,
   etc. One misclassification = a data breach.
3. **Cache-manifest poisoning.** A BuildKit cache manifest references blobs by
   digest; if the manifest itself is shared and writable, it is a poisoning
   surface equivalent to (1).

Today `PUBLIC_NAMESPACE` (`adapter_cache.rs:35`) is written **only by the server's
read-through mirror** (Homebrew/npm/PyPI pull-through from a trusted upstream) —
**no client PUT ever targets `_public`.** That property is what keeps it safe now,
and it is exactly the property F3.2 must preserve.

## The two blocking controls

**Control 1 — default-private classification (server-decided, never client-asserted).**
A layer/blob is eligible for `_public` ONLY if the SERVER can prove every input is
public. Concretely, the only automatically-provable case is: the blob was fetched
by the server's own pull-through from an allowlisted public registry, OR its digest
is on a server-maintained allowlist of known public base-image layer digests. The
client may never flag its own content as public. Default is private (tenant prefix).

**Control 2 — public writes gated to allowlisted base digests (poisoning defense).**
No client-driven write to a `_public` level-2 map entry, ever. `_public` entries are
authored ONLY by the server, ONLY for digests that pass Control 1. A client push of
a blob whose digest matches an allowlisted public base is deduped to the existing
`_public` blob (storage win) but cannot *create* or *repoint* a `_public` mapping.

## Realistic implementation scope (smallest safe surface)

1. A server-owned **allowlist of public base-image layer digests** (seeded from the
   popular public bases: ubuntu, alpine, debian, node, python, etc.), refreshed by
   a server job pulling those bases through the existing trusted mirror path.
2. On blob **write**: if the incoming digest ∈ allowlist, store/dedup under
   `_public` (shared); else under the tenant HMAC prefix (private). Read checks
   `_public` for allowlisted digests, then the tenant prefix.
3. **No new client-facing API.** BuildKit keeps pushing to `.../cache/<repo>`; the
   server transparently routes allowlisted public base blobs to the shared store.
4. Byte-accounting (`byte_accounting.rs:776`): public-base bytes are NOT charged to
   the tenant (they are shared infra), private bytes charge as today.

## Explicitly OUT of scope for F3.2

- Sharing customer-derived layers (private by origin — never).
- Client assertion of publicness (banned — Control 1).
- Cross-tenant sharing of mutable cache manifests (poisoning surface — server-only).

## Open questions for owner / audit review

1. Allowlist governance: who curates it, how is a base added, how is a compromised
   upstream base revoked from `_public`?
2. GDPR/erase interaction: `_public` uses a fixed sentinel UUID — does the erase
   seam (`cas_erase.rs`) need to explicitly refuse to erase `_public` (shared,
   not personal data) vs a tenant prefix?
3. Is the storage-COGS + cold-start win worth the added blast-radius surface at
   launch scale, or is it a post-launch optimization once tenant count justifies it?

## Recommendation

Ship F3.1 (private — done). Treat F3.2 as a security-reviewed feature: this design
→ audit-team red-team of the classification + poisoning controls → implementation
behind a flag → cross-tenant isolation test in the story suite → gated rollout.
Do NOT fast-path it.

# RE-SCOPE 2026-08-13 — the pull-through mirror model (supersedes the "cache-write hook" framing)

**Status:** RE-SCOPE STUDY. Flips the shape from "classify cache writes" to "serve a
pull-through mirror." Still gated on owner + audit sign-off before code. Author: Claude.

## Why re-scope

The original framing above tried to make the shared/private decision on the BuildKit
**cache-write** path (a client pushes `.../cache/<repo>`; the server classifies each
blob). That framing carries four hard blockers (surfaced in the multi-engine review,
PRs #1103–1106): base layers do NOT arrive via that hook (they arrive via the
registry PULL of `FROM`), the `_public` map is un-erasable off the tenant prefix,
byte-accounting can't "write-but-not-charge," and allowlist-by-tag is poisonable.
The **blocker-2 root cause** is structural: *we were watching the wrong path.*

**The mirror model watches the right path.** The shareable content — public base
image layers — enters the system when BuildKit **pulls** `FROM ubuntu:24.04`, not
when it pushes a cache. CoreLink already runs trusted server-side pull-through
mirrors for exactly this shape (npm/PyPI/Homebrew/cargo write `_public`;
`adapter_cache.rs:35 PUBLIC_NAMESPACE`), and the OCI surface **already anticipates
this exact follow-up**: `routes/oci.rs:188-192` — *"Cross-tenant public-image dedup
(storing public base images under PUBLIC_NAMESPACE) is an explicit follow-up"* — with
the upstream **fetch + digest-verify already implemented** (`oci.rs:518`, reject a
digest mismatch BEFORE `moat.put`). So F3.2-via-mirror is a small extension of an
existing, security-reviewed seam — not a new subsystem, and not a new client API.

## The model in one paragraph

Add a **container-registry pull-through mirror** for public base images. When any
tenant's build does `FROM <allowlisted-public-base>`, BuildKit pulls it through
CoreLink's OCI surface; the server fetches from the trusted upstream (docker.io /
registry-1), **verifies each layer digest against the upstream** (already done at
`oci.rs:518`), and stores it **once** under `PUBLIC_NAMESPACE`, keyed by its OCI
digest. Every later tenant that pulls the same base is served from the resident
`_public` blob — "born warm" — and it is stored once (COGS win). Customer-derived
layers (their `RUN` outputs) never enter this path; they stay private under the
tenant HMAC prefix (F3.1), unchanged.

## How the mirror dissolves each blocker

- **BLOCKER-2 (value premise — base layers bypass the cache-write hook):** DISSOLVED.
  The mirror sits on the PULL path, which is exactly where base layers travel. We no
  longer need a cache-write classifier at all.
- **BLOCKER-1 / GAP-A (`_public` un-erasable; re-hash only on `MoatCache.get`, not
  OCI):** NARROWED. `_public` holds ONLY public upstream base layers (no personal
  data), so the erase seam should *refuse* to erase `_public` by design (open Q2
  above) — the un-erasability becomes correct behavior, not a bug. The OCI read path
  must still content-verify on serve; wire the same re-hash-on-read the native
  `MoatCache.get` has (GAP-A) into `OciMoatStore` reads from `_public`.
- **BLOCKER-3 / GAP-E (first-writer / allowlist-by-tag poisoning, populate-then-
  allowlist TOCTOU):** DISSOLVED at the source. No client write ever authors a
  `_public` entry — only the server's mirror does, and ONLY for bytes it fetched from
  the trusted upstream and digest-verified. Allowlist is **by immutable digest**, not
  by tag (a tag→digest resolution is pinned server-side at mirror time, closing the
  tag-poisoning + TOCTOU windows).
- **BLOCKER-4 (byte-accounting can't write-but-not-charge):** SIDESTEPPED. `_public`
  base bytes are written by the **server mirror**, not on a tenant's metered write
  path, so there is nothing to "not charge" — the tenant's private writes account
  exactly as today (`byte_accounting.rs`).
- **GAP-B (verify-before-route), GAP-F (dedup timing side-channel):** verify-before-
  route is already the upstream-digest check; the dedup side-channel shrinks because
  membership in `_public` is a fixed server-curated base-digest set, not a function of
  another tenant's recent activity.

## Honest residual risks (what the audit MUST still red-team)

1. **Upstream compromise / revocation.** If a poisoned base ships from docker.io and
   we mirror it, every tenant gets it. Same trust we already extend to npm/PyPI
   mirrors — but base images are higher-value. Needs a revocation path (evict a digest
   from `_public` + allowlist) and a documented upstream-trust boundary.
2. **Allowlist governance (open Q1).** Who curates the base-digest allowlist, how a
   base is added, how a compromised digest is revoked. Must be server-side, auditable.
3. **`_public` read isolation.** A read for an allowlisted digest must serve `_public`;
   a read for ANY non-allowlisted digest must NEVER fall through to another tenant's
   prefix. The story-suite cross-tenant isolation test is mandatory before rollout.
4. **The "born-warm speed" claim is bounded.** COGS-dedup is unconditional. The
   cold-build *speed* win materializes only for the base-PULL, not for BuildKit
   cache-manifest import (manifests stay per-tenant, never shared — blocker-3 stays
   respected). Claim exactly that and no more.

## Smallest safe implementation (if owner says build)

1. Server-curated **allowlist of public base-image layer digests** (seed: ubuntu,
   alpine, debian, node, python, golang), refreshed by a server job that pulls those
   bases through the trusted mirror and pins tag→digest.
2. In `OciMoatStore` (`oci.rs:212`): on persist, if the OCI digest ∈ allowlist →
   `moat.put(PUBLIC_NAMESPACE, digest, …)`; else tenant prefix (today's behavior).
   On read: check `_public` for allowlisted digests, then tenant prefix. Wire
   re-hash-on-read (GAP-A) for `_public` serves.
3. Erase seam (`cas_erase.rs`): explicitly refuse `_public` (shared infra, not
   personal data) — closes open Q2.
4. Behind a flag; cross-tenant isolation story test green BEFORE any prod rollout.
5. No new client-facing API (property preserved from the original scope).

## Re-scope recommendation

**BUILD — but as its own gated mini-campaign, not a fast-path.** The mirror model is
the honest, structurally-safe route to the network-effect win, it reuses a seam the
code already flags as the intended follow-up, and it dissolves the four blockers that
killed the cache-write framing. It is NOT free: it extends our upstream-trust boundary
to base images and demands the allowlist-governance + revocation + isolation-test work
above. Sequence: this re-scope → audit-team red-team (upstream-trust + `_public` read
isolation) → flag-gated impl in `OciMoatStore` → story-suite cross-tenant isolation
gate → gated rollout. If the owner judges the launch-scale tenant count doesn't yet
justify the added blast radius (original open Q3), **defer, don't drop** — the seam is
marked and the private win (F3.1) already ships the customer-visible speedup (F3.3:
8.5×).

---

## Second adversarial review 2026-08-13 (DeepSeek + Kimi-K2, via gateway) — honesty correction

Two more independent engines attacked the mirror re-scope. **Split verdict:** DeepSeek
BUILD-WITH-CHANGES, Kimi-K2 DO-NOT-BUILD. They converge on one correction this doc must
own: **"dissolves the four blockers" was an over-claim.** Only **BLOCKER-2 is truly
dissolved** (the structural pull-vs-write fix). The other three are *relocated*, not
eliminated — and two of them are **build-blocking prerequisites, not audit TODOs**:

- **BLOCKER-3 → becomes an allowlist-governance control (not "dissolved").** Client-driven
  poisoning IS closed (only the server authors `_public`). But the trust root moves into the
  curation pipeline: the allowlist must be **digest-pinned** and the mirror-populate →
  allowlist-commit step must be **atomic** (else the GAP-E TOCTOU just moves server-side —
  a digest resident but not-yet-allowlisted, or allowlisted but not-yet-populated, reopens the
  window). Governance that accepts a *tag* anywhere reintroduces the vector.
- **BLOCKER-4 → cost externalization, PREREQUISITE.** "Server writes, nothing to not-charge"
  hides the COGS, it does not resolve it. `_public` bytes still cost money. Required BEFORE
  build: an explicit decision that the **platform absorbs `_public` storage as the network-effect
  subsidy**, PLUS an **unbounded-growth cap** (allowlist bloat / mirror-amplification must not
  let `_public` grow without bound — otherwise it is a cross-tenant cost-contagion DoS).
- **BLOCKER-1 → PREREQUISITE, not "narrowed-and-fine."** `_public` still has **no erase or
  revocation path**, and "holds only public data" is a classification *assertion* that fails
  under (a) upstream compromise or (b) any misclassification. A `_public` **revocation/erase
  path is a hard prerequisite** — needed for incident response (purge a poisoned upstream
  digest) independent of GDPR.

New operational risks both engines surfaced that this doc missed (verified as real):

- **Mirror-amplification DoS** — a cache miss triggers a server-side upstream fetch; an
  attacker who can force misses (un-allowlisted-digest flooding) amplifies tenant pulls into
  upstream fetches. Needs rate/cost throttling on the mirror path.
- **Silent allowlist desync** — if the tag→digest refresh job stalls (upstream down), tenants
  serve against stale pinned digests with no fail-closed / health signal. Needs a staleness gate.
- **Upstream rollback / orphaned pin** (Kimi N-1) — a pinned digest can be orphaned from
  upstream's later intent; folds into the same revocation-path requirement as BLOCKER-1.

One raised risk is **bounded, not independent:** the manifest/blob confused-deputy (Kimi N-3)
is harmless *while the B1 public-only invariant holds* — every `_public` blob is public by
construction, so cross-referencing one is not a leak. It becomes real only under a B1
misclassification, which is exactly why B1's revocation path is a prerequisite.

**Net (honest) verdict:** the mirror is still the **right direction** — B2 is genuinely fixed
and client-poisoning is genuinely closed — but the accurate framing is **"1 blocker dissolved,
1 becomes a governance control, 2 (B1 revocation, B4 shared-accounting+cap) are hard
prerequisites,"** NOT "4 dissolved." Recommendation is therefore **BUILD only after B1
(revocation/erase for `_public`) + B4 (platform-absorbs + growth cap) are designed and the
allowlist-commit atomicity + mirror rate-limit + staleness-gate are specced** — else **defer**.
This supersedes the "dissolves the four blockers" wording above.

---

## Cost measurement + owner decision 2026-08-13

**B4 storage COGS is NOT a seed-money problem — measured, not guessed.** The shareable
`_public` surface is a *bounded, server-curated* set of popular public base images.
Measured compressed sizes (Docker Hub `full_size`, 15 popular bases: ubuntu/alpine/
debian/node/python/golang/rust/busybox across common tags) sum to **1.71 GB** with no
cross-image layer dedup. On R2 (`$0.015/GB-mo`, **egress $0**):

| scenario | R2 storage / month |
|---|---|
| real 15-base set | **$0.026** |
| 10× (hundreds of tags, zero dedup) | $0.26 |
| 100× paranoia (huge curated set) | $2.56 |

Upstream pulls are one-time per digest (then resident) and Docker Hub pulls are free;
R2 Class A/B ops at base-image serve volume are cents. The **growth cap** (B4) is what
keeps `_public` inside this range — it caps the curated allowlist, so cost cannot run
away even under allowlist-bloat / mirror-amplification pressure. Closes open Q3: the
infra cost is trivially affordable, even with zero seed money.

**The real gate is engineering time, not cash:** designing the B1 `_public`
revocation/erase path + the B4 growth cap + passing the cross-tenant isolation audit.

**Owner decision (2026-08-13):** F3.2 (the cross-tenant public / network-effect moat) is
**committed — "super important, we have to do it"** — but **deferred to the END of the
backlog**. Sequence when picked up: design B1 (revocation) + B4 (growth cap; platform
absorbs the measured ~cents/mo COGS as the network-effect subsidy) → audit red-team →
flag-gated `OciMoatStore` impl → story-suite cross-tenant isolation gate → gated rollout.
Until then, F3.1 (private) ships the customer-visible speedup (F3.3: 8.5×).
