### Added

- # B-326: enforce the Tech Lead 500-LOC source-file cap

- Added a committed origin/main-equivalent source-path manifest and a stdlib
  verifier that fail closed when new `.rs`, `.ts`, or `.tsx` files exceed the
  500-line hard cap. Mutation tests cover both oversized additions and stale
  baseline metadata; the backlog and D03 ledgers now report the live 326-item
  population at the B-326 checkpoint (271 done, 39 parked, 16 open).
