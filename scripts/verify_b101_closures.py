#!/usr/bin/env python3
"""Canonical dispatcher for inverted B-101 implementation closure gates.

The backlog records the exact command emitted here.  ``--expect open`` is
intentionally rejected: an OPEN-state census and a positive closure witness
are different polarities and must never be interchangeable.
"""

from __future__ import annotations

import argparse
import runpy
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GROUPS = {
    **{f"B-{n}": "verify_b171_180_closures.py" for n in range(171, 181)},
    **{f"B-{n}": "verify_b215_b223_contracts.py" for n in range(215, 224)},
    **{f"B-{n}": "verify_b224_b230_contracts.py" for n in range(224, 231)},
}


class ClosureDispatchError(ValueError):
    """An invalid polarity or an unavailable closure witness."""


def verify(root: Path = ROOT, identifier: str | None = None) -> dict[str, str]:
    selected = (identifier,) if identifier else tuple(GROUPS)
    for item in selected:
        script = GROUPS.get(item)
        if script is None:
            raise ClosureDispatchError(f"unknown closure id: {item}")
        namespace = runpy.run_path(str(root / "scripts" / script))
        try:
            namespace["verify"](root, item)
        except KeyError as exc:
            raise ClosureDispatchError(f"{item}: closure script has no verifier") from exc
    return {item: "pass" for item in selected}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--id")
    parser.add_argument("--expect", choices=("done", "open"), required=True)
    args = parser.parse_args(argv)
    try:
        if args.expect != "done":
            raise ClosureDispatchError("closure verifier is inverted: open is not a closed result")
        result = verify(Path(args.root).resolve(), args.id)
    except (ClosureDispatchError, OSError, UnicodeError, ValueError) as exc:
        print(f"B-101 closure dispatcher: FAIL: {exc}", file=sys.stderr)
        return 1
    print("B-101 closure dispatcher: PASS: " + ", ".join(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
