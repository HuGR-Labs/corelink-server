#!/usr/bin/env python3
"""Read-only B-125 head/tail integrity verifier.

Unlike the retired aggregate, a checkpoint matching one of two maximum rows is
not a pass: a future drain cannot choose that row by SQLite order. The query
reports ambiguous maximum tails independently from sequence/hash mismatch.

Exit status: 0 clean; 1 integrity defect; 2 indeterminate evidence.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, NoReturn


HEAD_TAIL_SQL = """
WITH max_sequence AS (
    SELECT tenant_id, region, MAX(sequence_number) AS max_sequence
    FROM audit_outbox
    WHERE emitted_at IS NOT NULL AND sequence_number IS NOT NULL
    GROUP BY tenant_id, region
), tails AS (
    SELECT
        m.tenant_id,
        m.region,
        m.max_sequence,
        COUNT(o.id) AS tail_rows,
        SUM(o.chain_hash = h.head_hash) AS head_hash_matches
    FROM max_sequence AS m
    JOIN audit_outbox AS o
      ON o.tenant_id = m.tenant_id
     AND o.region = m.region
     AND o.sequence_number = m.max_sequence
     AND o.emitted_at IS NOT NULL
    JOIN audit_chain_head AS h
      ON h.tenant_id = m.tenant_id AND h.region = m.region
    GROUP BY m.tenant_id, m.region, m.max_sequence
), classified AS (
    SELECT
        h.tenant_id,
        h.region,
        h.next_sequence,
        t.max_sequence,
        t.tail_rows,
        t.head_hash_matches
    FROM audit_chain_head AS h
    LEFT JOIN tails AS t
      ON t.tenant_id = h.tenant_id AND t.region = h.region
)
SELECT
    COUNT(*) AS heads,
    SUM(max_sequence IS NULL) AS missing_tail_partitions,
    SUM(COALESCE(tail_rows, 0) > 1) AS ambiguous_tail_partitions,
    SUM(max_sequence IS NOT NULL AND next_sequence != max_sequence + 1)
        AS sequence_mismatch_partitions,
    SUM(max_sequence IS NOT NULL AND COALESCE(head_hash_matches, 0) != 1)
        AS hash_mismatch_partitions
FROM classified
""".strip()

FIELDS = (
    "heads",
    "missing_tail_partitions",
    "ambiguous_tail_partitions",
    "sequence_mismatch_partitions",
    "hash_mismatch_partitions",
)


class Indeterminate(ValueError):
    pass


def _parse(payload: object) -> dict[str, int]:
    if not isinstance(payload, dict) or payload.get("success") is not True:
        raise Indeterminate("D1 response is missing success=true")
    result = payload.get("result")
    if not isinstance(result, list) or len(result) != 1:
        raise Indeterminate("D1 response must contain exactly one result set")
    result_set = result[0]
    if not isinstance(result_set, dict) or result_set.get("success") is not True:
        raise Indeterminate("D1 result set is missing success=true")
    rows = result_set.get("results")
    if not isinstance(rows, list) or len(rows) != 1 or not isinstance(rows[0], dict):
        raise Indeterminate("D1 response must contain exactly one aggregate row")
    parsed: dict[str, int] = {}
    for field in FIELDS:
        value = rows[0].get(field)
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise Indeterminate(f"{field} is missing or not a non-negative integer")
        parsed[field] = value
    if parsed["heads"] == 0:
        raise Indeterminate("audit_chain_head population is empty")
    return parsed


def assess(counts: dict[str, int]) -> tuple[str, str]:
    defects = sum(counts[field] for field in FIELDS[1:])
    if defects:
        return "FAILED", "missing, ambiguous, sequence-divergent, or hash-divergent tails exist"
    return "COMPLIANT", "every head has exactly one matching maximum tail"


def _live(account_id: str, token: str, database_id: str, timeout: int) -> object:
    request = urllib.request.Request(
        f"https://api.cloudflare.com/client/v4/accounts/{account_id}/d1/database/{database_id}/query",
        data=json.dumps({"sql": HEAD_TAIL_SQL}).encode(),
        method="POST",
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310
            return json.loads(response.read().decode())
    except (OSError, urllib.error.URLError, json.JSONDecodeError) as error:
        raise Indeterminate(f"live D1 query unavailable or malformed: {error}") from error


def _stop(message: str) -> NoReturn:
    print(f"status=INDETERMINATE reason={message}", file=sys.stderr)
    raise SystemExit(2)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--database-id")
    parser.add_argument("--account-id", default=os.environ.get("CLOUDFLARE_ACCOUNT_ID"))
    parser.add_argument("--timeout-seconds", type=int, default=20)
    args = parser.parse_args(argv)
    if bool(args.input) == bool(args.database_id):
        _stop("choose exactly one evidence source: --input or --database-id")
    if not 1 <= args.timeout_seconds <= 60:
        _stop("timeout must be between 1 and 60 seconds")
    try:
        if args.input:
            payload: Any = json.loads(args.input.read_text(encoding="utf-8"))
        else:
            api_token = os.environ.get("CLOUDFLARE_API_TOKEN")
            if not args.account_id or not api_token:
                raise Indeterminate("live query requires account ID and API token")
            payload = _live(args.account_id, api_token, args.database_id, args.timeout_seconds)
        counts = _parse(payload)
        status, reason = assess(counts)
    except (OSError, json.JSONDecodeError, Indeterminate) as error:
        _stop(str(error))
    print(json.dumps({"status": status, "reason": reason, "counts": counts}, sort_keys=True))
    return 0 if status == "COMPLIANT" else 1


if __name__ == "__main__":
    raise SystemExit(main())
