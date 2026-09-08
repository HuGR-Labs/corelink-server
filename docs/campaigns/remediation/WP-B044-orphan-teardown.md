# WP-B044 — orphan-box teardown proof (external execution)

**Baseline:** `corelink-server@5e655c7d2`
**Owner:** `tl`
**Execution repository:** sibling `corelink-runners`
**Disposition:** authoring contract complete; production teardown remains open

This work package records the safety contract for B-044. The runtime
implementation, observation credentials, and production enablement belong to
`corelink-runners`; this repository must not claim that teardown is live. The
current observe-only reconciliation and its app scope/KV pagination are
pre-work, not proof that a destroy is safe.

## Scope and exclusive allowlist

The executor may change only these sibling-repository surfaces:

- `deploy/cloudflare/src/index.ts`, specifically `reconcileOrphanBoxes`,
  `listRunningInstances`, `listSpawnedBoxHandles`, and the `sbox:` writer;
- the directly associated runner tests, metrics, runbook, and declarations for
  `RECONCILE_ORPHAN_BOXES` and `RECONCILE_ORPHAN_TEARDOWN`.

This server-side authoring patch may change this WP, its static verifier and
tests, the B-044 backlog pointer, the focal workflow, and the changelog
fragment. It must not add a teardown implementation to `corelink-server` or
edit the dirty sibling checkout.

## Read first

Read B-044 in `BACKLOG.md`, the observe-only reconciliation, the app-scoped
instance listing, the complete paginated `sbox:` reader, the `sbox.h` writer,
the runner deployment/metrics runbook, and the Cloudflare Containers destroy
semantics. Treat the 403 account-level instance-delete probe as an API limit,
not as evidence that a different destroy target is safe.

## Decided change

The first real candidate must be observed and classified by an exact
instance-to-handle join. A candidate is **type-1** only when all of these are
true in the same fail-closed observation window:

1. the instance belongs to the stable runner application and the expected
   runner namespace;
2. the complete paginated `sbox:` set was read without truncation or error;
3. `instance.name` matches an existing `sbox:` record and that record's `h`
   is the exact handle selected for teardown; and
4. the join was re-read immediately before the action and remains unchanged.

Only a proven type-1 candidate may reach an **inert-by-default** teardown path
behind the `RECONCILE_ORPHAN_TEARDOWN` secret. A type-2 candidate (platform or
warm-pool name), an unknown name, a mismatch, a stale read, a missing record,
or any incomplete/error window is `HALT`: observe and alert, but never destroy.

The destroy target is `getContainer(RUNNER_NAMESPACE, h)` using the joined
record's `h`; `instance.name` is not a durable-object handle. The executor
must not use an account-level endpoint, an untrusted namespace, or an
instance-name fallback. The app-id and namespace checks are mandatory and
prevent cross-tenant or cross-application teardown.

## Safety invariants

- Enumeration is restricted to the runner app and fully paginated. Any
  pagination error, truncation, missing tick, zero/unknown population, or
  unclassifiable candidate fails closed.
- The teardown secret is absent/off by default and is not enabled by the
  implementation change. A per-tick destroy cap and an age floor above the
  longest job are required because destroy is liveness-blind.
- The final pre-destroy join re-read is authoritative. If the app, namespace,
  `instance.name`, or `sbox.h` changes, the action is refused.
- Destroy is idempotent for retries of the same exact namespaced handle. A
  retry may not select another tenant or handle. The `sbox:` record is
  conditionally removed only after a confirmed destroy; on read/destroy/delete
  error it is retained for a safe retry and `orphan_box_reaped` is not emitted.
  A success marker or equivalent dedupe prevents duplicate success accounting
  when delivery is repeated.
- Type-2, unknown, cross-tenant, stale, and otherwise indeterminate candidates
  are never destroyed, even when a caller supplies a plausible handle.
- Successful teardown emits the distinct `orphan_box_reaped` metric with the
  exact app/namespace/handle evidence; it does not reuse an ordinary job metric.

## Tests and mutation guard

The runner-side focal suite must cover a proven type-1 join, secret-off
behavior, type-2/unknown/mismatched names, cross-app and cross-namespace
records, pagination truncation/error, age-floor and per-tick-cap behavior,
destroy rejection, delete-after-destroy ordering, and repeated delivery.
There must be no live destroy in tests.

The server-side verifier and its mutation suite reject each of these changes:

1. remove the exact instance↔`sbox.h` join;
2. use `instance.name` as the destroy handle;
3. allow an unknown/type-2 candidate;
4. remove app or namespace isolation;
5. treat a pagination error/truncation as safe;
6. delete `sbox:` before destroy succeeds;
7. arm teardown by default or remove the per-tick/age safety bounds; and
8. remove retry idempotence or success deduplication.

## Closed-world completeness clause

The population is every runner instance and every key in the complete
paginated `sbox:` set during the recorded observation window. “No candidate,”
missing logs, zero scans, a truncated page, an API error, an unknown name, or
an unproven live join is not evidence of safety or closure; it is a HALT and
must remain observable. The work package is not complete until the exact
population, join evidence, destination, and owner-controlled enablement state
are recorded. If the observed candidates are type-2 or none, the honest
outcome is a documented programmatic-teardown limitation with observe+alert
retained.

## Definition of done and return card

The return card must include baseline/ref and observation timestamps, instance
population, complete `sbox:` pagination evidence, candidate classification,
exact files changed, invariant and failure-mode results, inert/live destination
evidence (or the external limitation), focal gates, and owner enablement state.
It must not contain secrets, a claim of runtime behavior not measured, or a
claim that B-044 is closed while `status: open` remains in the backlog.
