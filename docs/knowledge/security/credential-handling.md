---
type: "SecurityControl"
title: "Credential handling & PAT secrecy"
description: "How CoreLink stores, separates, and protects its secrets and PATs — and the one credential-mishandling finding (PAT plaintext in Clerk public_metadata) that drove a fix wave."
source_files:
  - "docs/security/2026-06-23-secreview-credentials.md"
  - "docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["security", "credentials", "pat", "secrets", "clerk"]
timestamp: "2026-06-26T00:00:00Z"
---

# Credential handling & PAT secrecy

Every CoreLink cache request is authenticated by a Personal Access Token, and the worker↔container
control path is gated by internal-auth keys; the whole money/identity surface rests on those secrets
never leaking and the credential planes staying separated. This concept records the go-live secrets
review posture (strong and defensible: no committed live secrets, dedicated keys where it matters,
fail-closed salts, constant-time compares) alongside the one real credential-mishandling finding —
PAT plaintext broadcast in a Clerk session JWT — that the team fixed by moving the secret off all
client/JWT surfaces. It complements the [PAT moat](/auth/pat-moat.md),
[Argon2id verify](/auth/argon2id-verify.md), and the [D1 PAT store](/auth/d1-pat-store.md).

# Role

It is the credential-secrecy control: where secrets live, how the PAT planes (native HMAC vs adapter
Argon2id) are separated, how internal-auth keys are split per consumer, and where the delivery
channel for a freshly-minted PAT must NOT be (client-readable Clerk metadata / the session JWT).

# How it works

- The go-live secrets review found no committed live secrets and rated the posture strong: dedicated
  internal-auth keys are genuinely separated where it matters, the PAT planes are cleanly separated,
  secret-bearing structs redact in Debug, the header-trust strip is structural, and the
  predictable-salt fallback fails CLOSED in prod (`docs/security/2026-06-23-secreview-credentials.md:9-29`).
- The two highest-value internal surfaces — cross-tenant introspection (`FABRIC_INTROSPECT_AUTH_KEY`)
  and billing ingest (`BILLING_INGEST_AUTH_KEY`) — read their dedicated key directly with a ≥32-char
  floor and NO fallback to the shared key (`docs/security/2026-06-23-secreview-credentials.md:72-82`).
- PAT plane separation holds: the native plane verifies via constant-time HMAC, the adapter plane via
  Argon2id, mint is a pure function that returns the plaintext once and never logs it, and rotate/
  revoke is tenant-scoped and never opens a zero-valid-PAT window (`docs/security/2026-06-23-secreview-credentials.md:116-139`).
- Internal-auth compares are constant-time on both the TS and Rust gates: the provided value is
  padded to the expected length, one timing-safe compare runs, then a length-equality bit is AND-ed
  in — no length oracle (`docs/security/2026-06-23-secreview-credentials.md:135-139`).
- The earlier HIGH finding: the signup flow wrote the freshly-minted PAT plaintext into Clerk
  `public_metadata`, which is client-readable and embedded in the session JWT — so the PAT was
  broadcast in every session token to every service that validates it (`docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:8-21`).
- The only intended cleanup was client-driven (it ran only if the user opened `/welcome`), so an
  un-visited welcome page left the PAT resident forever — confirmed on the owner's own account weeks
  after signup (`docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:18-26`).
- The fix moves the secret off all client/JWT surfaces: write the plaintext to Clerk
  `private_metadata` (backend-only), reveal it server-side via the Backend API, and add a scrub cron so an
  un-visited welcome cannot leave it resident — though this concept does NOT confirm that cron is
  wired-and-running in prod (live status unverified; see Gotchas)
  (`docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:32-46`).

# Invariants

- The PAT plaintext never lives on a client-readable or JWT-broadcast surface — `public_metadata`
  keeps only `{tenant_id, region}`; the secret goes to `private_metadata` and is scrubbed
  (`docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:32-46`).
- No live secret is committed: the repo secret sweep returns only test literals and doc placeholders,
  and `.env.local` (real test keys) is gitignored and untracked (`docs/security/2026-06-23-secreview-credentials.md:155-168`).
- Internal-auth verification is constant-time with no length oracle on both planes (`docs/security/2026-06-23-secreview-credentials.md:135-139`).

# Gotchas

- The internal-auth consumer-key split is INERT until the operator actually provisions the dedicated
  per-consumer keys: until then `pat_mint`/`admin`/`erase`/`runner_mint` all fall back to the one
  shared `CORELINK_INTERNAL_AUTH_KEY`, so a single leaked secret unlocks all four (LOW-1; the two
  highest-blast-radius keys are exempt from this fallback) (`docs/security/2026-06-23-secreview-credentials.md:34-53`).
- The remediation leaves a documented, accepted residual: the plaintext still lives transiently in
  Clerk `private_metadata` (a sub-processor backend store) until the clear/cron — the future
  hardening is a single-use reveal in our own D1 (`docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:48-51`).
- The "guaranteed scrub cron" is described in the fix design, but this concept does NOT confirm the cron
  is wired-and-running in prod — its live status is UNVERIFIED here. Until that is independently
  confirmed, PAT plaintext must be assumed to persist in Clerk `private_metadata`, so an IR responder must
  NOT conclude "no live PATs in Clerk" from the existence of the scrub-cron design
  (`docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:32-46`).

# Citations

1. `docs/security/2026-06-23-secreview-credentials.md:9-29` — go-live verdict + severity counts (no committed live secrets).
2. `docs/security/2026-06-23-secreview-credentials.md:34-53` — LOW-1: consumer keys fall back to the shared internal-auth key.
3. `docs/security/2026-06-23-secreview-credentials.md:72-82` — INFO-1: dedicated introspect/billing keys truly separated (no fallback).
4. `docs/security/2026-06-23-secreview-credentials.md:116-139` — INFO-4: PAT lifecycle, plane separation, constant-time, rotation.
5. `docs/security/2026-06-23-secreview-credentials.md:155-168` — repo secret sweep: no committed live secrets.
6. `docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:8-21` — the finding: PAT plaintext in `public_metadata` is JWT-broadcast.
7. `docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:18-26` — client-driven cleanup → PAT persists forever.
8. `docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:32-46` — the SOTA fix: move to `private_metadata` + server reveal + scrub cron.
9. `docs/security/2026-06-19-CRED-pat-plaintext-in-clerk-public-metadata.md:48-51` — the accepted transient-residual at the sub-processor.
