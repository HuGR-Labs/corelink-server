---
id: "cli-reference"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "cli", "clw", "wave-devenv"]
---

# CoreLink DevEnv — CLI Reference

The `clw` command-line utility provides direct orchestration of remote DevEnvs, workspace synchronization, and snapshot lifecycle.

---

## 1. Commands Overview

### `clw devenv start`
Starts or resumes a DevEnv container for the current tenant.
```bash
clw devenv start [--name <WORKSPACE_NAME>] [--tier <standard-2|standard-4|power-8|ultra-16>]
```

### `clw devenv status`
Queries the active state, endpoints, and vCPU usage of the DevEnv.
```bash
clw devenv status [--json]
```

### `clw devenv stop`
Safely stops the active DevEnv, triggering an automated pre-stop snapshot.
```bash
clw devenv stop [--force]
```

### `clw snapshot`
Creates a content-addressed snapshot of the local or remote workspace.
```bash
clw snapshot --name <REF_NAME> [--concurrency <N>] [--force] [--json]
```

### `clw hydrate`
Restores a snapshot reference into the local directory or target container.
```bash
clw hydrate --name <REF_NAME> [--force]
```

### `clw ls`
Lists all available workspace snapshots for the tenant.
```bash
clw ls [--name <REF_NAME>] [--ref-domain runner] [--json]
```
