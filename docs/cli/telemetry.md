# CoreLink CLI Telemetry — Privacy Policy

**Version:** 1.0.0  
**Effective:** 2026-05-14  
**WI:** WI-S15-005  
**LINDDUN review:** `specs/_audits/2026-05-14-linddun-cli-telemetry.md`

---

## Overview

CoreLink CLI telemetry is **opt-in, default off**. No data is collected unless you explicitly
enable it. This design follows GDPR Article 25 (data protection by design and by default) and
LGPD Article 8 (consent-based collection).

---

## What we collect (when opted in)

| Field | Example | Purpose |
|---|---|---|
| `cli_version` | `"0.1.0"` | Track CLI adoption and drive backcompat decisions |
| `os` | `"linux-x86_64"` | Prioritize platform-specific bug fixes |
| `subcommand` | `"ls"` | Understand which features are used |
| `outcome` | `"ok"` | Measure CLI reliability |
| `duration_ms` | `42` | Track latency for UX improvements |
| `anonymized_id` | UUID v4 | Session continuity without identity linkage |

---

## What we NEVER collect

- `tenant_id` — never collected; not derivable from telemetry data
- Blob digests — never collected
- PAT (Personal Access Token) — never collected
- File paths — never collected
- IP address — scrubbed server-side at ingestion; never stored
- Hostname — never collected
- Username or email — never collected
- Any PII (Personally Identifiable Information)

This is enforced structurally in the CLI codebase and verified by a property test
(`tests/cli_telemetry_optin.rs`) running 10,000 iterations.

---

## How to opt in

```bash
corelink config set telemetry on
```

This sets `telemetry = true` in `~/.corelink/config.toml`. No telemetry is sent before
this command is run on a given machine.

---

## How to opt out

```bash
corelink config set telemetry off
```

This sets `telemetry = false`. All subsequent CLI invocations emit zero telemetry events.

---

## How to check current status

```bash
corelink config list
```

Output includes:

```
telemetry: off
anonymized_id: <uuid>

Privacy policy: docs/cli/telemetry.md
Enable telemetry: corelink config set telemetry on
Rotate telemetry ID: corelink config rotate telemetry-id
```

---

## Anonymized ID

The `anonymized_id` is a UUID v4 generated on first launch and stored in
`~/.corelink/config.toml`. It is:

- **Not linked** to your user account, tenant, or PAT on our servers
- **Rotatable** at any time: `corelink config rotate telemetry-id`
- **Discardable**: deleting `~/.corelink/config.toml` removes it permanently

---

## Telemetry endpoint

Events are sent to:

```
https://telemetry.corelink.humangr.com/v1/events
```

This is a **separate domain** from the CoreLink data plane (`corelink.humangr.com`). You can block
`telemetry.corelink.humangr.com` in your firewall without impacting cache operations.

Timeout: 1 second. If the endpoint is unreachable, the CLI continues normally
(graceful failure — non-blocking; FM-R004).

---

## Data retention

- **Raw events:** aggregated and deleted within 7 days of collection
- **Aggregated metrics:** retained for 90 days
- Aggregated metrics contain only counts per `{cli_version, os, subcommand, outcome}` —
  no `anonymized_id` at the aggregated level

---

## Legal basis

| Regulation | Basis | Article |
|---|---|---|
| GDPR | Explicit consent (opt-in default-off) | Art. 7, Art. 25 |
| LGPD | Consentimento (opt-in) | Art. 8, Art. 6 X |

---

## Contact

Privacy questions: privacy@humangr.com

LINDDUN review is available at `specs/_audits/2026-05-14-linddun-cli-telemetry.md`.
