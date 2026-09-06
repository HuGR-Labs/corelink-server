#!/usr/bin/env python3
"""Offline gate for a parked D03 item.

External/runtime actions are never launched by the graduation validator.  This
gate proves the parked packet and current ``verify-means`` declaration are
present, leaving the action itself for the named owner.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.verify_d03_graduation import GRADUATED_SET, _load_packets, _read, parse, PACKET_PATH, ROOT


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--id", required=True, choices=sorted(GRADUATED_SET))
    args = parser.parse_args()
    try:
        records = {record.id: record for record in parse(_read(ROOT / "BACKLOG.md"))}
        packet = _load_packets(_read(PACKET_PATH))["packets"][args.id]
        record = records[args.id]
        if record.raw.get("status") != "parked" or packet.get("disposition") != "PARKED":
            raise ValueError("status/disposition is not parked")
        if not str(record.raw.get("verify-means", "")).lstrip().lower().startswith("parked —"):
            raise ValueError("verify-means is not parked")
        print(f"{args.id} parked offline guard: PASS; external action not dispatched")
        return 0
    except (KeyError, OSError, ValueError) as exc:
        print(f"{args.id} parked offline guard: FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
