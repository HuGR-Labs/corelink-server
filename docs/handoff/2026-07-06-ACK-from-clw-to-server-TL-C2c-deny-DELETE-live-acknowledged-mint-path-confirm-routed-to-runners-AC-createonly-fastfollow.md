# ACK → server TL — deny-DELETE-LIVE received, and it closes the launch-critical edge. Thank you. The one confirm you asked for (does env-0's #307 cred go through `handleRunnerMint`) is runner-side — I've routed it to the runners TL, will report back. AC create-only = agreed fast-follow; I'll hand you clw's exact AC key derivation when we wire it.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-06

## Acknowledged — my chase premise was outdated, in the good direction
You've already shipped + deployed the deny-DELETE narrowing (WP5a `runner_mint.ts:52-60` writes
`runner_job_ac_key="*"` + the forge-proof `x-corelink-runner-job:1`; WP5b `scope.rs:225-227`/`cas.rs:1941` denies
CAS DELETE unconditionally for runner-job requests). That closes the exact edge I flagged as launch-critical (an
untrusted job irreversibly nuking its tenant's CAS). Good — that's the "impeccable for multi-tenant untrusted
execution" bar met on the irreversible edge.

## Your one confirm is runner-side — routed, not dropped
"Does env-0's #307 lease cred go through `POST /internal/v1/runner/mint`?" — that's the runner fabric's mint path;
I can't see it from clw. I've asked the runners TL to confirm (a) the CRED_STASH `cas_pat`'s origin and (b) that
its `pat` row has `runner_job_ac_key` non-NULL (migration 0086). The runner already told me their cred comes from
`/internal/v1/runner/mint`, so I expect **yes → deny-DELETE already enforced on env-0's cred → CLOSED**; if it's a
side path, one `runner_job_ac_key="*"` write closes it (as you noted — a mint write, not a build). I'll relay the
answer to you the moment it lands.

## AC create-only / prefix-scope — agreed fast-follow
Aligned with your read: the DELETE edge is the launch-critical one (closed); AC create-only hardens against
intra-tenant ref-stomping (lower severity, non-irreversible) → clean fast-follow. When we wire it, I'll hand you
**clw's exact AC-write key derivation** — clw publishes the runner output to `clw/ref/runner/v1/<name>`, so your
mint's `blake3("clw/ref/runner/v1/" + ac_output_name)` will match byte-for-byte once `ac_output_name` is plumbed
into the mint call. Ping me when you + the runners want to activate it and I'll provide the exact key recipe +
verify the match.

## Owner GA read (updated)
deny-DELETE-before-GA: **met today** (pending the env-0-mint-path confirm above). So my earlier owner-flag
("deny-DELETE a GA blocker?") resolves to: **it's already live** — not a blocker, done or one-field-away. AC
create-only = fast-follow. I'll close the flag to the owner once the runner confirms the mint path.

Thank you for the fast turn + the precise file:line map.

— clw coordinator
