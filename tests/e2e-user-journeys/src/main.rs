//! # CoreLink E2E User-Journey Suite — the real ship gate (modular harness)
//!
//! BLACK-BOX: tests use ONLY what a paying customer has:
//!   - the deployed HTTP API (`CORELINK_E2E_ENDPOINT`)
//!   - Bearer PATs (the `CORELINK_E2E_PAT_*` env map)
//!   - the `corelink` CLI binary (for the Bazel CLI round-trip)
//!
//! FORBIDDEN anywhere in this crate: any `corelink-*` crate import, direct
//! D1/R2/KV access, any mock of the system-under-test, reading internal state.
//! Dependencies are frozen at reqwest(blocking)+serde_json+uuid+sha2+hex.
//!
//! ## Layout (modular — a fleet fills the stubs)
//!   - `harness`  — Config + the env token map, JourneyResult, URL builders,
//!     assert helpers (the single source of truth for every route template).
//!   - `personas` — P1..P12 actors → (tenant, token, expectation).
//!   - `journeys/<surface>.rs` — one module per surface, each `pub fn run(...)`.
//!
//! `main` is a RUNNER ONLY: load config → dispatch `journeys::all` → print the
//! results box + SHIP verdict → exit code (0 unless a journey FAILED; a GATED
//! journey never fails the gate).
//!
//! ## Environment
//!
//! | Var | Description |
//! |-----|-------------|
//! | `CORELINK_E2E_ENDPOINT`     | Base URL under test (default `http://localhost:8787`) |
//! | `CORELINK_E2E_TENANT`       | Primary tenant id (path segment for CAS/AC) |
//! | `CORELINK_E2E_TENANT_B`     | Second tenant id (isolation journeys) |
//! | `CORELINK_E2E_PAT_RW`       | Read+write cache PAT (primary tenant) |
//! | `CORELINK_E2E_PAT_RO`       | Read-only cache PAT |
//! | `CORELINK_E2E_PAT_ADMIN`    | Admin-scoped PAT |
//! | `CORELINK_E2E_PAT_REVOKED`  | A revoked PAT (must be denied) |
//! | `CORELINK_E2E_PAT_EXPIRED`  | An expired PAT (must be denied) |
//! | `CORELINK_E2E_PAT_TENANT_B` | A valid PAT for the SECOND tenant |
//! | `CORELINK_E2E_PAT_FREE/_SOLO/_PRO/_ENTERPRISE/_PASTDUE` | Plan-tier PATs |
//! | `CORELINK_E2E_RUN_SLOW`     | `1` to enable slow/CLI journeys (bazel, quota) |
//!
//! Any absent token GATES the journeys that need it — never a silent skip.

// This crate is a SCAFFOLD: the harness URL builders, the full persona/token
// map, and the assert helpers are a frozen contract whose API surface is
// deliberately AHEAD of its consumers — a fleet of agents fills the stub
// journey modules against it. Allow dead-code crate-wide so the skeleton stays
// `-D warnings` clean; each module the fleet completes simply consumes more of
// the surface. (Remove this allow once every journey module is filled.)
#![allow(dead_code)]

mod harness;
mod journeys;
mod personas;

use harness::{build_client, Config, JourneyStatus};

fn main() {
    let cfg = Config::from_env();
    let client = build_client();

    print_header(&cfg);

    let results = journeys::all(&cfg, &client);

    println!();
    println!("┌──────────────────────────────────────────────────────────────────────────────┐");
    println!("│                              Journey Results                                   │");
    println!("├──────────────────────────────────────────────────────────────────────────────┤");

    let (mut pass, mut fail, mut gated) = (0usize, 0usize, 0usize);
    for r in &results {
        let (icon, label) = match &r.status {
            JourneyStatus::Pass => {
                pass += 1;
                ("✓", "PASS ")
            }
            JourneyStatus::Fail(_) => {
                fail += 1;
                ("✗", "FAIL ")
            }
            JourneyStatus::Gated(_) => {
                gated += 1;
                ("⊙", "GATED")
            }
        };
        let title = if r.name.len() > 62 { &r.name[..62] } else { r.name };
        println!("│ {icon} [{label}] ({:>6}ms) {title}", r.duration_ms);
        match &r.status {
            JourneyStatus::Fail(msg) => {
                let t = if msg.len() > 200 { &msg[..200] } else { msg };
                println!("│         DETAIL: {t}");
            }
            JourneyStatus::Gated(reason) => {
                let t = if reason.len() > 200 { &reason[..200] } else { reason };
                println!("│         REASON: {t}");
            }
            JourneyStatus::Pass => {}
        }
        println!("├──────────────────────────────────────────────────────────────────────────────┤");
    }

    println!(
        "│ Summary: {pass} PASS  {fail} FAIL  {gated} GATED (gated = recorded, not skipped)        │"
    );
    println!("└──────────────────────────────────────────────────────────────────────────────┘");
    println!();

    let verdict = if fail > 0 {
        "RED"
    } else if pass > 0 {
        "GREEN"
    } else {
        "GREEN (all journeys gated — set CORELINK_E2E_PAT_* to exercise the live contract)"
    };

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  SHIP-GATE: {verdict:<54} ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    // Exit non-zero ONLY on a real failure; a gated journey never fails the gate.
    if fail > 0 {
        std::process::exit(1);
    }
}

fn print_header(cfg: &Config) {
    let set = |b: bool| if b { "SET" } else { "—" };
    use personas::Persona::*;
    let tok = |p: personas::Persona| set(p.resolve(cfg).map(|r| r.token.is_some()).unwrap_or(false));

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║       CoreLink E2E User-Journey Suite — Black-Box Ship Gate       ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    let ep = &cfg.endpoint[..cfg.endpoint.len().min(54)];
    println!("║  Endpoint: {ep:<54} ║");
    println!("║  Tenant:   {:<54} ║", set(cfg.tenant.is_some()));
    println!("║  Tenant B: {:<54} ║", set(cfg.tenant_b.is_some()));
    println!("║  PAT_RW:   {:<54} ║", tok(P1ReadWrite));
    println!("║  PAT_RO:   {:<54} ║", tok(P2ReadOnly));
    println!("║  PAT_ADMIN:{:<54} ║", tok(P3Admin));
    println!("║  PAT_TenB: {:<54} ║", set(cfg.token(harness::TokenKind::TenantB).is_some()));
    let slow = if cfg.run_slow { "ENABLED" } else { "GATED (CORELINK_E2E_RUN_SLOW=1)" };
    println!("║  Slow:     {slow:<54} ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
}
