# Cold review W2 — `corelink-handler-customer`

Reviewer: independent agent `/root/cold_review_handler_customer`; fresh task context.  
Reviewed: 2026-09-23T18:36:13Z. Checkout HEAD: `3df52eb71acdd4e00084d42f0b64191674bf42eb`; source pin: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. The package source and customer container paths have no diff from that pin. This review assesses the current working-tree document bytes, including uncommitted W2 edits. It is not an author approval or runtime certification.

| Artifact | SHA-256 | Verdict |
|---|---|---|
| `.claude/skills/own-corelink-handler-customer/SKILL.md` | `a8bab4256eb38d44baa5961067671ab878ea36111479e86012bca047cc4b9e77` | `FIX_FIRST` |
| `REFERENCE.md` | `c1f3fe32b0208a2a2e6cc6051049fb0fa37fd6b608f1fd3542bd6689f206b382` | `FIX_FIRST` |
| `BLAST_RADIUS.md` | `e8f3f0be2d757697a0368520b44f6d609b5f6fea7ca6f187df53bb388b8f58ea` | `FIX_FIRST` |
| `MAINTENANCE.md` | `5487b26fd1d71bab5e98a64be61f32d9868937ecdce20dd0770ef7c36a6a6c19` | `FIX_FIRST` |

`check_docs.py --root .` reported `IMPLEMENTED_CHECKS_PASS` separately for all four artifacts (S profile: 84/140/194/94 lines; 568/1452/1516/703 words). `git diff --check` passed. These checks do not establish semantic completeness. No Cargo build, test, network, customer data or runtime action was performed.

## Skill — `FIX_FIRST`

Navigation challenge: editing `CustomerTeamHandler::remove`, changing `AuditSink::emit`, or changing SLI observation activates the skill and reaches R03/R04, B03/B04 and M03/M04. Mounting `/v1/customer/*` and changing a real PAT backend do not activate this package as primary owner; the skill's S02/S06 refuse those actions. A customer-data lookup and a deploy request also stop at S06. The routing is useful and stays within three links.

Required finding S1: S03 omits the applicable cross-cutting OKF/`built-not-wired` review routes required by `STANDARD.md` §5. The skill already uses the implemented/wired distinction, but it does not route the maintainer to its canonical source. Add only the relevant canonical links; do not duplicate OKF policy.

## Reference — `FIX_FIRST`

The W2 endpoint split is source-consistent: 11 trait methods correspond to API-001–011; `handler.rs:426-939` confirms the per-endpoint tenant guard, distinct audit kinds/resources, and create/revoke/invite/remove branches. API-010 correctly separates the trait's PAT revocation obligation from the fake's `0` revoked count. INV-001 is supported by the guard calls; INV-002 is supported only for the named fake mutation paths and correctly notes post-mutation `AuditFailed`.

Required finding R1: R03's two “Support contracts” (`AuditSink::emit` and `SliObserver::observe`) have no API IDs or complete contract records. The public `AuditEventKind::slug`, `AuditEvent::new`, `SliObservation::new`, fake constructors/inspection methods, and exported audit/observer types are also not reconciled to contract records or explicit justified exclusions. This prevents a complete public-contract census under `STANDARD.md` §6; `audit.rs`, `observer.rs` and `lib.rs` are direct evidence. At minimum record the two collaborator contracts, their error/effect semantics, units (`at_unix_ms`, `latency_us`) and relevant relations.

Required finding R2: R06 contains errors and compatibility, while target/feature/config and observability facts are scattered or absent. The required R06/R07/R08 questions need directly navigable answers: manifest has no feature/build dependency, and the SLI fake may drop observations on poisoned lock (`observer.rs`). These can be concise; the checker passing sections is not proof that their required subject matter is present.

## Blast radius — `FIX_FIRST`

The W2 atomicity repair succeeds for the challenged paths: REL-012–022 name each endpoint's tenant-denial kind/resource; REL-023–031 separate missing/idempotent/active revoke, invalid/accepted invite, and missing/owner/allowed remove. API-001–011 point to the matching denial and mutation spans; every new REL points back to an API. Source trace in `handler.rs:426-939` supports those local facts. The reference's INV-002 backlink range ends at REL-029 and omits REL-030/031, however, so the invariant does not navigate to both remove outcomes.

Required finding B1: the direct manifest dependencies `corelink-slo`, `uuid` and `thiserror`, the inverse `corelink-server` consumer, and local tests are mentioned unevenly but not reconciled as discovered/documented/excluded relationships. `uuid` materially supplies the invitation token shape (`handler.rs:812-826`), and `corelink-slo::definition::Sli` is a reexport/runtime type (`observer.rs`). B04/B06 prose does not give those boundaries atomic relation records or population counts. `STANDARD.md` §7.2/7.5 requires the census and explicit exclusions.

Required finding B2: B01's 11 method relations cite the server's Cargo REL-033 as reciprocal, but that peer does not identify each individual method boundary or a shared contract key. Record the shared cross-package identity or clearly separate the package dependency from method-specific consumer relations, then reconcile peer facts under `COMMON.md` “Relações”. No runtime claim is needed.

Required finding B3: B04 is a grouped SLI propagation statement rather than an atomic relation for the `SliObserver` emission and `corelink-slo::definition::Sli` reexport. It also lacks activation/failure boundary and validation per relation. Add the local typed boundary, including the observer's infallible interface and best-effort in-memory behavior, without claiming delivered telemetry.

## Maintenance — `FIX_FIRST`

PROC-002 now routes a method to its API, denial REL and applicable mutation branch, and explicitly checks the team removal obligation against the D1 implementation. Its stop on external identity/durability is appropriate. PROC states remain `reviewed-not-executed`, consistent with this review's no-Cargo scope.

Required finding M1: PROC-003 tells the maintainer to “execute checker” but supplies no exact command or file selection; M04 is prose about tests rather than the required package/target/features validation matrix. Add copyable document-check commands and distinguish any source test command as reviewed, blocked or applicable in an isolated environment. `STANDARD.md` §8 requires the exact validation and expected predicate; a checker result in this review does not retroactively mark PROC-003 executed in the manual.

Required finding M2: for a post-mutation `Committed` audit failure, M03/PROC-002 says to escalate but gives no concrete local recovery/compensation check for the partial fake state. The source can leave a PAT row without token if the second lock fails (`handler.rs:659-670`) and can return `AuditFailed` after a completed mutation (`handler.rs:672-681,878-892`). M05 should state how to inspect/reconcile state and where any real backend recovery belongs, without suggesting an unsafe blind retry or `git revert` as data recovery.

No artifact is rejected: the W2 endpoint/denial/mutation corrections are materially accurate. Fix the listed contract and coverage gaps, capture new hashes, and review the changed bytes again before approval.
