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
  `container.start({ env })` block. Secrets-matrix rows #204/#205 already
  existed but described only the edge half (`worker/src/index.ts`); they now
  record the container half and why it was unreachable.

### Notes

- **`?? ""` is safe here by a specific, verified property — not by convention.**
  The container fails **CLOSED** on a sibling that is present-but-malformed:
  `from_env` returns `None` and the adapter routes do not mount at all
  (`adapter_pat.rs:1470`). Forwarding a non-empty placeholder would have turned
  an *unprovisioned optional secret* into total PAT-auth downtime — and neither
  sibling is bound in prod today, so the empty path is the **real** path, not an
  edge case. It works because the container reads these through `non_empty_env`
  (`storage.rs:119`), which trims and treats EMPTY exactly like ABSENT. The
  item's closing verify gates on that property directly, so a change to
  `non_empty_env` cannot silently arm the outage.

- Measured: `tsc --noEmit` on `worker/` reports **16 errors with and without this
  change** — all pre-existing (`replication_coordinator_do.ts`, plus 4 already in
  `durable_object.ts`). Zero new. The `Env` type already declared both siblings
  optional (`index.ts:175-176`). `secrets-checklist-verify` OK (no drift);
  `validate_secrets_matrix` `code_only=0`.

- **No Rust changed.** B-081 declares a dependency on B-067 (no CI test execution
  for `corelink-pat`) because it is a credential repair. The container half
  already existed; the defect was entirely Worker-side. That dependency still
  holds for the next repair that touches the crate — not for this one.
