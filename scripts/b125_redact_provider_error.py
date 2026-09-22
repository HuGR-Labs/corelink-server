#!/usr/bin/env python3
"""Write a bounded, credential-redacted provider failure into a B-125 receipt."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

MAX_EXCERPT = 2000


def redact_provider_stderr(raw: str) -> str:
    """Remove bearer, header, environment, and generic secret values."""
    redacted = re.sub(
        r"(?i)(authorization\s*:\s*bearer)\s+[^\s,;]+",
        r"\1 [REDACTED]",
        raw,
    )
    redacted = re.sub(
        r"(?i)\b(CF_API_TOKEN|CLOUDFLARE_API_TOKEN|CF_ACCOUNT_ID|CLOUDFLARE_ACCOUNT_ID)\b\s*=\s*[^\s,;]+",
        r"\1=[REDACTED]",
        redacted,
    )
    redacted = re.sub(
        r"(?i)\b(api[_ -]?token|authorization|password|secret|private[_ -]?key|access[_ -]?key)\b\s*(?:[:=]\s*|\s+)[^\s,;]+",
        r"\1=[REDACTED]",
        redacted,
    )
    redacted = "".join(
        char if char in "\n\t" or ord(char) >= 32 else " " for char in redacted
    )
    return redacted[-MAX_EXCERPT:] or "[no provider stderr]"


def record_provider_failure(receipt_path: Path, query_id: str, status: int, stderr_path: Path) -> None:
    raw = stderr_path.read_text(encoding="utf-8", errors="replace") if stderr_path.exists() else ""
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    receipt.setdefault("provider_failures", []).append(
        {
            "query_id": query_id,
            "exit_code": status,
            "stderr": redact_provider_stderr(raw),
            "stderr_sha256": hashlib.sha256(raw.encode("utf-8", "replace")).hexdigest(),
        }
    )
    receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    record_provider_failure(Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), Path(sys.argv[4]))
