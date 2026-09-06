#!/usr/bin/env python3
"""Semantic B-089 contract check; comments and generated trees are not evidence."""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLAIM = re.compile(r"issued automatically|automatic(?:ly)?[^\n]*credit", re.I)
TOKENS = re.compile(r"\b(?:service_credit|sla_credit|credit_note|balance_transaction)\b")


def fail(message: str) -> None:
    raise RuntimeError(message)


def code_with_comments_blank(text: str) -> str:
    """Blank comments while retaining string literals as fail-closed evidence."""
    text = re.sub(r"/\*.*?\*/", lambda m: "\n" * m.group(0).count("\n"), text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def check_claim(text: str) -> None:
    # Markdown comments and fenced examples cannot satisfy a contractual claim.
    visible: list[str] = []
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
                raw = raw.split("-->", 1)[1]
                html = False
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
            visible.append(raw)
    if fence or html:
        fail("unterminated Markdown fence/comment")
    if not any(CLAIM.search(line) for line in visible):
        fail("SLA no longer contains the automatic-credit promise")


def check_tree() -> None:
    sla = ROOT / "legal/sla/v1.0.0.md"
    if sla.is_symlink() or not sla.is_file(): fail("SLA missing or non-regular")
    check_claim(sla.read_text(encoding="utf-8"))
    files = [p for root in (ROOT / "crates", ROOT / "worker/src", ROOT / "apps")
             for p in root.rglob("*") if p.is_file() and not p.is_symlink()
             and p.suffix in {".rs", ".ts", ".tsx"}
             and "target" not in p.parts and "node_modules" not in p.parts]
    if not files: fail("empty implementation population")
    for path in files:
        code = code_with_comments_blank(path.read_text(encoding="utf-8"))
        if TOKENS.search(code):
            fail(f"credit implementation token is active or ambiguous: {path}")


def self_test() -> None:
    try: check_claim("<!-- issued automatically against next invoice -->")
    except RuntimeError: pass
    else: fail("comment-only claim passed")
    try: check_claim("```text\nissued automatically against next invoice\n```")
    except RuntimeError: pass
    else: fail("fenced claim passed")
    try: check_claim("## no credit")
    except RuntimeError: pass
    else: fail("missing-claim mutation passed")
    if TOKENS.search(code_with_comments_blank("// service_credit\nfn ok() {}")):
        fail("comment mutation became active")
    if not TOKENS.search(code_with_comments_blank('const x = "service_credit";')):
        fail("string mutation disappeared")


if __name__ == "__main__":
    try:
        check_tree(); self_test()
    except (OSError, RuntimeError, UnicodeError) as error:
        print(f"B-089 semantic check FAILED: {error}")
        raise SystemExit(1)
    print("B-089 semantic check PASS: live SLA promise and zero active credit implementation")
