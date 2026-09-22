"""Static contract checks for the B-125 read-only D1 evidence lane."""

from __future__ import annotations

import json
import re
import sqlite3
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/b125-audit-throughput-read-only.yml"
FIXTURE_NAMES = (
    "b125-d1-aggregate-direct.json",
    "b125-d1-aggregate-wrangler.json",
)
REQUIRED_COLUMNS = {
    "window_start_utc",
    "window_end_utc",
    "arrivals",
    "sealed_rows",
}


def _result_blocks(document: object) -> list[dict[str, object]]:
    """Mirror the workflow's accepted Wrangler/D1 result envelopes."""
    if isinstance(document, list):
        blocks = document
    elif isinstance(document, dict):
        result = document.get("result")
        if isinstance(result, list):
            blocks = result
        elif isinstance(result, dict):
            blocks = [result]
        elif isinstance(document.get("results"), (list, dict)):
            blocks = [document]
        else:
            blocks = []
    else:
        blocks = []
    if isinstance(blocks, dict):
        blocks = [blocks]
    assert isinstance(blocks, list) and blocks
    return blocks  # type: ignore[return-value]


def _rows(document: object) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for block in _result_blocks(document):
        assert block.get("success") is not False
        result_rows = block.get("results") or []
        assert isinstance(result_rows, list)
        rows.extend(result_rows)  # type: ignore[arg-type]
    return rows


def _hourly_sql() -> str:
    source = WORKFLOW.read_text(encoding="utf-8")
    match = re.search(r"^\s*SQL\[hourly\]='([^']+)'$", source, re.MULTILINE)
    assert match, "hourly SQL must remain an explicit shell allowlist entry"
    return match.group(1)


def test_hourly_sql_is_one_select_only_aggregate_with_six_offsets() -> None:
    sql = _hourly_sql()
    assert sql.startswith("SELECT ")
    assert ";" not in sql
    assert not re.search(r"\b(?:INSERT|UPDATE|DELETE|REPLACE|DROP|ALTER|CREATE|PRAGMA|ATTACH|DETACH)\b", sql)
    assert "aggregate_counts AS" in sql
    assert sql.count("COUNT(CASE WHEN") == 12
    assert {int(value) for value in re.findall(r"arrivals_(\d)", sql)} == set(range(6))
    assert {int(value) for value in re.findall(r"sealed_(\d)", sql)} == set(range(6))


def test_hourly_sql_executes_as_six_row_sqlite_aggregate() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("CREATE TABLE audit_outbox (enqueued_at INTEGER, emitted_at INTEGER)")
    connection.executemany(
        "INSERT INTO audit_outbox VALUES (?, ?)",
        [(0, 0), (None, None), (1, 1)],
    )
    result = connection.execute(_hourly_sql()).fetchall()
    assert len(result) == 6
    assert all(len(row) == 4 for row in result)


def test_aggregate_fixtures_preserve_six_row_redacted_shape() -> None:
    expected: list[dict[str, object]] | None = None
    for name in FIXTURE_NAMES:
        document = json.loads((ROOT / "tests/fixtures" / name).read_text(encoding="utf-8"))
        rows = _rows(document)
        assert len(rows) == 6
        assert all(set(row) == REQUIRED_COLUMNS for row in rows)
        assert all(isinstance(row["arrivals"], int) and row["arrivals"] >= 0 for row in rows)
        assert all(isinstance(row["sealed_rows"], int) and row["sealed_rows"] >= 0 for row in rows)
        if expected is None:
            expected = rows
        else:
            assert rows == expected


def test_workflow_keeps_four_query_ids_and_remote_only_execution() -> None:
    source = WORKFLOW.read_text(encoding="utf-8")
    assert "for query_id in population hourly latency heads; do" in source
    assert "--remote --command \"$sql\" --json" in source
    assert "--local" not in source
    assert "contents: read" in source
    assert "persist-credentials: false" in source
