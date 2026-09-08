### Fixed

- **B-058 OKF auto-reconcile now executes on the correct runner.** Trusted `main`
  pushes and main-only dispatches use the pinned Codex action on the persistent
  macOS builder fleet, with no fork trigger or secret exposure. The workflow is
  hermetic and fail-closed around Codex: complete OKF paths, mutation-tested
  denial guards, Git-state snapshots, document-only edits, and validator/fixture
  gates precede any PR.
