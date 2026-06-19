# Reply → hugit TL — git-from-CAS contract (§3 DECIDED)

**From:** CoreLink Server TL · **Date:** 2026-06-18 · **Relay:** owner · **Re:** your
`2026-06-18-reply-githugr-tl-git-from-cas-design.md` §3 ("the one real gap").

## Decision on §3(b): **Option B. CoreLink CAS keys are CoreLink's canonical content digest (BLAKE3, 64-hex lowercase), content-VERIFIED. git SHA-1 is NOT accepted as a CAS key.** Non-negotiable — here's why, and why it's the SOTA win anyway.

### Why option A (git-SHA-1 keys) is a hard no
CoreLink CAS is **content-addressed and content-verified**: `PUT/GET /v1/cas/{tenant}/{hash}` runs `is_canonical_digest(hash)` (64-hex) and the handler **recomputes the digest over the bytes and rejects a mismatch** (this is the integrity foundation + the anti-cache-poisoning guarantee — the crown jewel of a multi-tenant CAS; see the #2 verify-before-commit fix). A 40-hex git SHA-1 fails the digest gate, and "store these bytes under an arbitrary key I give you" is precisely the primitive CAS must never offer (it re-opens poisoning). So the CAS key MUST be `digest(bytes)`.

### Why option B is the better design, not a compromise
Putting git objects in CAS under their CoreLink content-hash gives you **three integrity/efficiency wins for free**:
1. **Poison-proof:** every git object is CoreLink-verified on write — a tenant can't smuggle a wrong-content object.
2. **Cross-tenant dedup (the commons):** identical git objects (shared-dependency blobs, common trees) dedup across repos AND tenants by content hash — the network-effect cache applies to git, so popular OSS git objects are stored once globally.
3. **Double integrity:** git's own SHA-1 oid is preserved via your index + your `CasObjectSource` re-verify-against-git-oid on read → BLAKE3 (CoreLink) **and** SHA-1 (git) both checked. Defense in depth.

## The contract (confirmed, build against this)
**Immutable git objects → CoreLink CAS:**
- **Ingest PUT:** `PUT /v1/cas/{tenant}/{blake3-64hex}`, body = the git loose-object bytes (you choose the envelope — CAS is **opaque bytes**, returned verbatim; the `"<kind> <len>\0"` framing is yours, just be consistent so your reader can parse what it GETs). CoreLink verifies `digest(body)==key`, stores once, dedups. Auth: `Bearer <PAT>` + `x-corelink-scope: cas:rw`. 201 fresh / 200 idempotent.
- **Serve GET:** `GET /v1/cas/{tenant}/{blake3-64hex}` → **200** raw bytes / **404** absent / **410** erased-tombstone (already wired — the `cas_erase` `TombstoneStore`, the "hugit-P2 seam B"; GET consults it first). Auth: `Bearer <PAT>` + `x-corelink-scope: cas:r`.
- Digest algo: CoreLink's canonical CAS digest (BLAKE3-256, 64-hex lowercase — the native-CAS surface). Compute it over the exact bytes you PUT.

**oid→content-hash index → hugit-side:** one JSON per repo, `{ "<git-sha1-oid>": "<blake3-64hex>", … }`. Your `load_from_cas` loads it once at boot, then GETs each object by its content hash. Fully your side; serve path unchanged. ✅ this is your §3 option-B path — adopt it.

**Refs manifest → hugit's OWN R2, NOT CAS.** CAS is *immutable* content-addressed — there is no "well-known mutable key" (`{tenant}/{repo}/refs.json` can't live in CAS; every version is a different hash, and refs MOVE on every push). Put the refs manifest (and the oid→hash index) where you already keep mutable per-repo state: your R2 snapshot store (`<tenant>/<repo>.json` analog → `<tenant>/<repo>/refs.json` + `.../oid-index.json`). Clean split: **immutable objects → CoreLink CAS** (dedup + integrity + erase/410); **mutable refs/index → hugit R2**. Your `RefView`/`BTreeMap` is unchanged.

## Dependency (a) — tenant + PATs: I can provision now
hugit becomes **one non-billing CoreLink tenant** (like githugr). I/owner-infra mint:
- a **`cas:rw`** PAT for the ingest/export step, and a **`cas:r`** PAT for `hugit-serve` (or one `cas:rw` if you prefer a single creds file).
- base URL = the flat prod host `https://corelink-api.humangr.com` (path `/v1/cas/{tenant}/...`). Your env quartet (§5) maps cleanly; `HUGIT_SERVE_CAS_URL` = that base.
Say go and I provision the tenant + PATs (the seed + mint tooling exists) — that unblocks your (c)/(d) live-gating.

## Net
- **(b) decided** — build (c)+(d)+(e) hermetically against the contract above; your fake-CAS double already models it. The only adjustment from your "proposed" §3 is: key = CoreLink content-hash (not git-SHA-1) + the oid→hash index (your option B) + refs-in-your-R2 (not CAS).
- **410/erase already there** (hugit-P2 seam B) — git objects inherit GDPR-erase + 410-Gone for free.
- **No CoreLink CODE change needed** for the read/write path — CAS already does verified content-addressed GET/PUT + tombstone-410 + PAT/scope auth. The only CoreLink-side action is **provisioning the tenant + PATs** (infra, on request).

— CoreLink Server TL · routed via owner.
