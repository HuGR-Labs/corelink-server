#!/usr/bin/env python3
"""Prove removal of provider-deferred terminal guards fails the static oracle."""
from pathlib import Path
from tempfile import TemporaryDirectory
import sys

from verify_b072_provider_deferred import REQUIRED, verify

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    sources = {
        "contract": (ROOT / "apps/synthetic-pager-worker/src/contract.ts").read_text(),
        "receiver": (ROOT / "apps/synthetic-pager-worker/src/index.ts").read_text(),
        "scheduler": (ROOT / "worker/src/index_schedule.ts").read_text(),
        "migration": (ROOT / "migrations/d1/0150_b072_provider_deferred_receipts.sql").read_text(),
        "config": (ROOT / "apps/synthetic-pager-worker/wrangler.toml").read_text(),
    }
    mutations = {
        "terminality": ("scheduler", REQUIRED["scheduler terminality"]),
        "correlation": ("scheduler", REQUIRED["scheduler correlation check"]),
        "execution time": ("scheduler", REQUIRED["scheduler execution check"]),
        "serving SHA": ("scheduler", REQUIRED["scheduler serving SHA check"]),
        "scheduler receiver revision non-empty": ("scheduler", REQUIRED["scheduler receiver revision non-empty"]),
        "scheduler receiver revision max length": ("scheduler", REQUIRED["scheduler receiver revision max length"]),
        "receiver revision": ("receiver", REQUIRED["receiver revision fail-closed"]),
        "idempotent readback": ("receiver", REQUIRED["replay equality"]),
        "audit write": ("receiver", REQUIRED["audit on durable receipt"]),
        "deferred sweep": ("receiver", REQUIRED["provider deferred no-send sweep"]),
        "typed outcome": ("migration", "outcome                 TEXT    NOT NULL CHECK (outcome = 'provider_deferred')"),
    }
    for name, (source_name, needle) in mutations.items():
        mutated = dict(sources)
        if mutated[source_name].count(needle) != 1:
            raise AssertionError(f"{name}: mutation anchor is not unique")
        mutated[source_name] = mutated[source_name].replace(needle, "", 1)
        try:
            verify(mutated["contract"], mutated["receiver"], mutated["scheduler"], mutated["migration"], mutated["config"])
        except AssertionError:
            continue
        raise AssertionError(f"{name}: removed guard unexpectedly passed")
    print(f"B-072 provider-deferred mutation PASS: {len(mutations)}/{len(mutations)} unsafe mutants rejected")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError) as error:
        print(f"B-072 provider-deferred mutation FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
