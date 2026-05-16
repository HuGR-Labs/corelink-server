//! CLI subcommand handlers for `corelink-cli` (WI-S15-001).

pub mod audit;
pub mod bench;
pub mod config_cmd;
pub mod doctor_cmd;
pub mod get;
pub mod ls;
pub mod put;
pub mod runbook_drill;
pub mod stat;
pub mod verify_ndjson;
// Wave-19 HTTP-aware sibling of `verify_ndjson` — adds
// `verify-ndjson --url <export-url>` (streams the response, reads the
// `x-corelink-audit-export-aborted` trailer, surfaces the canonical
// diagnostic with sysexits DATAERR (65)).
pub mod verify_ndjson_http;
pub mod version;
