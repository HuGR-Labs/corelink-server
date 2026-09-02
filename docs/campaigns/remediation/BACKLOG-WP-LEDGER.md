# Backlog work-package ledger

> Baseline: `origin/main@76fa37de9bcf12b5e562edce129ad1328f9f8be2`,
> measured 2026-09-02 in `America/Sao_Paulo`.

This is the execution ledger for the open CoreLink backlog. `BACKLOG.md` remains
the single source of truth for item status. This ledger derives work ownership,
dependency order and completion contracts from that source; it never overrides
the backlog.

The baseline population is 168 items: 107 open, 60 done and one parked. The 107
open items are partitioned exactly once across four contract catalogs:

```ledger-state
base-ref: origin/main
base-sha: 76fa37de9bcf12b5e562edce129ad1328f9f8be2
observed-at: 2026-09-02
item-count: 168
open-count: 107
done-count: 60
parked-count: 1
catalog-counts: B001-B045=8,B046-B090=31,B091-B130=36,B131-B167=32
```

| Catalog | Numeric range | Open IDs |
|---|---:|---:|
| [`work-packages/B001-B045.md`](work-packages/B001-B045.md) | B-001..B-045 | 8 |
| [`work-packages/B046-B090.md`](work-packages/B046-B090.md) | B-046..B-090 | 31 |
| [`work-packages/B091-B130.md`](work-packages/B091-B130.md) | B-091..B-130 | 36 |
| [`work-packages/B131-B167.md`](work-packages/B131-B167.md) | B-131..B-167 | 32 |
| **Total** | | **107** |

PR #1544 is not part of this baseline. Its proposed B-169/B-170 additions are
intentionally not assigned here; if that PR merges, the lead must re-run the
verifier on the merge commit and append both IDs in a new catalog revision.
The earlier B-168 allocation is already terminal (`done`) on this baseline and
is excluded from all executable coverage.

The CI-grammar lane has one executable order, checked mechanically below; the
order deliberately breaks the former WP-148/WP-150 cycle:

```wp-dependency-order
WP-140 | none
WP-146 | none
WP-148 | WP-140,WP-146
WP-150 | WP-148
```

## Authority and contract

The normative delegation contract is
[`docs/internal/DELEGATION-WP-CONTRACT.md`](../../internal/DELEGATION-WP-CONTRACT.md).
Its axiom applies without exception: execution is delegated; judgment is not.
The lead owns decisions, cold review, integration, GitHub operations and merge.

Every executable WP in the catalogs must state all of the following:

1. exact tracked B-IDs and live predecessor/base;
2. read-first sources and the facts to learn from each;
3. exclusive file/symbol allowlist and explicit non-goals;
4. the decided change, with no delegated product or policy choice hidden in it;
5. safety and behavior invariants;
6. a closed population plus zero/truncated/unparsed failure behavior;
7. Definition of Done, including behavior proof and integration state;
8. quality gates, including a load-bearing mutation with the expected failure;
9. predecessor, conflict lane and integration order;
10. the return card required from the executor.

An executor must HALT when the closed world is wrong: an owned file moved, a
required population cannot be enumerated, an upstream decision is unresolved,
or completing the task requires a file outside the allowlist. The executor does
not silently widen scope.

## Global quality invariants

- Every numeric claim names its population, ref and observation time.
- Zero, truncated, unparsed or unreachable populations are `INDETERMINATE`, not
  green.
- A gate proves green -> relevant mutation -> the expected red -> restore ->
  green. Making the entire harness fail does not count.
- A shipped capability is verified at its production entrypoint and artifact;
  source presence alone is not wiring.
- Documentation that truthfully refuses a claim does not close the runtime
  defect behind that claim.
- Shared spines have one writer. Sibling WPs may research in parallel but cannot
  edit `BACKLOG.md`, OpenAPI/router surfaces, common workflows, shared memory
  budgets or other declared spines concurrently.
- Commits carry DCO, changelog fragments when required, no secrets, applicable
  OKF validation and a clean diff. Merge is only through
  `bash scripts/pre-merge-gate-check.sh --merge <PR>` after independent cold
  review and all non-skipped checks are green.

## Conflict lanes

| Serial lane | Shared authority | Parallel work allowed outside the lane |
|---|---|---|
| Backlog/parser | `BACKLOG.md`, `scripts/backlog_verify.py`, backlog workflow | Read-only census and file-disjoint implementation |
| Auth | PAT schema, principal semantics, auth test gate | Credential-independent docs and probes |
| Audit | drain caller, chain sealing, audit evidence queries | Read-only production measurements |
| Money | tier/DPA auth, Stripe subscription state, billing semantics | Read-only exposure census |
| Memory | process envelope, CAS/Turbo/Argon2 limits | Measurement harnesses without runtime edits |
| Timing | phase ledger, `Server-Timing`, residual attribution | Independent black-box probes |
| API | OpenAPI, shipped routes, generated docs, API comparator | Client-only fixes after the contract freezes |
| OKF | citation parser, anchor verifier, bulk shifter | Read-only citation inventory |
| CI grammar | workflow parser, triggers, concurrency groups | File-disjoint lane repairs after grammar lands |
| Published claims | shared pages and corpus checkers | Specialized pages with explicit exclusions |

## Critical path

1. Reconcile active PRs and repair the backlog/parser and gate instruments.
2. Establish auth execution, API-surface, performance-phase and deployability
   instruments.
3. Execute the Auth, Audit, Money, Memory and Timing serial spines.
4. Reconcile OpenAPI/routes/generated docs, then run disjoint client contracts.
5. Prove deployability -> staging -> current artifact in all production regions.
6. Run the full station matrix with two tenants and real signup; only then make
   a go-live decision.

Owner-only actions remain explicit WPs, not hidden blockers. Agents may prepare
the evidence packet and exact command, but may not spend money, sign legal
instruments, delete owner-held keys or mutate third-party control planes unless
the owner performs or separately authorizes that act.

## Mechanical completeness

The ledger is complete only when a verifier compares the live `status: open`
population in `BACKLOG.md` with the `tracked-ids` declarations in all four
catalogs and proves:

- no open ID is absent;
- no open ID appears twice;
- no done or parked ID is assigned for execution;
- every declared predecessor resolves to a WP, a live PR, an owner action or an
  explicitly external repository contract;
- every shared file/symbol is owned by one active author at a time.

The verifier and its positive/negative fixtures are part of this ledger. The
numeric table above remains a claim about this immutable baseline; a later
`BACKLOG.md` change requires a new observation and catalog revision.
