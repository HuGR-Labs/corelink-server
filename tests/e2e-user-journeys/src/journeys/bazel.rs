//! Bazel REAPI v2 journeys — `/bazel/v2/{instance}/...`.
//!
//! Migrated from the old single-file suite (journey 4). This is the CLI
//! round-trip: `corelink bazel-init` in a throwaway workspace, then two
//! `bazel build //...` — the second must show a remote cache hit. Gated behind
//! `corelink` + `bazel` on PATH and `CORELINK_E2E_RUN_SLOW=1`.

use std::env;
use std::process::Command;
use std::time::Instant;

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};
use crate::personas::Persona;

/// Run the Bazel journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![bazel_round_trip(cfg, client)]
}

/// Bazel CLI round-trip → second build hits the remote cache.
fn bazel_round_trip(cfg: &Config, _client: &Client) -> JourneyResult {
    let name = "Bazel: CLI round-trip — bazel-init + 2 builds → 2nd hits cache";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if !cfg.run_slow {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_RUN_SLOW=1 not set — Bazel CLI round-trip skipped (requires bazel + corelink CLI + a valid PAT)",
        );
    }
    if Persona::P1ReadWrite.resolve(cfg).is_err() {
        return JourneyResult::gated(name, "CORELINK_E2E_PAT_RW not set");
    }

    // Require both binaries on PATH.
    if !cmd_ok("which", &["bazel"]) {
        return JourneyResult::gated(name, "`bazel` not found on PATH (Bazel 7+ required)");
    }
    if !cmd_ok("corelink", &["--version"]) {
        return JourneyResult::gated(
            name,
            "`corelink` CLI not found on PATH (https://corelink-get.humangr.com)",
        );
    }

    // Throwaway workspace.
    let tmp = env::temp_dir().join(format!("corelink-e2e-bazel-{}", uuid::Uuid::new_v4()));
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        return JourneyResult::fail(name, ms(start), format!("mkdir temp: {e}"));
    }
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&tmp);
    };
    if let Err(e) = std::fs::write(
        tmp.join("MODULE.bazel"),
        "module(name = \"corelink_e2e_test\", version = \"0.0.1\")\n",
    ) {
        cleanup();
        return JourneyResult::fail(name, ms(start), format!("write MODULE.bazel: {e}"));
    }
    let build = r#"genrule(
    name = "hello",
    outs = ["hello.txt"],
    cmd = "echo 'hello corelink e2e' > $@",
)
"#;
    if let Err(e) = std::fs::write(tmp.join("BUILD.bazel"), build) {
        cleanup();
        return JourneyResult::fail(name, ms(start), format!("write BUILD.bazel: {e}"));
    }

    // corelink bazel-init.
    let init = Command::new("corelink")
        .arg("bazel-init")
        .current_dir(&tmp)
        .env("CORELINK_E2E_ENDPOINT", &cfg.endpoint)
        .output();
    match init {
        Ok(o) if !o.status.success() => {
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("bazel-init failed: {err}"));
        }
        Err(e) => {
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("bazel-init exec: {e}"));
        }
        Ok(_) => {}
    }

    // First (cold) build.
    let b1 = Command::new("bazel")
        .args(["build", "//..."])
        .current_dir(&tmp)
        .env_remove("BAZEL_CACHE_SILO_KEY")
        .output();
    match b1 {
        Ok(o) if !o.status.success() => {
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("first build failed: {err}"));
        }
        Err(e) => {
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("first build exec: {e}"));
        }
        Ok(_) => {}
    }

    // Second build — expect remote cache hit.
    let b2 = Command::new("bazel")
        .args(["build", "//..."])
        .current_dir(&tmp)
        .output();
    cleanup();
    let b2 = match b2 {
        Ok(o) => o,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("second build exec: {e}")),
    };
    if !b2.status.success() {
        let err = String::from_utf8_lossy(&b2.stderr);
        return JourneyResult::fail(name, ms(start), format!("second build failed: {err}"));
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&b2.stdout),
        String::from_utf8_lossy(&b2.stderr)
    );
    if !combined.contains("remote cache hit") && !combined.contains("cache hit") {
        let tail = &combined[..combined.len().min(500)];
        return JourneyResult::fail(
            name,
            ms(start),
            format!("second build showed no remote cache hit. output: {tail}"),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// `true` if the command exists and exits 0.
fn cmd_ok(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
