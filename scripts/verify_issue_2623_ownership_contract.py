#!/usr/bin/env python3
"""Fail-closed source boundary guard for issue #2623's shared contract."""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/corelink-container/src/storage/staging_load_test_ownership.rs").read_text()
ADMISSION = (ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs").read_text()
TS = (ROOT / "apps/signup-worker/src/staging_load_test_ownership.ts").read_text()
MIGRATION = (ROOT / "migrations/d1/0149_staging_load_test_r2_intents.sql").read_text()

required = {
    "private Rust registration fields": "    run_id: &'a str," in RUST and "    pub run_id: &'a str," not in RUST,
    "context-only Rust constructor": "fn ownership_registration<'a>(" in RUST,
    "D1 statement composition": "fn d1_statement(" in RUST and "SQL_REGISTER_RESOURCE_IN_BATCH" in RUST,
    "R2 durable prepare": "fn prepare_statement(" in RUST and "SQL_PREPARE_R2_INTENT" in RUST,
    "R2 atomic commit pair": "fn commit_statements(" in RUST and "SQL_COMMIT_R2_INTENT" in RUST,
    "worker envelope signer domain": 'b"corelink/staging-ownership-envelope/v1\\0"' in ADMISSION,
    "worker bounded verifier": "MAX_ENVELOPE_BYTES = 512" in TS and "crypto.subtle.verify" in TS,
    "worker exact request binding": "requestId !== expectedRequestId" in TS,
    "worker verified-object guard": "verifiedContexts.has(context)" in TS,
    "worker D1 statement composition": "ownershipInsertStatement(" in TS and "D1PreparedStatement" in TS,
    "R2 immutable intent": "trg_staging_load_test_r2_intent_identity_immutable" in MIGRATION,
    "R2 resource-before-commit": "trg_staging_load_test_r2_intent_commit_requires_resource" in MIGRATION,
    "R2 resolved-before-seal": "trg_staging_load_test_run_seal_requires_resolved_r2" in MIGRATION,
    "R2 append-only": "trg_staging_load_test_r2_intent_no_delete" in MIGRATION,
}
missing = [name for name, present in required.items() if not present]
if missing:
    print("issue-2623 contract verification failed: " + ", ".join(missing), file=sys.stderr)
    raise SystemExit(1)
print("issue-2623 ownership contract verification: PASS")
