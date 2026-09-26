"""Source guard for the #2579 CAS/Cargo ownership writer boundary."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def verify() -> None:
    fence = read("crates/corelink-container/src/storage/cas_write_fence.rs")
    r2 = read("crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs")
    cas_route = read("crates/corelink-container/src/routes/cas/foundation_core.rs")
    cas_single = read("crates/corelink-container/src/routes/cas/single_handlers.rs")
    cas_batch = read("crates/corelink-container/src/routes/cas/batch_write.rs")
    cargo_route = read("crates/corelink-container/src/routes/cargo/part-00-01.rs")

    # Closed class census: every applicable route enters the same native writer.
    assert "cas_reference_registration" in fence
    assert "StagingLoadTestResourceClass::CasReference" in fence
    assert "StagingLoadTestDisposition::Retained" in fence
    assert "StagingLoadTestScenario::Cas | StagingLoadTestScenario::B103CargoWrite" in fence
    assert "corelink/staging-cas-reference/v1\\0" in fence
    assert 'opaque.push_str("cas-reference:")' in fence
    assert '"cas-reference:{}:{}:{}:{}"' not in fence
    assert "fn handle_write(" in cas_single and "write_with_effect_and_context(req, staging_context)" in cas_single
    assert ".write_with_effect_and_context(req, staging_context.clone())" in cas_batch
    assert "StagingCasWriteContext::new(context)" in cargo_route
    assert "staging_admission_context(context)" in r2
    assert "fence.begin(" in r2 and "fence.commit(" in r2

    # A supplied route claim is verified and durably consumed before writer entry.
    assert "admit_staging_load_test_request(" in cas_route
    assert "StagingLoadTestScenario::Cas" in cas_route
    assert "invalid staging admission" in cas_single and "invalid staging admission" in cas_batch
    assert "staging_context.is_some()" in r2

    # The R2 path prepares before mutation and commits ownership with metadata;
    # transaction aborts are forced for a stale/missing lease and DB triggers
    # require the canonical registered resource before intent commit.
    assert "intent.prepare_statement(prepared_at_ms)" in fence
    assert "fn reusable_staging_operation_id(" in fence
    assert "state IN ('prepared', 'committed')" in fence
    assert "retry_after_r2_failure_reuses_verified_durable_handle_intent" in fence
    assert "D1BatchStatement::new(metadata_sql, metadata_params)" in fence
    assert "WHERE changes() = 0 OR NOT EXISTS" in fence
    assert "register_resource" in fence and "close_intent" in fence
    assert "state = 'committed', committed_at_ms = ?1" in fence
    assert "None, None) => self.query_sync(metadata_sql, &metadata_params)" in fence
    assert "'invalid'" in fence  # check-constraint violation forces batch rollback

    # No raw admission header or free identity constructor is forwarded to storage.
    assert "STAGING_LOAD_TEST_ADMISSION_HEADER" not in r2
    assert "let context = StagingLoadTestAdmissionContext {" not in fence


if __name__ == "__main__":
    verify()
    print("#2579 CAS/Cargo ownership source guard passed")
