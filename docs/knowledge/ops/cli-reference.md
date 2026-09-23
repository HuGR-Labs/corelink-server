---
type: "Runbook"
title: "The corelink CLI: JSON output schema & opt-in telemetry"
description: "The machine-readable --output=json contract (with SemVer discipline) and the default-off, PII-free CLI telemetry policy."
checkpoint_sha: "648ecdccd229bdb5154b86843053c28b9cce9d36"
source_files:
  - "docs/cli/json-output-schema.md"
  - "docs/cli/telemetry.md"
source_blobs:
  - "docs/cli/json-output-schema.md@664cc0c97343c0dcf96be8dc6fb700a176c2b04c"
  - "docs/cli/telemetry.md@6e62d551af916eb1745df479357f5ca3575707e0"
---
# The corelink CLI: JSON output schema & opt-in telemetry

The `corelink` CLI is the customer's scripting surface, so two contracts govern it: a stable,
SemVer-disciplined `--output=json` schema that automation and CI can depend on, and a privacy policy
for telemetry that is opt-in and default-off. Both exist to make the CLI safe to wrap — JSON output
never silently drops a field within a major version, and telemetry never carries a tenant id, digest,
PAT, or any PII. This concept is the operator/integrator reference for both. Related: [the audit export + analytics plane](/ops/audit-analytics-plane.md).

# Role

It is the integration contract for anyone scripting against the CLI: what the JSON looks like, how it
evolves, and exactly what (minimal, anonymous) data the binary may phone home when explicitly enabled.

# How it works

- The schema document lists JSON output for the main subcommands, but explicitly records that `config list --output=json` currently ignores the flag and emits TOML; callers must not assume every listed invocation is JSON `docs/cli/json-output-schema.md:17-30`, `docs/cli/json-output-schema.md:187-209`.
- The supported `ls` response is a paginated server body with no total count `docs/cli/json-output-schema.md:34-38`; `version` has its own shape and does not advertise an attestation URL before a real bundle exists `docs/cli/json-output-schema.md:155-183`.
- Errors go to stderr with exit 1 for general failures and exit 2 for CTRL-CRED-001 violations `docs/cli/json-output-schema.md:213-226`.
- Telemetry is opt-in, default off — no data leaves the machine until enabled `docs/cli/telemetry.md:10-15`.
- The collected fields are a fixed, minimal set (version, os, subcommand, outcome, duration, anonymized id) `docs/cli/telemetry.md:18-29`.
- Opt-in/out is a single config command writing `~/.corelink/config.toml` `docs/cli/telemetry.md:46-64`.
- Events go to a separate `telemetry.corelink.humangr.com` domain with a 1s timeout and graceful failure `docs/cli/telemetry.md:98-111`.

# Invariants

- SemVer discipline: additive fields are MINOR, removing/renaming is MAJOR, deprecations warn ≥ 90 days `docs/cli/json-output-schema.md:13-15`.
- The PAT is always redacted in output; passing it as a CLI arg is a hard exit-2 CTRL-CRED-001 violation `docs/cli/json-output-schema.md:221-226`.
- Telemetry NEVER collects tenant_id, digests, PAT, file paths, IP, hostname, username/email, or any PII `docs/cli/telemetry.md:32-44`.
- The `anonymized_id` is not linked to account/tenant/PAT, is rotatable, and is discardable `docs/cli/telemetry.md:87-94`.
- Raw telemetry events are deleted within 7 days; only PII-free aggregates persist 90 days `docs/cli/telemetry.md:114-120`.

# Gotchas

- `write_latency` is `null` in `--read` mode and `read_latency` is `null` in `--write` mode — wrappers must tolerate the null `docs/cli/json-output-schema.md:114-131`.
- You can firewall-block the telemetry domain without affecting cache operations because it is a separate host from the data plane `docs/cli/telemetry.md:98-111`.

# Citations

1. `docs/cli/json-output-schema.md:13-15` — SemVer discipline for the JSON schema.
2. `docs/cli/json-output-schema.md:17-30` — the global `--output=json` flag + subcommand set.
3. `docs/cli/json-output-schema.md:34-38` — `ls` is a paginated `{blobs,next_cursor}` page; `config list` is an explicit exception and prints TOML: `docs/cli/json-output-schema.md:187-209`.
4. `docs/cli/json-output-schema.md:155-183` — version output shape and the absent-until-published attestation field.
5. `docs/cli/json-output-schema.md:114-131` — bench read/write latency nullability.
6. `docs/cli/json-output-schema.md:213-226` — error response + exit codes (1 general, 2 CTRL-CRED-001).
7. `docs/cli/json-output-schema.md:221-226` — PAT-in-args exit-2 violation.
7. `docs/cli/telemetry.md:10-15` — opt-in, default-off telemetry.
8. `docs/cli/telemetry.md:18-29` — the minimal collected field set.
9. `docs/cli/telemetry.md:32-44` — the never-collected list (no PII).
10. `docs/cli/telemetry.md:46-64` — opt-in/out commands.
11. `docs/cli/telemetry.md:87-94` — anonymized_id properties.
12. `docs/cli/telemetry.md:98-111` — separate telemetry domain + graceful failure.
13. `docs/cli/telemetry.md:114-120` — data retention (7d raw / 90d aggregate).
