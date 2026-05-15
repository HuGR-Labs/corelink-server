#!/usr/bin/env python3
"""
audit-gap-detector.py — walks an audit chain export and detects gaps.

Input: a JSON file exported per FORENSICS-GUIDE.md §3.1, shape:

    {
      "results": [
        {
          "id": "<uuidv7>",
          "tenant_id": "<uuidv7>",
          "request_id": "<uuid>",
          "event_type": "corelink.cas.put_completed",
          "payload_json": "{...}",            // the CloudEvent envelope
          "enqueued_at": 1715781600000,
          "emitted_at":  1715781601000
        },
        ...
      ]
    }

Or a bare list shape (`[{...}, {...}]`) — both accepted.

The chain hash + sequence_number live INSIDE `payload_json` (see
`crates/corelink-audit-chain/src/event.rs` and §3 of the guide). This
script:

    1. Parses every event's payload_json.
    2. Verifies sequence_number is strictly +1 monotonic per tenant.
    3. Verifies prev_hash of event N == hash of event N-1.
    4. Emits a JSON report of every gap / out-of-order / hash-mismatch.

Exit codes:
    0 — chain is gap-free and hash-contiguous
    1 — gaps or violations found (report on stdout)
    2 — invocation / parse error

Usage:
    python3 scripts/forensics/audit-gap-detector.py <export.json>

NOTE: this script does NOT verify cryptographic hashes — it only checks
ordering + contiguity. Cryptographic verification is the job of
`corelink-audit-chain::verifier` (see `crates/corelink-audit-chain/src/
verifier.rs`); run that separately if you need it.
"""

from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path


def load_events(path: Path) -> list[dict]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(raw, dict) and "results" in raw:
        events = raw["results"]
    elif isinstance(raw, list):
        events = raw
    else:
        raise SystemExit(
            "error: unrecognized export shape; expected list or "
            "{results: [...]}"
        )
    if not isinstance(events, list):
        raise SystemExit("error: 'results' must be a list of event rows")
    return events


def parse_payload(raw: str) -> dict:
    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        raise SystemExit(f"error: payload_json failed to parse: {exc}") from exc


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2

    path = Path(sys.argv[1])
    if not path.exists():
        print(f"error: file not found: {path}", file=sys.stderr)
        return 2

    events = load_events(path)
    if not events:
        print(json.dumps({"ok": True, "events": 0, "findings": []}))
        return 0

    # Group by tenant — each tenant has its own chain head + sequence
    # space.
    by_tenant: dict[str, list[dict]] = defaultdict(list)
    for ev in events:
        payload = parse_payload(ev["payload_json"])
        by_tenant[ev["tenant_id"]].append(
            {
                "id": ev["id"],
                "enqueued_at": ev["enqueued_at"],
                "sequence_number": payload.get("sequence_number"),
                "prev_hash": payload.get("prev_hash"),
                "this_hash": payload.get("this_hash") or payload.get("hash"),
                "event_type": ev.get("event_type"),
            }
        )

    findings: list[dict] = []

    for tenant_id, evs in by_tenant.items():
        # Sort by enqueued_at then sequence_number for deterministic walk.
        evs.sort(key=lambda e: (e["enqueued_at"], e["sequence_number"] or 0))

        prev_seq: int | None = None
        prev_hash_announced: str | None = None
        for ev in evs:
            seq = ev["sequence_number"]
            if seq is None:
                findings.append({
                    "tenant_id": tenant_id,
                    "kind": "missing_sequence_number",
                    "event_id": ev["id"],
                })
                continue

            if prev_seq is not None:
                expected = prev_seq + 1
                if seq < expected:
                    findings.append({
                        "tenant_id": tenant_id,
                        "kind": "out_of_order",
                        "event_id": ev["id"],
                        "expected_seq": expected,
                        "actual_seq": seq,
                    })
                elif seq > expected:
                    findings.append({
                        "tenant_id": tenant_id,
                        "kind": "gap",
                        "after_event_id": ev["id"],
                        "missing_seq_range": [expected, seq - 1],
                    })

            if prev_hash_announced is not None and ev["prev_hash"] is not None:
                if ev["prev_hash"] != prev_hash_announced:
                    findings.append({
                        "tenant_id": tenant_id,
                        "kind": "prev_hash_mismatch",
                        "event_id": ev["id"],
                        "expected_prev_hash": prev_hash_announced,
                        "actual_prev_hash": ev["prev_hash"],
                    })

            prev_seq = seq
            prev_hash_announced = ev["this_hash"]

    report = {
        "ok": not findings,
        "events": len(events),
        "tenants": len(by_tenant),
        "findings": findings,
    }
    print(json.dumps(report, indent=2))
    return 0 if not findings else 1


if __name__ == "__main__":
    sys.exit(main())
