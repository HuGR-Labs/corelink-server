//! CLI subcommand handlers for `corelink-cli` (WI-S15-001).

pub mod audit;
/// Networked audit surface (`audit export` production + `audit tail`),
/// wired to `GET /v1/audit/:tenant/export`. Binary-only (depends on
/// `crate::client`), unlike the lib-mirrored `audit` module.
pub mod audit_net;
/// `corelink bazel-init` — wire a Bazel repo to the CoreLink remote cache.
pub mod bazel_init;
pub mod bench;
/// `corelink cas get/export` + shared local-dir → CAS uploader.
pub mod cas;
/// `corelink ci mirror` — one-shot local-cache → CoreLink mirror.
pub mod ci;
pub mod config_cmd;
pub mod doctor_cmd;
pub mod get;
/// `corelink import` — bulk pre-warm the CAS from a local directory.
pub mod import_cmd;
pub mod ls;
pub mod put;
pub mod runbook_drill;
pub mod stat;
/// `corelink tenant export/verify-export` — data-portability / offboarding.
pub mod tenant;
pub mod verify_ndjson;
// Wave-19 HTTP-aware sibling of `verify_ndjson` — adds
// `verify-ndjson --url <export-url>` (streams the response, reads the
// `x-corelink-audit-export-aborted` trailer, surfaces the canonical
// diagnostic with sysexits DATAERR (65)).
pub mod verify_ndjson_http;
pub mod version;

// Stream-1 "ridiculously easy to use" CLI additions.
/// `corelink ac put/get` — action cache operations.
pub mod ac;
/// `corelink login --token=<PAT>` — store token + cache tenant_id.
pub mod login;
/// `corelink whoami` — query /v1/users/me and display identity.
pub mod whoami;
