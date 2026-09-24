# Current-main drift readback — 2026-09-23

Observed `origin/main`: `e0f231110524fe81ed0b8d3451903879daf9d519`.
Campaign HEAD: `3df52eb71acdd4e00084d42f0b64191674bf42eb`.
Historical campaign baseline: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.

## Findings

- The current main branch is not the campaign baseline; the branch divergence is `629 ahead / 380 behind` from the campaign checkout perspective.
- Ten Cargo manifests or lock/workspace files differ from the historical baseline: root `Cargo.toml`, `Cargo.lock`, `crates/corelink-audit-chain/Cargo.toml`, `corelink-client-verify`, `corelink-handler-cas`, `corelink-hash`, `corelink-ops`, `corelink-rate-headers`, `corelink-runner-aggregate`, `tenant-path`, and `tests/e2e-user-journeys/Cargo.toml`.
- Material drift includes repository URL changes, dependency additions (`uuid`, `regex`, `blake3`), package/version changes, and audit/runner description or policy changes.
- Therefore historical artifact pins remain evidence of the reviewed snapshot only. A freeze/publication decision must either re-anchor affected packages to this current main or explicitly preserve the historical pin and mark the affected package stale.
- No source, Cargo, registry, issue or production state was changed by this readback.

## Gate consequence

Current-main reconciliation is **BLOCKED** for affected packages until their manifest/source evidence is refreshed. The Cargo census identity result (105 eligible packages, 107 tracked manifests) remains valid as a population count, but it is not a current-main semantic approval.
