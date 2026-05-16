# `corelink audit verify-ndjson` — re-verify an audit-log export

> WI-S09-008 customer-facing audit-export endpoint, customer-CLI
> re-verify path. Wave-17 shipped offline file mode; wave-19 adds
> HTTP-aware streaming with mid-stream abort detection.

The `corelink` CLI re-verifies a streaming NDJSON envelope produced
by `GET /v1/audit/export`. Two transports are supported.

## Offline (file) mode — wave-17

Use this after downloading the export NDJSON to disk (e.g., for
long-term retention or air-gapped re-verification):

```bash
corelink audit verify-ndjson \
    --ndjson ./export-2026-05-15.ndjson \
    --chain-head-anchor 7f8a...c4
```

| Flag | Required | Description |
|---|---|---|
| `--ndjson <FILE>` | yes | Path to the NDJSON envelope on disk. Mutually exclusive with `--url`. |
| `--chain-head-anchor <HEX>` | yes | 64-char BLAKE3 hex from the `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header at export time. |

Exit codes:

- `0` — full chain integrity verified.
- `1` — structured chain-break / malformed envelope / anchor mismatch
  (the error message carries the canonical
  `{verified, file, line, observed, expected, kind}` payload).

## HTTP-aware mode — wave-19

Use this to re-verify directly off the wire (no intermediate file).
The CLI streams the response, watches for the wave-18
`x-corelink-audit-export-aborted` HTTP trailer, and surfaces the
canonical mid-stream-abort diagnostic if the server detected a chain
break before flushing all bytes:

```bash
export CORELINK_PAT=corelink_prod_<token_id>.<secret>.<sig>
corelink audit verify-ndjson \
    --url 'https://api.corelink.dev/v1/audit/export?from=0&to=10000000000'
```

You can also pass the bearer explicitly via `--bearer <TOKEN>`; the
flag is hidden from `--help` output to avoid accidental shell-history
leakage (CTRL-CRED-001). The CLI never logs or prints the token.

| Flag | Required | Description |
|---|---|---|
| `--url <URL>` | yes | Export endpoint URL (`http` or `https`). Mutually exclusive with `--ndjson`. |
| `--bearer <TOKEN>` | yes (env fallback) | PAT for the export request. Defaults to `CORELINK_PAT` env var. Never logged. |
| `--chain-head-anchor <HEX>` | optional | Recovered from the `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header automatically. Supply for defence-in-depth: if both the header and the flag are present, they MUST match constant-time. |

### Mid-stream abort detection

When the server detects a chain break MID-STREAM, it emits an HTTP/1.1
trailer:

```
x-corelink-audit-export-aborted: {"break_at_seq":42,"break_at_chunk":7,"observed":"<64hex>","expected":"<64hex>"}
```

The CLI parses the payload, prints the canonical diagnostic to
**stderr**, and exits **sysexits DATAERR (65)**:

```
$ corelink audit verify-ndjson --url '...'
AUDIT_EXPORT_ABORTED: break_at_seq=42 break_at_chunk=7 observed=deadbeef... expected=cafef00d...
$ echo $?
65
```

The structured payload survives intact in any JSON output (`--output
json`) so SIEM / Drata wrappers can ingest the four fields directly.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Verified — chain intact, manifest + anchor agree. |
| `65` | sysexits DATAERR — mid-stream abort trailer detected; diagnostic on stderr. |
| `1` | Other error (network, TLS, HTTP non-2xx, malformed envelope, chain-break before trailer detection, anchor disagreement, etc.). |

Wrapping scripts (Drata evidence collection, SIEM re-export
automation) can distinguish the structured data-integrity event from
generic CLI failure by checking specifically for `65`.

### Security notes

- The bearer token is materialised into a single `Authorization`
  header and **never** logged, printed, or surfaced in error messages
  (CTRL-CRED-001 enforced by the wave-19 network-failure regression
  test).
- The anchor cross-check (`--chain-head-anchor` flag vs response
  header) uses constant-time ASCII compare so a side channel cannot
  reveal which byte differs.
- The HTTP-fetch path caps the response body at 64 MiB so a
  tampered/malicious server cannot drain CLI memory; a 1k-event
  export is comfortably under 1 MiB.

### Cross-references

- Spec: `specs/04_sprints/S09/work_items/WI-S09-008-customer-audit-export.md` §6 + §12
- Server emit: `apps/server/src/routes/audit_export.rs` (constants `HEADER_EXPORT_ABORTED`, `HEADER_CHAIN_HEAD_ANCHOR`)
- Wave-18 server tests: `apps/server/tests/audit_export.rs` (`abort_trailer_emitted_on_mid_stream_chain_break`, `customer_cli_handles_abort_trailer_gracefully`)
- Wave-19 CLI sources: `crates/corelink-cli/src/commands/verify_ndjson_http.rs` + `crates/corelink-cli/tests/verify_ndjson_http.rs`
