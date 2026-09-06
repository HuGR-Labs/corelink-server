---
id: "ADR-S14-010"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-09-05"
updated: "2026-09-05"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s14", "failover", "health", "low-traffic", "fail-closed"]
---

# ADR-S14-010 — Low-traffic failover samples remain conservative

## Decision

Keep `FAILOVER_MIN_SAMPLES` (default 50) as an anti-blip statistical floor,
but never interpret a newly observed under-floor 5xx event as healthy. Each
5xx event has a monotonic identity and is consumed once; a clean observation
therefore cannot re-count the same transient while it remains in the bounded
window. `InMemoryFailoverRouter` maps a newly indeterminate probe to a degraded
snapshot, and the container `HysteresisGate` still requires three consecutive
degraded observations before blocking writes. Empty or clean under-floor
windows remain healthy.

An authenticated internal `POST /_internal/failover/heartbeat` endpoint
refreshes a heartbeat wired through `FailoverLayerState`. The public `/_health`
readiness endpoint is deliberately unable to refresh it. If that external
signal is stale, the guard fails closed after the same hysteresis threshold even
when the data plane has seen no traffic at all; data-plane responses never
refresh the heartbeat.

This gives both required properties: a three-request transient cannot freeze
writes by itself, while a quiet region that is actually returning errors cannot
remain falsely healthy forever. Probe uncertainty is therefore fail-closed at
the routing decision, with hysteresis limiting the blast radius.

## Invariants

- A newly observed low-traffic 5xx event never returns a positive health claim
  more than once, and a following clean event is healthy.
- A clean/empty low-traffic window does not trigger failover.
- Probe errors map to degraded in the router; writes remain blocked only after
  the existing hysteresis latch.
- A stale external heartbeat is fail-closed through the same hysteresis gate,
  without requiring request samples.
- Anonymous or incorrectly authenticated requests cannot refresh the heartbeat;
  missing internal auth fails closed.
- The decision is deterministic for a fixed sample sequence and timestamp.

## Implementation anchors

- `crates/corelink-container/src/routes/failover.rs`
- `crates/corelink-failover-router/src/router.rs`
- `docs/knowledge/planes/replication-failover.md`
