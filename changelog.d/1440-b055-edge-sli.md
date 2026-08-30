### Fixed

- **An edge-served `findMissingBlobs` emitted no CAS SLI at all, so the SLOs
  stopped covering exactly the requests F2 made fast (B-055).** With
  `EDGE_FIND_MISSING = "on"` in all five prod envs, the Worker answers in-colo
  and never reaches `R2CasHandler::exists_batch_inner`, which is where
  `Sli::AvailCasGet` / `Sli::LatencyCasGetP99` were emitted. The audit rows
  were already written through the container (B-047) — the observation was
  not. An R2 fault in-colo could have made every edge probe slow and moved
  those SLOs by **zero**, in every region.
  The emit now rides the awaited `POST /_internal/audit/cas-attempted` call the
  edge already makes, which is the one place the edge and the container agree
  on what happened: no second transport, no second author. It mirrors
  `exists_batch_inner`'s own `emit` closure exactly — **one pair per BATCH, not
  per digest**, on the same three outcomes (cross-tenant denial and audit
  failure as errors, a written batch as a success).
  Both handlers and the route now fold into ONE process-wide
  `CountingSliObserver` (`sli_aggregate::shared()`). Two observers would have
  produced two partial views of a single SLO — the same shape of blindness this
  item exists to remove.
  The Worker reports the R2 probe window as `edge_ms`. ⚠️ Stated rather than
  quietly mixed: that window **excludes the audit round-trip being made to
  report it**, while the container's own latency includes its audit write, so
  edge-served samples run slightly LOW against container-served ones. The field
  is OPTIONAL, so a Worker deployed before it existed still gets its rows and
  its 204, recorded with a zero latency that `SliCounters` keeps
  distinguishable from "fast" by counting it in `total` while adding nothing to
  the sum.
  Pinned in both languages, and the latency pin proven RED first: forcing
  `latency_us` back to a literal `0` fails the Rust test with `left: 0, right:
  10000`. ⚠️ **What is NOT proven here:** that the emit reaches production. That
  needs a container roll, and a deploy freeze is in effect — see the PR body
  for the exact probe.
