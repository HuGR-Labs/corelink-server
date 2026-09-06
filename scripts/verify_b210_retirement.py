#!/usr/bin/env python3
"""Verify the truthful retired state of B-210 after the B-119 decision.

B-119 deliberately removed the unbound ``/admin/ops*`` UI surface.  Therefore
B-210 cannot be closed by restoring a page that would re-publish that phantom
API.  This small local gate makes that reconciliation executable: B-210 must
remain parked, the B-119 retirement must remain done, and the retired queue
page must stay absent.  Its self-test mutates each load-bearing fact in an
isolated temporary tree and requires every mutation to fail closed.
"""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OPS_PAGE = Path("apps/admin-ui/src/app/[locale]/(authenticated)/admin/ops/page.tsx")


class RetirementError(ValueError):
    pass


def _read(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise RetirementError(f"missing/non-regular evidence: {relative}")
    return path.read_text(encoding="utf-8")


def _backlog_records(root: Path):
    spec = importlib.util.spec_from_file_location("backlog_verify_b210", root / "scripts/backlog_verify.py")
    if spec is None or spec.loader is None:
        raise RetirementError("backlog parser unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    try:
        return {record.id: record.raw for record in module.parse(_read(root, Path("BACKLOG.md")))}
    except (ValueError, TypeError) as exc:
        raise RetirementError(f"invalid BACKLOG: {exc}") from exc


def verify(root: Path = ROOT) -> dict[str, str]:
    records = _backlog_records(root)
    b210 = records.get("B-210")
    b119 = records.get("B-119")
    if b210 is None or b119 is None:
        raise RetirementError("B-210/B-119 backlog records are required")
    if b210.get("status") != "parked":
        raise RetirementError("B-210 must be parked while the admin-operation surface is retired")
    if b119.get("status") != "done":
        raise RetirementError("B-119 must remain done: the admin-operation surface is retired")
    verify_command = str(b210.get("verify", ""))
    if "verify_b210_retirement.py" not in verify_command:
        raise RetirementError("B-210 verify command does not name the retirement gate")
    means = str(b210.get("verify-means", ""))
    if not means.lstrip().lower().startswith("parked —"):
        raise RetirementError("B-210 verify-means must describe the parked state")
    if "B-119" not in means or "retir" not in means.lower():
        raise RetirementError("B-210 verify-means does not record the B-119 retirement decision")
    page = root / OPS_PAGE
    if page.exists() or page.is_symlink():
        raise RetirementError(f"retired admin-operation page is present: {OPS_PAGE}")
    return {"B-210": "parked", "B-119": "done"}


def _expect_failure(root: Path, label: str) -> None:
    try:
        verify(root)
    except (RetirementError, OSError, UnicodeDecodeError, ImportError):
        return
    raise RetirementError(f"self-test mutation escaped: {label}")


def self_test() -> int:
    import tempfile

    backlog = """### B-119 — retired admin operations\n\n```backlog\nid: B-119\nrepo: corelink-server\nowner: tl\nstatus: done\nverify: python3 scripts/verify_b119_surface.py\nverify-means: done — retired\nlast-verified: 2026-09-06\n```\n\n### B-210 — stale SSR finding\n\n```backlog\nid: B-210\nrepo: corelink-server\nowner: tl\nstatus: parked\nverify: python3 scripts/verify_b210_retirement.py\nverify-means: parked — B-119 retired the admin/ops surface\nlast-verified: 2026-09-06\n```\n"""
    with tempfile.TemporaryDirectory(prefix="b210-retirement-") as raw:
        root = Path(raw)
        (root / "scripts").mkdir()
        (root / "scripts/backlog_verify.py").write_text(
            "def parse(text):\n"
            "    class Record:\n"
            "        def __init__(self, ident, raw): self.id, self.raw = ident, raw\n"
            "    import re\n"
            "    out = []\n"
            "    for body in re.findall(r'```backlog\\n(.*?)```', text, re.S):\n"
            "        fields = dict(line.split(': ', 1) for line in body.splitlines() if ': ' in line)\n"
            "        out.append(Record(fields['id'], fields))\n"
            "    return out\n",
            encoding="utf-8",
        )
        (root / "BACKLOG.md").write_text(backlog, encoding="utf-8")
        verify(root)

        (root / OPS_PAGE).parent.mkdir(parents=True)
        (root / OPS_PAGE).write_text("// stale route", encoding="utf-8")
        _expect_failure(root, "restored route")
        (root / OPS_PAGE).unlink()

        (root / "BACKLOG.md").write_text(backlog.replace("status: parked", "status: done", 1), encoding="utf-8")
        _expect_failure(root, "done status")
        (root / "BACKLOG.md").write_text(backlog.replace("status: done", "status: parked", 1), encoding="utf-8")
        _expect_failure(root, "B-119 retirement reversal")
    print("B-210 retirement self-test: clean state and three fail-closed mutations passed")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test()
        verify()
    except (RetirementError, OSError, UnicodeDecodeError, ImportError) as exc:
        print(f"B-210 retirement gate: FAIL: {exc}", file=sys.stderr)
        return 1
    print("B-210 retirement gate: PASS: parked with B-119 retirement and no admin/ops page")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
