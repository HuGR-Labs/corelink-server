---
id: "CLI-JSON-SCHEMA"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
tags: ["cli", "json-output", "schema", "semver"]
---

# CoreLink CLI JSON Output Schema (v1.0.0)

> **SemVer discipline**: additive changes (new optional fields) are MINOR bumps.
> Removing or renaming existing fields is a BREAKING change requiring a MAJOR bump.
> Deprecation warnings MUST be emitted for ≥ 90 days before removal (`--debug` mode + telemetry when opt-in).

## Global flag

All subcommands accept `--output=json` (or the shorthand `--json` on `doctor`).

```
corelink ls      --tenant acme --output=json
corelink get     <digest>      --output=json
corelink put     <file>        --output=json
corelink stat    <digest>      --output=json
corelink bench   --full        --output=json
corelink doctor  --json
corelink version              --output=json
corelink config  list         --output=json
```

---

## `corelink ls --output=json`

```json
{
  "entries": [
    {
      "digest": "string (BLAKE3 hex)",
      "size_bytes": 12345,
      "tenant_prefix": "string (pseudonymised)",
      "created_at": "ISO-8601 timestamp"
    }
  ],
  "next_cursor": "string | null",
  "total_count": 42
}
```

---

## `corelink get --output=json`

```json
{
  "digest": "string",
  "bytes_written": 12345,
  "destination": "string (file path or '-' for stdout)",
  "client_verify": "ok"
}
```

---

## `corelink put --output=json`

```json
{
  "digest": "string (BLAKE3 hex)",
  "bytes_uploaded": 12345,
  "source": "string (file path)"
}
```

---

## `corelink stat --output=json`

```json
{
  "digest": "string",
  "size_bytes": 12345,
  "created_at": "ISO-8601 timestamp",
  "age": "string (human-readable, e.g. '2 days')",
  "region": "string (e.g. 'wnam')",
  "tenant_id_pseudonym": "string (e.g. 't-abc***')"
}
```

---

## `corelink bench --output=json`

```json
{
  "mode": "full | write | read",
  "ops": 100,
  "total_ms": 4200,
  "ops_per_sec": 23.8,
  "write_latency": {
    "p50_ms": 18,
    "p95_ms": 45,
    "p99_ms": 120,
    "min_ms": 5,
    "max_ms": 250
  },
  "read_latency": {
    "p50_ms": 12,
    "p95_ms": 30,
    "p99_ms": 80,
    "min_ms": 3,
    "max_ms": 200
  }
}
```

`write_latency` is `null` in `--read` mode; `read_latency` is `null` in `--write` mode.

---

## `corelink doctor --json`

```json
[
  {
    "check": "network",
    "status": "ok | fail | skip",
    "latency_ms": 42,
    "error_code": "COR_NET_UNREACHABLE | null",
    "next_action": "string | null"
  }
]
```

Array of 8 check objects. See `docs/error_taxonomy.md` for all `COR_*` error codes.

Exit code: `0` if all checks ok/skip, `1` if any check fails.

---

## `corelink version --output=json`

```json
{
  "version": "0.1.0",
  "git_rev": "abc1234",
  "build_timestamp": "epoch:1715040000",
  "slsa_attestation": "https://corelink.dev/attestations/cli/0.1.0/abc1234/slsa3.json",
  "target_triple": "aarch64-apple-darwin"
}
```

---

## `corelink config list --output=json`

```json
{
  "auth": {
    "pat": "corelink_prod_abc***",
    "byok_enabled": false
  },
  "defaults": {
    "tenant_id": "acme-corp",
    "output_format": "text"
  },
  "telemetry": {
    "enabled": false
  }
}
```

PAT is always redacted in output (CTRL-CRED-001). Full secret is never emitted.

---

## Error response (any subcommand, non-zero exit)

Errors are written to `stderr`. Exit code `1` for general errors, `2` for CTRL-CRED-001 violations.

```
error: <human-readable message>
```

For exit code `2` (PAT in CLI args):

```
error: PAT must NOT be passed as a CLI argument (security control CTRL-CRED-001).
       Use env var CORELINK_PAT or config file ~/.corelink/config.toml instead.
```
