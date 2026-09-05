"""Bounded parity checks for localized generated API reference trees."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parent.parent
LOCALIZED_API_ROOTS = tuple(
    (
        locale,
        REPO_ROOT / "apps" / "docs" / "i18n" / locale /
        "docusaurus-plugin-content-docs" / "current" / "reference" / "api",
    )
    for locale in ("de", "es-419", "pt-BR")
)
MAX_LOCALIZED_INDEX_BYTES = 1 << 20
MAX_LOCALIZED_ENDPOINT_FILES = 1_000
ENDPOINT_LINK_PATTERN = re.compile(r"\]\(\./endpoints/([A-Za-z0-9_.-]+\.mdx)\)")


def validate_localized_api_indexes(expected_files: Iterable[str]) -> list[str]:
    """Require each localized landing page to be an exact, bounded mirror."""
    expected = set(expected_files)
    failures: list[str] = []
    for locale, api_root in LOCALIZED_API_ROOTS:
        index_path = api_root / "index.mdx"
        if not index_path.is_file():
            failures.append(f"{locale}: missing localized API index: {index_path}")
            continue
        try:
            if index_path.stat().st_size > MAX_LOCALIZED_INDEX_BYTES:
                failures.append(f"{locale}: localized API index exceeds {MAX_LOCALIZED_INDEX_BYTES} bytes")
                continue
            index = index_path.read_text(encoding="utf-8")
        except OSError as exc:
            failures.append(f"{locale}: cannot read localized API index: {exc}")
            continue
        links = ENDPOINT_LINK_PATTERN.findall(index)
        if len(links) > MAX_LOCALIZED_ENDPOINT_FILES:
            failures.append(f"{locale}: localized API index exceeds {MAX_LOCALIZED_ENDPOINT_FILES} endpoint links")
            continue
        linked = set(links)
        if len(linked) != len(links):
            failures.append(f"{locale}: localized API index contains duplicate endpoint links")
        missing_links = sorted(expected - linked)
        extra_links = sorted(linked - expected)
        if missing_links:
            failures.append(f"{locale}: localized API index is missing {', '.join(missing_links)}")
        if extra_links:
            failures.append(f"{locale}: localized API index has unexpected {', '.join(extra_links)}")
        try:
            endpoint_files = sorted(
                entry.name for entry in (api_root / "endpoints").iterdir()
                if entry.is_file() and entry.suffix == ".mdx"
            )
        except OSError as exc:
            failures.append(f"{locale}: cannot scan localized API endpoints: {exc}")
            continue
        if len(endpoint_files) > MAX_LOCALIZED_ENDPOINT_FILES:
            failures.append(f"{locale}: localized API endpoint tree exceeds {MAX_LOCALIZED_ENDPOINT_FILES} files")
            continue
        actual = set(endpoint_files)
        if actual != expected:
            missing_files = sorted(expected - actual)
            extra_files = sorted(actual - expected)
            if missing_files:
                failures.append(f"{locale}: localized API files missing {', '.join(missing_files)}")
            if extra_files:
                failures.append(f"{locale}: localized API files unexpectedly contain {', '.join(extra_files)}")
    return failures
