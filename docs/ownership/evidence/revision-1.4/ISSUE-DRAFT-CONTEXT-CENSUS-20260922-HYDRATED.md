# Issue-draft context census — hydrated readback (2026-09-22)

## Scope

This readback covers the 105 publication-draft bodies represented by the
campaign ledger: 95 workspace packages and 10 independent fuzz packages.
It is a documentation-preparation check, not semantic approval, cold review,
runtime evidence, or publication authorization.

## Source

- observed `origin/main`: `0389714d9f5408f744e17227b82d795fff245a32`;
- preparation seed source commit recorded by the current-main bundle:
  `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`;
- source bundle path on `main`:
  `docs/ownership/preparation/2026-09-19/`;
- campaign branch: `codex/corelink-ownership-campaign`.

## Result

| Population | Count |
|---|---:|
| ledger rows inspected | 105 |
| workspace drafts with current-main hydration | 95 |
| fuzz drafts with current-main hydration | 10 |
| drafts with hydration section | 105 |
| drafts without either hydration or prior structured section | 0 |

Every draft contains the bounded section `Contexto hidratado — preparação
current-main`, including source pin, verified facts, packet-level relation or
consumer hints, selected OKF context/risk, candidate commands marked
not-executed, source evidence and explicit limits. Draft body hashes were
refreshed in the publication ledger and the documentary suite passed 137/137.

## Boundary

The hydration section is a starting packet. Its own source states
`SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED`, `deep_semantic_relations_complete:
false` and `issue_ready:false`. It does not prove exhaustive consumers,
runtime reachability, deployment, independent review, frozen standard,
deduplication or issue readiness. Those gates remain separately blocked.

The older metric of 24/105 drafts with the former structured-manifest section
is retained as historical evidence; it must not be added to this 105/105
hydration result as if the populations were disjoint.
