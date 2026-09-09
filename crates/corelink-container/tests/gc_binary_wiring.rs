//! B-071 artifact wiring contract.
//!
//! These checks intentionally inspect the declared build and CI interfaces
//! rather than requiring a container build in the Rust test process. The
//! production build workflow independently extracts the OCI rootfs and runs
//! this same executable in forced dry-run mode.

const DOCKERFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Dockerfile"));
const BUILD_WORKFLOW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.github/workflows/container-build-push-prod.yml"
));
const DRY_RUN_WORKFLOW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.github/workflows/gc-sweep-dry-run.yml"
));
const PRODUCTION_SWEEP_BIN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bin/gc_sweep.rs"));

fn gate_4b_source(workflow: &str) -> Option<&str> {
    let start_marker =
        "      - name: Gate 4b — native production GC binary is present and fail-closed";
    let end_marker = "      - name: ADR-0015 reproducibility attestation (reminder)";
    let start = workflow.find(start_marker)?;
    let body_start = start + start_marker.len();
    let end = workflow[body_start..].find(end_marker)? + body_start;
    Some(&workflow[start..end])
}

fn gate_4b_has_required_contract(gate: &str) -> bool {
    gate.contains("GC_OBSERVATION_ONLY=true")
        && gate.contains("GC_LIVE_DELETE=false")
        && gate.contains("gc_sweep FAILED (fail-closed)")
}

#[test]
fn runtime_image_contains_a_separately_invoked_gc_binary() {
    let build = "cargo build --release --locked -p corelink-gc --bin gc_sweep;";
    let stage = "cp /build/target/release/gc_sweep /out/gc_sweep";
    let runtime = "COPY --from=builder /out/gc_sweep /usr/local/bin/gc_sweep";
    let production_build =
        "cargo build --release --locked -p corelink-server --bin corelink-gc-sweep-production";
    let production_stage =
        "cp /build/target/release/corelink-gc-sweep-production /out/corelink-gc-sweep-production";
    let production_runtime =
        "COPY --from=builder /out/corelink-gc-sweep-production /usr/local/bin/corelink-gc-sweep-production";

    assert!(
        DOCKERFILE.contains(build),
        "gc_sweep must be built in builder"
    );
    assert!(
        DOCKERFILE.contains(stage),
        "gc_sweep must leave the cache mount"
    );
    assert!(
        DOCKERFILE.contains(runtime),
        "gc_sweep must enter the runtime image"
    );
    assert!(
        DOCKERFILE.contains(production_build),
        "native production sweep must be built under its distinct name"
    );
    assert!(
        DOCKERFILE.contains(production_stage),
        "native production sweep must leave the cache mount"
    );
    assert!(
        DOCKERFILE.contains(production_runtime),
        "native production sweep must enter the runtime image"
    );
    assert!(
        !DOCKERFILE.contains("ENV GC_LIVE_DELETE=true"),
        "the image must never default to live deletion"
    );
    assert!(
        DOCKERFILE.contains("ENTRYPOINT [\"/usr/local/bin/corelink-server\"]"),
        "shipping gc_sweep must not replace the server entrypoint"
    );
}

#[test]
fn native_sweep_observation_requires_explicit_false_delete_gate() {
    let source = include_str!("../src/gc_sweep.rs");
    for required in [
        "GC_OBSERVATION_ONLY",
        "GC_OBSERVATION_ONLY=true requires GC_LIVE_DELETE=false",
        "pub observation_only: bool",
    ] {
        assert!(
            source.contains(required),
            "observation contract must contain {required:?}"
        );
    }
    assert!(
        PRODUCTION_SWEEP_BIN.contains("observation_only={}"),
        "the native sweep binary must report the observation-only contract"
    );
}

#[test]
fn image_inspection_runs_gc_as_a_measurable_dry_run() {
    for required in [
        "Gate 4 — embedded GC binary is forced dry-run",
        "[ -x \"$B/rootfs/usr/local/bin/gc_sweep\" ]",
        "GC_LIVE_DELETE=false",
        "reclaimable_count         = 1",
        "reclaimable_bytes         = 4096",
        "deleted_count             = 0",
        "deleted_bytes             = 0",
    ] {
        assert!(
            BUILD_WORKFLOW.contains(required),
            "image inspection must contain {required:?}"
        );
    }
}

#[test]
fn native_production_gate_scopes_explicit_false_delete_guard() {
    let gate = gate_4b_source(BUILD_WORKFLOW).expect("Gate 4b must be present");
    assert!(gate_4b_has_required_contract(gate));

    // A mutation that removes the explicit false delete guard from Gate 4b
    // must not be rescued by the identical dry-run setting in Gate 4.
    let mutated = gate.replacen("GC_LIVE_DELETE=false", "GC_LIVE_DELETE", 1);
    assert!(!gate_4b_has_required_contract(&mutated));
}

#[test]
fn scheduled_sweep_is_credentialless_and_cannot_arm_delete() {
    for required in [
        "GC_LIVE_DELETE: \"false\"",
        "env -u CLOUDFLARE_API_TOKEN -u CLOUDFLARE_ACCOUNT_ID cargo run -p corelink-gc --bin gc_sweep",
        "reclaimable_count         = 1",
        "reclaimable_bytes         = 4096",
        "deleted_count             = 0",
        "deleted_bytes             = 0",
    ] {
        assert!(DRY_RUN_WORKFLOW.contains(required), "scheduled dry-run must contain {required:?}");
    }
    assert!(
        !DRY_RUN_WORKFLOW.contains("live_delete:"),
        "the scheduled dry-run workflow must not expose a live-delete input"
    );
}
