### Fixed

- **Following the documented PAT signing-key rotation runbook caused an outage
  (B-081).** The container already accepts rotation-overlap keys —
  `adapter_pat.rs:1466` folds `PAT_SIGNING_KEY_PREV` / `PAT_SIGNING_KEY_NEW`
  into the HMAC key set, so a PAT minted under either still verifies during the
  overlap window (`key_management.md §3.2.1`). The Durable Object forwarded only
  the current key, so that whole mechanism was **unreachable**: an operator
  swapping the key invalidated every live PAT at the container instantly — a
  self-inflicted outage at the worst possible moment, since rotation is what you
  do when a key is compromised.

  `worker/src/durable_object.ts` now forwards both siblings in the
  `container.start({ env })` block, and a behavioral test observes that exact
  start object for present and absent siblings. Secrets-matrix rows #204/#205 already
  existed but described only the edge half (`worker/src/index.ts`); they now
  record the container half and why it was unreachable. This repairs the code
  path but does **not** close B-081: start-time env does not update the active
  population — every per-tenant container plus the shared `_oci` container that
  authenticates OCI PATs at `/token` and serves `/v2/*` in each environment.

### Notes

- **`?? ""` is safe here by a specific, verified property — not by convention.**
  The container fails **CLOSED** on a sibling that is present-but-malformed:
  `from_env` returns `None` and the adapter routes do not mount at all
  (`adapter_pat.rs:1470`). Forwarding a non-empty placeholder would have turned
  an *unprovisioned optional secret* into total PAT-auth downtime — and neither
  sibling is bound in prod today, so the empty path is the **real** path, not an
  edge case. It works because the container reads these through `non_empty_env`
  (`storage.rs:119-133`), which trims and treats EMPTY exactly like ABSENT. A
  focused semantic unit test covers absent, empty, blank, and non-empty values,
  so the proof does not depend on grepping for an implementation token.

- **Operational rollout remains blocked.** The normal data plane has one
  `CoreLinkServer` per tenant, while `/_internal/admin/recycle-system` reaches
  only the five regional `_system` instances. There is no active-population
  enumerator, exact-population recycle ledger, or boot-generation attestation.
  B-081 therefore remains open; the non-executable safe ritual and its unblock
  conditions are recorded in
  `specs/_runbooks/RB-PAT-SIGNING-KEY-ROTATION.md`.

- Measured: `tsc --noEmit` on `worker/` reports **16 errors with and without this
  change** — all pre-existing (`replication_coordinator_do.ts`, plus 4 already in
  `durable_object.ts`). Zero new. The `Env` type already declared both siblings
  optional (`index.ts:175-176`). `secrets-checklist-verify` OK (no drift);
  `validate_secrets_matrix` `code_only=0`.

- The verifier behavior is unchanged. Rust now factors its existing
  EMPTY-as-ABSENT rule through a pure helper solely so the load-bearing semantic
  contract has an executable unit test. B-067 remains relevant to authoritative
  CI execution for credential code.

### Fixed (pre-existing, surfaced by this PR)

- **Five OKF citations in `docs/knowledge/planes/durable-object.md` pointed at the
  wrong lines, and had on `main` too.** `R2_AUDIT_BUCKET` was cited at `:899`
  (actually `NEAR_CEILING_ALERT_SINK`), `AUDIT_ARCHIVE_BATCH_LIMIT` at `:900`, and
  `CORELINK_ORIGIN_TIMING_DETAIL` / `OCI_PUBLIC_DEDUP_ENABLED` /
  `OCI_UPSTREAM_ON_MISS` at `:909` / `:1015` / `:1016` — all five landing on
  comment lines. Verified against `origin/main` before touching them: the content
  at those lines is byte-identical there, so this is pre-existing drift, not
  displacement caused by this PR (this branch's file is net-zero lines and differs
  from `main` only in 836-848).

  Corrected by CONTENT to `:912`, `:913`, `:929`, `:1028`, `:1029`, and the block
  range `774-1018` → `774-1031` (the env object actually closes at `:1031`).
  Cold review also caught the newly added B-081 citation landing on Stripe
  runner prices (`:868-869`); its actual siblings are at `:847-848`.
  Re-anchoring alone would have restored a green gate over five wrong citations —
  the failure mode this repo has already paid for.
