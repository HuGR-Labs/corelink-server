"""SEV-0 alert routing must preserve partition-specific page identity."""

from pathlib import Path

import pytest

from scripts.verify_b063_hosted_evidence_workflow import verify_alert_contract


MONITOR = Path(__file__).parents[1] / ".github/workflows/audit-archive-lag.yml"


def test_partition_and_absence_pages_keep_distinct_dedup_keys() -> None:
    verify_alert_contract(MONITOR.read_text(encoding="utf-8"))


def test_alert_contract_rejects_collapsed_partition_dedup_key() -> None:
    source = MONITOR.read_text(encoding="utf-8")
    collapsed = source.replace(
        'dedup = "audit-archive-partition-failure-" + os.environ["RUN_DATE"]',
        'dedup = "audit-archive-absent-" + os.environ["RUN_DATE"]',
        1,
    )
    with pytest.raises(AssertionError, match="SEV-0 alert contract"):
        verify_alert_contract(collapsed)
