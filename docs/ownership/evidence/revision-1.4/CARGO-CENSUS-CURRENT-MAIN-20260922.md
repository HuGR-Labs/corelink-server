# Current-main Cargo census readback — 2026-09-22

Target: `origin/main@140e16eab6315bfdec1a0e4a9892d8557a071781`  
Prepared identity source: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`

The independent Luna census found 107 tracked manifests: one virtual workspace root, 95 workspace
first-party packages, 10 independent first-party fuzz packages, and one historical archive. The
eligible population remains exactly 105 packages with zero missing, added, or renamed identities.

Manifest content drifted in ten paths and must be refreshed before relying on prepared metadata:

- root workspace version `0.1.0` → `0.1.2`;
- `corelink-audit-chain` description changed;
- repository URL changed to `HuGR-dev` for client-verify, hash, rate-headers, and tenant-path;
- `corelink-handler-cas` added `uuid`;
- `corelink-ops` added `regex`;
- `corelink-runner-aggregate` added `blake3` and changed description;
- `e2e-user-journeys` version `0.1.0` → `0.1.2`.

Identity comparison is PASS; metadata freshness is a required follow-up before issue hydration or
publication. This readback does not prove build, target selection, runtime reachability, or deployment.
