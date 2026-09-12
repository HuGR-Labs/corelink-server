#!/usr/bin/env python3
"""Read-only B-063 partition census. No archive call or paging dispatch.

Exit 0 means no failing partition was observed in three D1 reads, not recovery.
Exit 1 means a failing partition remains; exit 2 means the read is inconclusive.
"""

from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.request

DATABASE_ID = "d64742ea-e102-40b2-a844-ff02e3f94562"  # production CONFIG_DB
TARGET_TENANT = "93da3f7a-984d-4ef9-86e0-b0bd9fd0252e"
TARGET_REGION = "enam"


def query(sql: str, account: str, token: str) -> list[dict]:
    url = f"https://api.cloudflare.com/client/v4/accounts/{account}/d1/database/{DATABASE_ID}/query"
    request = urllib.request.Request(
        url,
        data=json.dumps({"sql": sql, "params": []}).encode(),
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
        method="POST",  # D1's read-only query API uses HTTP POST.
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        if response.status != 200:
            raise ValueError("D1 HTTP failure")
        body = json.load(response)
    if not isinstance(body, dict) or body.get("success") is not True:
        raise ValueError("D1 response did not report success")
    blocks = body.get("result")
    if not isinstance(blocks, list) or len(blocks) != 1:
        raise ValueError("D1 result population is ambiguous")
    block = blocks[0]
    if not isinstance(block, dict) or block.get("success") is not True:
        raise ValueError("D1 result block did not report success")
    rows = block.get("results")
    if not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows):
        raise ValueError("D1 results missing or malformed")
    return rows


def sample(account: str, token: str, now_ms: int) -> dict:
    cutoff = now_ms - 3 * 60 * 60 * 1000
    # Reproduce RB-AUDIT-ARCHIVE-ABSENT §3.3 for ALL partitions. Inlining
    # integer milliseconds avoids D1 REST JSON-number bind ambiguity.
    failures = query(
        "SELECT o.tenant_id, o.region, COUNT(*) AS pending_old, "
        "MIN(o.enqueued_at) AS oldest_pending_ms, "
        "(SELECT MAX(o2.archived_at) FROM audit_outbox o2 "
        "WHERE o2.tenant_id=o.tenant_id AND o2.region=o.region) AS part_last_archived_ms "
        "FROM audit_outbox o WHERE o.emitted_at IS NOT NULL AND o.archived_at IS NULL "
        "AND o.quarantined_at IS NULL AND o.enqueued_at < " + str(cutoff) + " "
        "GROUP BY o.tenant_id, o.region HAVING part_last_archived_ms IS NULL "
        "OR part_last_archived_ms < " + str(cutoff),
        account, token,
    )
    # The detector's eight-character display prefix is NOT an identity.
    control = query(
        "SELECT COUNT(*) AS target_rows FROM audit_outbox "
        f"WHERE tenant_id='{TARGET_TENANT}' AND region='{TARGET_REGION}'",
        account, token,
    )
    if len(control) != 1 or type(control[0].get("target_rows")) is not int or (
        control[0]["target_rows"] < 1
    ):
        raise ValueError("exact target partition control is empty or malformed")
    for row in failures:
        if not isinstance(row.get("tenant_id"), str) or not isinstance(row.get("region"), str) or (
            type(row.get("pending_old")) is not int
        ) or row["pending_old"] < 1:
            raise ValueError("failed-partition row is malformed")
    return {
        "captured_at_ms": now_ms,
        "cutoff_ms": cutoff,
        "failed_partitions": len(failures),
        "exact_target_failed": any(
            row["tenant_id"] == TARGET_TENANT and row["region"] == TARGET_REGION
            for row in failures
        ),
        "exact_target_rows_control": control[0]["target_rows"],
    }


def main() -> int:
    if os.environ.get("OWNER_APPROVED_READONLY") != "1":
        print("B-063: OWNER_APPROVED_READONLY=1 is required", file=sys.stderr)
        return 2
    account = os.environ.get("CF_ACCOUNT_ID", "")
    token = os.environ.get("CF_API_TOKEN", "")
    if not account or not token:
        print("B-063: Cloudflare D1 read credentials are required", file=sys.stderr)
        return 2
    try:
        samples = []
        for ordinal in range(3):
            if ordinal:
                time.sleep(1)
            samples.append(sample(account, token, int(time.time() * 1000)))
    except (ValueError, urllib.error.URLError, TimeoutError, OSError) as exc:
        # Never print provider responses, SQL, tokens, or full tenant UUIDs.
        print(f"B-063: readback indeterminate ({type(exc).__name__})", file=sys.stderr)
        return 2
    zero = all(item["failed_partitions"] == 0 for item in samples)
    print(json.dumps({
        "database_id": DATABASE_ID, "target_prefix": TARGET_TENANT[:8],
        "target_region": TARGET_REGION, "samples": samples,
        "verdict": "NO_FAILED_PARTITIONS_OBSERVED" if zero else "FAILED_PARTITIONS_REMAIN",
    }))
    return 0 if zero else 1


if __name__ == "__main__":
    raise SystemExit(main())
