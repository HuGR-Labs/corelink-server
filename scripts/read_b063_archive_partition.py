#!/usr/bin/env python3
"""Read-only B-063 partition census. No archive call or paging dispatch.

Exit 0 means three D1 reads found no stuck partition and the exact canary's
active queue replayed or was empty. It does not prove a production R2
write or a completed archive cron. Exit 1 means recovery is still required;
exit 2 means the read or replay is inconclusive.
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

REPLAY_SPEC = importlib.util.spec_from_file_location(
    "verify_b063_archive_replay",
    Path(__file__).with_name("verify_b063_archive_replay.py"),
)
if REPLAY_SPEC is None or REPLAY_SPEC.loader is None:
    raise RuntimeError("B-063 replay verifier is unavailable")
REPLAY = importlib.util.module_from_spec(REPLAY_SPEC)
REPLAY_SPEC.loader.exec_module(REPLAY)
ReplayError = REPLAY.ReplayError
replay = REPLAY.replay

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
    meta = block.get("meta")
    if not isinstance(meta, dict) or meta.get("changed_db") is not False or (
        type(meta.get("rows_written")) is not int or meta["rows_written"] != 0
    ) or meta.get("served_by_primary") is not True:
        raise ValueError("D1 read-only primary provenance is missing or invalid")
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
        "SELECT COUNT(*) AS target_rows, "
        "SUM(CASE WHEN emitted_at IS NOT NULL AND archived_at IS NULL "
        "AND quarantined_at IS NULL THEN 1 ELSE 0 END) AS target_active_rows, "
        "SUM(CASE WHEN archived_at IS NOT NULL THEN 1 ELSE 0 END) AS target_archived_rows, "
        "SUM(CASE WHEN quarantined_at IS NOT NULL THEN 1 ELSE 0 END) AS target_quarantined_rows "
        "FROM audit_outbox "
        f"WHERE tenant_id='{TARGET_TENANT}' AND region='{TARGET_REGION}'",
        account, token,
    )
    if len(control) != 1 or type(control[0].get("target_rows")) is not int or (
        control[0]["target_rows"] < 1
    ):
        raise ValueError("exact target partition control is empty or malformed")
    for field in ("target_active_rows", "target_archived_rows", "target_quarantined_rows"):
        if type(control[0].get(field)) is not int or control[0][field] < 0:
            raise ValueError("exact target partition state is malformed")
    active_rows = query(
        "SELECT id, tenant_id, region, sequence_number, prev_hash, chain_hash, "
        "enqueued_at, canonical_jcs, algorithm_id, epoch_id, link_key_id "
        "FROM audit_outbox "
        f"WHERE tenant_id='{TARGET_TENANT}' AND region='{TARGET_REGION}' "
        "AND emitted_at IS NOT NULL AND archived_at IS NULL "
        "AND quarantined_at IS NULL ORDER BY sequence_number, id LIMIT 10001",
        account, token,
    )
    if len(active_rows) > 10000:
        raise ValueError("exact target replay population exceeds the evidence bound")
    if len(active_rows) != control[0]["target_active_rows"]:
        raise ValueError("exact target replay population is incomplete")
    if active_rows:
        try:
            replay_result = replay({
                "partition": {"tenant_id": TARGET_TENANT, "region": TARGET_REGION},
                "rows": active_rows,
            })
        except ReplayError as exc:
            raise ValueError("exact target replay population is invalid") from exc
        replay_verdict = replay_result["verdict"]
        replay_rows = replay_result["rows"]
        verifying_prefix_rows = replay_result["verifying_prefix_rows"]
        break_reason = (replay_result.get("break") or {}).get("reason")
    else:
        replay_verdict = "empty_active_queue"
        replay_rows = 0
        verifying_prefix_rows = 0
        break_reason = None
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
        "exact_target_archived_rows": control[0]["target_archived_rows"],
        "exact_target_quarantined_rows": control[0]["target_quarantined_rows"],
        "exact_target_active_rows": replay_rows,
        "exact_target_replay_verdict": replay_verdict,
        "exact_target_verifying_prefix_rows": verifying_prefix_rows,
        "exact_target_break_reason": break_reason,
        "exact_target_replay_writes": {"d1": 0, "r2": 0},
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
    zero = all(
        item["failed_partitions"] == 0
        and item["exact_target_replay_verdict"] in ("drainable", "empty_active_queue")
        for item in samples
    )
    receipt = {
        "schema_version": 2,
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "database_id": DATABASE_ID,
        "target_prefix": TARGET_TENANT[:8],
        "target_region": TARGET_REGION, "samples": samples,
        "verdict": "REPLAY_AND_PARTITION_CHECKS_OK" if zero else "RECOVERY_REQUIRED",
        "safety": {
            "operations": "SELECT-only",
            "rows_written": 0,
            "archive_endpoint_invoked": False,
            "external_alert_invoked": False,
            "raw_audit_payloads_persisted": False,
        },
        "redaction": {
            "tenant_ids": "eight-character target prefix only",
            "audit_payloads": "used in-memory for BLAKE3 replay; never emitted",
            "credentials": "omitted",
        },
    }
    print(json.dumps(receipt, sort_keys=True))
    return 0 if zero else 1


if __name__ == "__main__":
    raise SystemExit(main())
