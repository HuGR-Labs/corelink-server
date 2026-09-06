#!/usr/bin/env python3
"""Fail-closed semantic check for the retired B-080 scope names."""
from __future__ import annotations
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REMOVED = ("admin:tenant-read", "admin:tenant-write", "admin:tokens", "admin:billing", "admin:audit", "admin:users")
LEDGER = ("cache:delete", "execute:action", "report:result")

def active_code(text: str) -> str:
    """Blank comments while retaining literals and rejecting malformed Rust."""
    out: list[str] = []
    i = 0
    block_depth = 0
    quote = False
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if block_depth:
            if c == "/" and n == "*":
                block_depth += 1; out.extend("  "); i += 2; continue
            if c == "*" and n == "/":
                block_depth -= 1; out.extend("  "); i += 2; continue
            out.append("\n" if c == "\n" else " "); i += 1; continue
        if quote:
            out.append(c)
            if c == "\\" and n:
                out.append(n); i += 2; continue
            if c == '"': quote = False
            i += 1; continue
        if c == "/" and n == "*":
            block_depth = 1; out.extend("  "); i += 2; continue
        if c == "/" and n == "/":
            while i < len(text) and text[i] != "\n": out.append(" "); i += 1
            continue
        if c == '"': quote = True
        out.append(c); i += 1
    if block_depth or quote:
        raise RuntimeError("unterminated Rust comment or string")
    return "".join(out)

def check(scopes: str, tests: str) -> None:
    active_scopes=active_code(scopes)
    for name in REMOVED:
        if name in active_scopes: raise RuntimeError(f"retired scope active: {name}")
    active_tests=active_code(tests)
    for fn in ("every_canonical_scope_name_is_enforced_or_declared", "removed_admin_scope_names_are_denied_everywhere"):
        if not re.search(rf"\bfn\s+{fn}\s*\(", active_tests): raise RuntimeError(f"semantic test missing: {fn}")
    region=active_tests[active_tests.find("const UNENFORCED_BY_DESIGN"):]
    for name in LEDGER:
        if not re.search(rf'"{re.escape(name)}"\s*,', region): raise RuntimeError(f"ledger entry missing: {name}")
    if sum(1 for name in re.findall(r'"([a-z-]+:[a-z-]+)"\s*,', region) if name in LEDGER) != 3:
        raise RuntimeError("ledger population changed")

def self_test(scopes: str, tests: str) -> None:
    mutated = re.sub(r"(pub const SCOPE_ADMIN[^\n]*\n)", r'\1const x = "admin:tenant-read";\n', scopes, count=1)
    try: check(mutated, tests)
    except RuntimeError: pass
    else: raise RuntimeError("active retired-name mutation passed")
    check(scopes + "\n// admin:tenant-read\n", tests)

if __name__ == "__main__":
    try:
        sp=ROOT/"crates/corelink-pat/src/scopes.rs"; tp=ROOT/"crates/corelink-container/tests/scope_catalog_closure.rs"
        if any(path.is_symlink() or not path.is_file() for path in (sp, tp)):
            raise RuntimeError("scope source/test missing or non-regular")
        s=sp.read_text(encoding="utf-8"); t=tp.read_text(encoding="utf-8"); check(s,t); self_test(s,t)
    except (OSError,RuntimeError,UnicodeError) as error: print(f"B-080 semantic check FAILED: {error}"); raise SystemExit(1)
    print("B-080 semantic check PASS: retired scopes absent and closure tests/ledger are live")
