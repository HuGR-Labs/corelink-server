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

#[test]
fn runtime_image_contains_a_separately_invoked_gc_binary() {
    let build = "cargo build --release --locked -p corelink-gc --bin gc_sweep;";
    let stage = "cp /build/target/release/gc_sweep /out/gc_sweep";
    let runtime = "COPY --from=builder /out/gc_sweep /usr/local/bin/gc_sweep";

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
        !DOCKERFILE.contains("ENV GC_LIVE_DELETE=true"),
        "the image must never default to live deletion"
    );
    assert!(
        DOCKERFILE.contains("ENTRYPOINT [\"/usr/local/bin/corelink-server\"]"),
        "shipping gc_sweep must not replace the server entrypoint"
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
