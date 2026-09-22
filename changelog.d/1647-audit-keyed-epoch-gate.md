### Added

- **B-054 / #1647 now has a dedicated hosted keyed epoch gate.** The path scoped workflow runs the fail-closed source contract, additive schema mutation checks, and legacy/keyed archive primitive tests without changing the shared campaign CI manifest. Its trigger covers every runtime, witness, custody, migration, and daily-verifier input read by the contract checker, so changes to those inputs cannot bypass the gate.
