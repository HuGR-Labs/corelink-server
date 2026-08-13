# F3.2 — Cross-tenant public layer cache (the network-effect jaw-drop)

**Status:** DESIGN — owner + security review REQUIRED before any code lands.
**Author:** engineering (Claude), 2026-08-13.
**Precedes:** any implementation PR. This doc exists to be reviewed, not merged as fact.

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
