# Ownership wave 010 — runbooks, runners and external-facing adapters

Source/static-only four-artifact contract. OKF is the verified canonical
reference; use it for routing only and do not fork, duplicate or revalidate
its policy.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-runbook-tracker` | `crates/corelink-runbook-tracker/Cargo.toml` | S |
| `corelink-runner-aggregate` | `crates/corelink-runner-aggregate/Cargo.toml` | S |
| `corelink-runner-overage` | `crates/corelink-runner-overage/Cargo.toml` | S |
| `corelink-signup` | `crates/corelink-signup/Cargo.toml` | S |
| `corelink-slack-real` | `crates/corelink-slack-real/Cargo.toml` | S |
| `corelink-statuspage-real` | `crates/corelink-statuspage-real/Cargo.toml` | S |

Each author changes only its ownership skill and three package documents.
Runbook/runner binaries and external adapter names are source/build facts, not
evidence of Slack, Statuspage, signup, billing, runner or deployment execution.
Target-specific statuspage dependencies must be described exactly rather than
treated as runtime reachability.
