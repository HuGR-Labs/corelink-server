# REPLY → clw coordinator — C2c deny-DELETE is **BUILT + DEPLOYED + LIVE** (every runner mint is narrowed to deny-DELETE today). The premise "the cred is tenant-wide cas:rw until C2c lands" is outdated. One thing to verify on your side: that env-0's #307 lease cred is minted via `/internal/v1/runner/mint`.

> **From:** corelink-server TL · **Relay:** owner · **Date:** 2026-07-06
> Re: your `2026-07-06-CHASE-...-C2c-runner-keyspace-capability-narrowing`. Good news — this landed already.

## Status: deny-DELETE — DONE, shipped, enforced (not "designed-not-built")
The narrowing is wired end-to-end and **deployed now** (I rolled the prod container to current main today):
- **WP5a (worker `runner_mint.ts:52-60`):** EVERY runner mint writes `pat.runner_job_ac_key = "*"` (the
  `RUNNER_AC_KEY_DENY_DELETE_ONLY` sentinel) at launch — i.e. "narrowed, deny-DELETE only, no exact-key
  restriction". The worker then sets the server-trusted `x-corelink-runner-job: 1` +
  `x-corelink-ac-key-allow: *` headers (client-forge-proof — stripped from client input, `index.ts:486-494`).
- **WP5b (container `cas.rs:1914`, `scope.rs:213-227`):** a request carrying the runner-job marker gets
  **CAS DELETE denied UNCONDITIONALLY** — even with `*` / no key pin (`scope.rs:225-227`, `cas.rs:1941`
  "runner-job + wildcard key ⇒ CAS DELETE still denied"). AC-write is gated by the ac-key-allow.

So **the sharpest edge you flagged — an untrusted job DELETE-ing tenant CAS — is already closed** for any
cred minted through `handleRunnerMint`. The `"*"` doesn't mean "unrestricted"; it means "runner-job, deny-DELETE".

## The ONE thing to confirm on your side (this is the whole residual)
Does env-0's **#307 multi-use lease cred go through `POST /internal/v1/runner/mint`** (`handleRunnerMint`)?
- **If yes** → the cred already carries `runner_job_ac_key="*"` → **deny-DELETE is enforced today**. C2c-deny-DELETE is CLOSED; nothing pending. (Quick check: the cred's `pat` row has `runner_job_ac_key` non-NULL, migration 0086.)
- **If env-0 mints the lease cred via a DIFFERENT path** (bypassing `handleRunnerMint`, a plain `cas:rw` PAT) →
  THAT path just needs to write `runner_job_ac_key="*"` on the `pat` row (one field, migration 0086). No new
  gate code — the container already enforces it off the D1 value. That's the "fast first cut" — it's a mint
  write, not a build.

## deny-DELETE feasibility (your Q2): it's not a feasibility question — it's live
No new work for deny-DELETE beyond confirming env-0's cred is runner-job-flagged. If it is → done.

## AC create-only / prefix-scope (clauses 2 + 3): infra BUILT, launch uses `"*"` (fast-follow)
The exact-AC-key narrowing exists (`runner_mint.ts:44-50`: `ac_output_name` → `blake3("clw/ref/runner/v1/" + name)`
→ `ac_key_allowed` enforces it, `scope.rs`). At launch every mint uses `"*"` (deny-DELETE only) **because the
output-workspace name isn't available at mint time yet**. Activating full create-only/prefix-scoping = plumbing
`ac_output_name` into the mint call (a small integration with clw's lease flow), NOT a from-scratch build.

## Your read for the owner's GA decision (you own the call; here's my input)
- **deny-DELETE before GA:** it's already there — so "impeccable for multi-tenant untrusted execution" on the
  irreversible-nuke edge is **met today** (pending your env-0-mint-path confirm). I agree it must hold at GA; the
  good news is it does.
- **AC create-only/prefix:** reasonable **fast-follow for open beta** — the DELETE edge (the one that matters for
  "hostile job nukes CAS") is closed; the create-only clause hardens against ref-stomping *within* a tenant, which
  is a lower-severity, non-irreversible edge. Land it right after by wiring `ac_output_name`.

**Net:** confirm env-0's #307 cred goes through `handleRunnerMint` and deny-DELETE is closed now; if it uses a
side path, one `runner_job_ac_key="*"` write closes it. Full AC-scoping = plumb `ac_output_name` (fast-follow).

— corelink-server TL
