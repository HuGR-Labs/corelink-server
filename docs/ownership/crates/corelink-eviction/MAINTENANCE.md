---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-eviction
manifest: crates/corelink-eviction/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: eviction-source-static-20260920
---

# corelink-eviction — maintenance guide

This guide is deliberately source-only. Every procedure has execution mode `SOURCE_ONLY` and evidence state `NOT_EXECUTED`; it must not be read as a runbook for a backend, D1, Cloudflare, scheduler, or production eviction. Verified OKF material is routed at [the OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md), not reproduced or revalidated.

[Scope](#m01) · [Triage](#m02) · [Pipeline](#m03) · [State](#m04) · [TTL/quota](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Establish source-only scope

**Execution mode:** SOURCE_ONLY. **Evidence state:** NOT_EXECUTED. Identify `crates/corelink-eviction/Cargo.toml`, `src/lib.rs`, and the specific source module before reviewing a change. Record the exact source commit, symbol, invariant, and whether evidence is source text or unobserved execution/runtime. Success means the request is bounded to static ownership. Stop when it requires a real adapter, scheduler, persistent data, or runtime result; route that decision to the accountable integration owner.

<a id="m02"></a>
## M02 — Triage against the relation map

**Execution mode:** SOURCE_ONLY. **Evidence state:** NOT_EXECUTED. Classify the request against [B01](BLAST_RADIUS.md#b01), [B02](BLAST_RADIUS.md#b02), [B03](BLAST_RADIUS.md#b03), [B04](BLAST_RADIUS.md#b04), [B05](BLAST_RADIUS.md#b05), and the four known consumer relations in [B06](BLAST_RADIUS.md#b06). Identify the exported type/function and source test that would falsify the change.

Preserve B06 unknowns instead of assuming a consumer or backend. Success means every touched static contract has an owned relation; completeness means untraced edges are marked unknown. Do not turn this classification into a claim that any path executes.

<a id="m03"></a>
## M03 — Maintain decision and reachable-check invariants

**Execution mode:** SOURCE_ONLY. **Evidence state:** NOT_EXECUTED. For phase or probe work, compare [phase.rs](../../../../crates/corelink-eviction/src/phase.rs) with [reachable.rs](../../../../crates/corelink-eviction/src/reachable.rs) and the relevant assertions in [prop_eviction.rs](../../../../crates/corelink-eviction/tests/prop_eviction.rs). Maintain tenant scope, the strict timestamp predicate, explicit decision result, and a source-visible no-delete branch when a witness is returned. Quality standard: state the falsifying input boundary, not a vague safety promise. Stop if the change affects chunks, physical deletion, or a query implementation outside the trait/fake.

<a id="m04"></a>
## M04 — Maintain state and soft-delete contracts

**Execution mode:** SOURCE_ONLY. **Evidence state:** NOT_EXECUTED. For state or tombstone work, inspect [storage_state.rs](../../../../crates/corelink-eviction/src/storage_state.rs) and [blob_meta.rs](../../../../crates/corelink-eviction/src/blob_meta.rs). Reconcile the tenant-plus-region state key, non-underflow reclaim path, monotonic watermarks, digest validation, and already-resolved outcome. Quality standard: distinguish the fake map’s behavior from any potential database behavior. Stop if atomicity, migration application, object lifecycle, or cross-service recovery is needed; none is established by this package source.

<a id="m05"></a>
## M05 — Maintain TTL and quota arithmetic

**Execution mode:** SOURCE_ONLY. **Evidence state:** NOT_EXECUTED. For retention or trigger work, inspect [tier.rs](../../../../crates/corelink-eviction/src/tier.rs), [reservation.rs](../../../../crates/corelink-eviction/src/reservation.rs), and [trigger.rs](../../../../crates/corelink-eviction/src/trigger.rs). Record exact source boundaries: 95% inclusive trigger, zero-quota below-threshold outcome, 90% rounded-up target, reservation floor/cap, and Enterprise-only override cap. Quality standard: preserve integer rounding/saturation meaning and name an example that would falsify the proposed result. Do not claim a quota counter, reservation, or dispatch is live.

<a id="m06"></a>
## M06 — Handoff and Definition of Done

**Execution mode:** SOURCE_ONLY. **Evidence state:** NOT_EXECUTED. **Success criteria:** the review names source files, exported contract, B relation, R05 invariant, affected consumer, route, and unknowns. Route shared tier changes to analytics/rate-limit owners, facade exports to CAS, and reservation TTL/region/state contracts to billing, coordinating each with eviction. **Completeness criteria:** changed source-local interfaces, fakes, test assertions, consumer relations, and documentation are reconciled; adapter and runtime questions remain explicitly open.

**Quality standards:** links resolve; SOURCE is not conflated with execution/runtime; no D1, Cloudflare, scheduler, or production behavior is asserted. **Definition of Done:** source scope and baseline are recorded, affected invariants are falsifiable, B01–B06 and R06/R07 are updated as needed, and an independent reviewer is requested for cross-boundary work.

Recovery is a design decision for the owner of the unverified external boundary, not a promise of this source-only guide.

**Candidate documentary checker procedure:** from the repository root, preserve the JSON result of each relevant kind invocation after an ownership-doc change.
```sh
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-eviction/SKILL.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-eviction/REFERENCE.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-eviction/BLAST_RADIUS.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-eviction/MAINTENANCE.md
```
The checker is external and unintegrated. PASS is structural only, not semantic completeness, cold approval, or execution/runtime proof; record those evidence states separately.

[Return to scope](#m01)
