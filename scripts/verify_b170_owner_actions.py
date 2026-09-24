#!/usr/bin/env python3
"""Fail-closed guard for the three external B-170 owner artifacts.

The repository can verify that the owner packet and its canonical evidence
paths are present, regular, and non-empty.  It cannot validate a legal
disposition, a PagerDuty export, or a notification decision without the
external account/contract context.  Missing evidence therefore reports the
truthful ``open`` state; three present artifacts deliberately report
``DRIFTED`` so an owner must review their contents and invert this gate.
"""

from __future__ import annotations

import argparse
import json
import stat
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PACKET = Path("docs/internal/b087-questionnaire-owner-actions.md")
RESIDENCY_TEMPLATE = Path("legal/dpa-residency-amendment.md")
EVIDENCE = (
    Path("reports/owner-actions/b170-legal-contract-review.md"),
    Path("reports/owner-actions/b170-pagerduty-export.json"),
    Path("reports/owner-actions/b170-recipient-notification-decision.md"),
)
PACKET_MARKERS = (
    "Legal's disposition",
    "Operations' redacted",
    "`reports/owner-actions/b170-recipient-notification-decision.md` — Sales/Legal",
    "no notification is claimed here",
    "executed DPA/SLA claims and the pending residency template",
    "docs/customer/dpa-onboarding.md",
    "has executed its DPA with a lighthouse enterprise customer",
    *tuple(path.as_posix() for path in EVIDENCE),
)


class PacketError(RuntimeError):
    """The owner packet or an evidence path is absent or unsafe."""


def _checked_path(
    root: Path, relative: Path, label: str, *, allow_missing: bool
) -> Path | None:
    """Walk every component without following a repository path symlink."""

    if relative.is_absolute() or any(part in {"", ".", ".."} for part in relative.parts):
        raise PacketError(f"{label}: unsafe relative path: {relative}")
    current = root
    for index, part in enumerate(relative.parts):
        current /= part
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            if allow_missing:
                return None
            raise PacketError(f"{label}: missing/non-regular artifact: {relative}") from None
        except OSError as exc:
            raise PacketError(f"{label}: cannot inspect path component {current}: {exc}") from exc
        if stat.S_ISLNK(metadata.st_mode):
            raise PacketError(f"{label}: symlink path component is not allowed: {current}")
        if index < len(relative.parts) - 1 and not stat.S_ISDIR(metadata.st_mode):
            raise PacketError(f"{label}: path component is not a directory: {current}")
    return current


def _regular_text(root: Path, relative: Path, label: str) -> str:
    path = _checked_path(root, relative, label, allow_missing=False)
    assert path is not None
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode):
        raise PacketError(f"{label}: artifact is not a regular file: {relative}")
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise PacketError(f"{label}: unreadable artifact {relative}: {exc}") from exc
    if not text.strip():
        raise PacketError(f"{label}: artifact is empty: {relative}")
    return text


def _packet_text(root: Path) -> str:
    text = _regular_text(root, PACKET, "B-170 owner packet")
    missing = [marker for marker in PACKET_MARKERS if text.count(marker) != 1]
    if missing:
        raise PacketError(
            "B-170 owner packet lost or duplicated canonical action markers: "
            + ", ".join(missing)
        )
    template = _regular_text(root, RESIDENCY_TEMPLATE, "B-170 residency template")
    front_matter = template.split("---", 2)
    if (
        len(front_matter) < 3
        or 'doc_status: "PENDING_LEGAL_REVIEW"' not in front_matter[1]
        or "NOT a finalised legal instrument" not in template
    ):
        raise PacketError("B-170 residency instrument status changed; re-evaluate owner packet")
    return text


def verify(root: Path = ROOT) -> dict[str, object]:
    """Return the repository-visible B-170 state without validating owner acts."""

    _packet_text(root)
    missing: list[str] = []
    for relative in EVIDENCE:
        path = _checked_path(root, relative, "B-170 evidence", allow_missing=True)
        if path is None:
            missing.append(relative.as_posix())
            continue
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode):
            raise PacketError(f"B-170 evidence is not a regular file: {relative}")
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as exc:
            raise PacketError(f"B-170 evidence is unreadable: {relative}: {exc}") from exc
        if not text.strip():
            raise PacketError(f"B-170 evidence is empty: {relative}")

    if missing:
        return {
            "status": "open",
            "missing": missing,
            "non_claim": "No legal, operations, sales, notification, or contract outcome is inferred.",
        }
    return {
        "status": "DRIFTED",
        "missing": [],
        "non_claim": "Presence is not validation; owner must review all three artifacts and transition B-170.",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = verify(Path(args.root).resolve())
    except PacketError as exc:
        print(f"INSTRUMENT BROKEN: {exc}", file=sys.stderr)
        return 2
    if args.json:
        print(json.dumps(result, indent=2))
    elif result["status"] == "open":
        print("B-170 open: owner evidence pending: " + ", ".join(result["missing"]))
    else:
        print(
            "B-170 DRIFTED: all three owner artifacts exist; validate their contents, "
            "close B-170, and invert this guard",
            file=sys.stderr,
        )
    return 0 if result["status"] == "open" else 1


if __name__ == "__main__":
    raise SystemExit(main())
