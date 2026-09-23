# Luna blocker-wave return cards

Wave baseline: `bdf023adb09115aa09deeca450a42ae9f6e387e8`  
Observed `origin/main`: `140e16eab6315bfdec1a0e4a9892d8557a071781`  
Mode: independent, read-only, cold first-pass review; no GitHub writes.

| WP | Verdict | Lead conclusion |
|---|---|---|
| LUNA-STD | BLOCKED | Standard SHA `e9b9c8ae…` remains blocked by G0–G3; billing semantic confirmation, cf atomicity, current-main/peer reconciliation, pilot approvals, and publication contract remain open. |
| LUNA-HASH | FIX_FIRST | Four current artifact bytes require fresh review; all 42 peer states remain unreconciled; `Digest::as_bytes` contract authority is unresolved because container persistence consumes it. |
| LUNA-BILLING | FIX_FIRST | Package `SKILL.md` is absent; post-update audit failure leaves a partial limiter effect; test pins/execution state and required procedures are inconsistent. |
| LUNA-CF | FIX_FIRST | Relation records need atomic anchors/splits; direct-dependency inventory is incomplete; required local validations are marked not executed. |
| LUNA-SERVER | FIX_FIRST | Pilot is stale against `origin/main@140e16e`; DSR migration numbering and composition-root sources changed; current-pin readback is required. |
| LUNA-E2E | FIX_FIRST | Package `SKILL.md` is absent; tamper coverage is described through the wrong path; recovery is not executable. |
| LUNA-POP | PASS (identity only) | 105 eligible identities still reconcile exactly; 10 manifests changed and package metadata must be refreshed. |
| LUNA-DEDUPE | PARTIAL | Bounded 12-package batch yielded explicit REUSE/EXPAND/DISTINCT/UNRESOLVED decisions with issue/backlog evidence; remaining population still needs decisions. |
| LUNA-AUTH | BLOCKED | No locally evidenced independent owner/peer authority or enforced branch protection; registry remains `UNVERIFIED`. |

## Wave disposition

The wave closed the identity question but did not close the campaign. The next safe actions are
bounded documentation/readback repairs for server, billing, cf-bindings, e2e, and package metadata,
plus evidence-ledger updates for deduplication. Authority-bound peer/owner approvals, standard freeze,
hash contract ownership, remaining deduplication, and issue publication remain blocked and must not be
simulated by local edits.
