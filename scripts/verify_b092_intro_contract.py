#!/usr/bin/env python3
"""Semantic Markdown check for the published BLAKE3 onboarding instruction."""
from __future__ import annotations
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHA = re.compile(r"sha[- ]?256", re.I)

def active_lines(text: str) -> list[str]:
    lines: list[str] = []
    fence = False
    html = False
    for raw in text.splitlines():
        stripped = raw.strip()
        if stripped.startswith("```"):
            fence = not fence
            continue
        if fence:
            continue
        if html:
            if "-->" in raw:
                raw = raw.split("-->", 1)[1]; html = False
            else:
                continue
        if "<!--" in raw:
            before, after = raw.split("<!--", 1)
            raw = before
            if "-->" in after:
                raw += after.split("-->", 1)[1]
            else:
                html = True
        if raw.strip() and not raw.lstrip().startswith("#"):
            lines.append(raw)
    if fence or html:
        raise RuntimeError("unterminated Markdown fence/comment")
    return lines


def check(text: str) -> None:
    lines = active_lines(text)
    if not any(re.search(r"blake3|b3sum", line, re.I) for line in lines):
        raise RuntimeError("BLAKE3/b3sum instruction is missing")
    for line in lines:
        if SHA.search(line) and not re.search(r"not.{0,12}sha256sum", line, re.I):
            raise RuntimeError("positive SHA-256 instruction found")

def self_test(text: str) -> None:
    try: check(re.sub(r"BLAKE3|b3sum", "digest", text, flags=re.I))
    except RuntimeError: pass
    else: raise RuntimeError("missing-BLAKE3 mutation passed")
    try: check(text + "\nUse sha256sum for uploads.\n")
    except RuntimeError: pass
    else: raise RuntimeError("positive SHA mutation passed")

if __name__ == "__main__":
    try:
        path = ROOT / "apps/docs/docs/intro.md"
        if path.is_symlink() or not path.is_file(): raise RuntimeError("intro.md missing or non-regular")
        text = path.read_text(encoding="utf-8"); check(text); self_test(text)
    except (OSError, RuntimeError, UnicodeError) as error:
        print(f"B-092 semantic check FAILED: {error}"); raise SystemExit(1)
    print("B-092 semantic check PASS: BLAKE3 instruction and SHA-256 negative are explicit")
