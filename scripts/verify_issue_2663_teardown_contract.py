#!/usr/bin/env python3
"""Static fail-closed guard for #2663's frozen teardown schema."""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations/d1/0151_staging_load_test_teardown_receipts.sql"
WORKFLOW = ROOT / ".github/workflows/issue-2663-teardown-contract.yml"

schema = MIGRATION.read_text()
workflow = WORKFLOW.read_text()
required_schema = (
    "staging_load_test_teardown_locators",
    "FOREIGN KEY (run_id, scenario, resource_class, receipt_ref)",
    "webhook_inbox_v1",
    "webhook_effect_v1",
    "dsr_r2_export_v1",
    "signup_pilot_v1",
    "byok_pending_synthetic_v1",
    "trg_staging_load_test_teardown_locator_kind_matches_class",
    "staging_load_test_synthetic_tenants",
    "baseline_marker = 'generation_zero_empty'",
    "UNIQUE (tenant_id)",
    "staging_load_test_prepared_intent_reconciliations",
    "'cas_reference', 'dsr_artifact', 'audit_evidence', 'byok_artifact'",
    "readback_outcome IN ('absent', 'deleted', 'quarantined')",
    "staging_load_test_teardown_receipt_counts",
    "attempted_count INTEGER NOT NULL CHECK (attempted_count >= 0 AND attempted_count <= inventory_count)",
    "staging_load_test_teardown_receipts",
    "corelink.staging-load-test-teardown-receipt.v2",
    "terminal_state IN ('reconciled', 'failed', 'quarantined')",
    "NEW.terminal_state IN ('failed', 'quarantined')",
    "trg_staging_load_test_reconciled_receipt_requires_complete_readback",
    "disposition = 'disposable' AND state <> 'deleted'",
    "receipt_count.inventory_count <> scan.observed_count",
    "trg_staging_load_test_terminal_receipt_immutable",
)
required_workflow = (
    "pull_request:",
    "contents: read",
    "github.event.pull_request.head.repo.full_name == github.repository",
    "ref: ${{ github.event.pull_request.head.sha }}",
    "EXPECTED_HEAD: ${{ github.event.pull_request.head.sha }}",
    "scripts/check_migration_prefixes.py",
    "scripts/check_migrations_additive.py",
    "scripts/verify_issue_2663_teardown_contract.py",
    "tests/test_issue_2663_teardown_contract.py",
)
missing = [value for value in required_schema if value not in schema]
missing += [value for value in required_workflow if value not in workflow]
if missing:
    print("issue-2663 teardown contract verification failed: " + ", ".join(missing), file=sys.stderr)
    raise SystemExit(1)
print("issue-2663 teardown contract verification: PASS")
