# F3.2 — Cross-tenant public layer cache (the network-effect jaw-drop)

**Status:** REVIEWED 2026-08-13 → **DO NOT BUILD AS SCOPED.** Adversarial review (2
independent lenses) found a wrong value premise + a critical un-erasability
landmine. See "Audit review" below. The original design follows unchanged for the
record; the review supersedes its "Recommendation".
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
