---
title: "OKF-CoreLink Concept Template & Worked Example"
type: "Contract"
description: "The fillable per-concept stub agents transcribe, plus one fully worked example showing every frozen rule satisfied."
status: "FROZEN — cold-critic FREEZE-OK (2026-06-26). Campaign 2 complete."
tags: ["okf", "contract", "template"]
---

# Concept template & worked example

Companion to `01-okf-corelink-profile.contract.md`. An agent authoring a concept **copies the stub,
fills the marked `<…>` slots, and deletes nothing structural**. Every rule here is enforced by
`validate_okf.py` — an agent never needs to judge; it transcribes and the gate verifies.

---

## A. The stub (copy this into `docs/knowledge/<dir>/<concept-id>.md`)

```markdown
---
type: "<one of the taxonomy types — §1.1 of the profile>"
title: "<human title, non-empty>"
description: "<one sentence>"
source_files:
  - "<repo-relative path that exists>"
  - "<…add every file this concept is grounded in>"
checkpoint_sha: "<40-hex SHA the concept was reconciled against>"
provenance: "AUTHORED"   # or GENERATED (then add the marker line below)
tags: ["<area>", "<…>"]
timestamp: "<ISO-8601>"
---

# <Title>

<Lead paragraph: the WHY — the role this thing plays in the system, the cross-crate context, the
thing rustdoc cannot tell you. 2–5 sentences. No fluff.>

# Role
<What problem it solves / where it sits in the plane topology.>

# How it works
<Mechanics, in structural markdown. Reference the cited lines inline as `path:line`.>

# Invariants
<The hard rules that must not break. Tie each to a code anchor.>

# Gotchas
<The non-obvious traps — the tribal knowledge. Optional but high-value.>

# Citations
1. `<path:line>` — <what this anchor proves>
2. `<path:line-line>` — <…>
   (every `source_files` path MUST appear in at least one citation)
```

For a `deferred` candidate (placeholder, not yet written):

```markdown
---
type: "<taxonomy type>"
title: "<title>"
deferred: "<reason — e.g. 'blocked on W-E7; ADR-0099 not yet re-indexed'>"
---
```

---

## B. Worked example (the SHAPE — real paths, real SHA, placeholder lines)

> **This shows the frozen shape, not an executable passing artifact.** Paths and `checkpoint_sha` are
> REAL and resolve; line numbers are written as `:<L>`/`:<L1-L2>` placeholders the author resolves
> against the real file at HEAD (a real concept replaces every `<L>` with a resolving line — the
> validator's C6 rejects `<L>`). The canonical *executable* passing artifact lives in the golden
> fixture `tests/okf/fixtures/good/` that W-B builds and the validator self-tests against.
> Note every `# How it works` / `# Invariants` bullet carries a `path:line` token (C6c), every cited
> file is in `source_files` (C6b), and full repo-relative paths are used (C3).

```markdown
---
type: "AuthMechanism"
title: "The 2-level PAT moat"
description: "How CoreLink rejects forged tokens cheaply at the edge and proves possession deeply in the container."
source_files:
  - "worker/src/lib/internal_auth.ts"
  - "crates/corelink-container/src/adapter_pat.rs"
checkpoint_sha: "6d670131b94ffd6c4ff197a05747c5d42b3a77f2"
provenance: "AUTHORED"
tags: ["auth", "pat", "security", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# The 2-level PAT moat

CoreLink authenticates every cache request with a Personal Access Token, but it never pays the full
verification cost on a forged token. The moat is two layers: a constant-time HMAC fast-reject at the
Worker edge (cheap, runs before any D1 round-trip), and a full Argon2id possession proof in the
container plane (expensive, only reached by tokens that already passed the cheap gate). This is why a
concurrent burst of garbage tokens cannot exhaust the Argon2id pool.

# How it works
1. The Worker computes a constant-time HMAC check on the presented token and rejects mismatches in
   well under a millisecond — `worker/src/lib/internal_auth.ts:<L>`.
2. Survivors reach the container, which looks up the non-secret `token_id` in D1, then runs Argon2id
   against the stored hash with a synthetic timing-burn so a hit and a miss are indistinguishable —
   `crates/corelink-container/src/adapter_pat.rs:<L1-L2>`.

# Invariants
- A token that fails the HMAC layer NEVER reaches Argon2id (`crates/corelink-container/src/adapter_pat.rs:<L>`).
- Verification is constant-time at both layers, no timing oracle (`worker/src/lib/internal_auth.ts:<L>`; `crates/corelink-container/src/adapter_pat.rs:<L>`).

# Gotchas
- The native CAS path uses HMAC(deployed PAT_SIGNING_KEY) + D1-existence + scope — Argon2id runs only
  on the adapter plane. A CAS 401 means bad-key OR no-row, not necessarily a bad password.

# Citations
1. `worker/src/lib/internal_auth.ts:<L>` — the edge HMAC fast-reject.
2. `crates/corelink-container/src/adapter_pat.rs:<L1-L2>` — the Argon2id possession proof + timing-burn.
```

---

## C. Authoring checklist (the agent's RELAY — cannot be skipped)

- [ ] `type` is from the taxonomy; `title` non-empty.
- [ ] `source_files` lists EVERY file the claims rest on; each path exists.
- [ ] `checkpoint_sha` = the current HEAD the concept was written against.
- [ ] Every `source_files` path appears ≥1× in `# Citations` as a resolving `path:line`.
- [ ] No claim without a citation. Unverifiable claim → remove it (quality standard, not optional).
- [ ] Cross-links use leading-`/` bundle-relative form.
- [ ] If GENERATED: the marker line is present and the file is not hand-edited afterward.
- [ ] Return the compact card only (path + type + source_files + 1-line summary) — never dump the body.
