#!/usr/bin/env python3
"""Refuse a raw NUL byte in a tracked TEXT file.

RULE: a source file may not contain a literal 0x00.

Why this is a gate and not a style nit. Git decides "binary" by looking for a
NUL in the first 8000 bytes of a blob. One raw NUL therefore turns a normal
source file into a binary one for every git operation that matters to review:
`git diff` prints `Binary files a/x and b/x differ` instead of a patch, the
line counts in `--stat` collapse to `Bin N -> M bytes`, and GitHub's PR view
shows nothing at all. The content still compiles and the tests still pass, so
nothing goes red -- the file simply stops being reviewable, and every later
change to it ships unread. That is the same failure class as a gate that
answers wrongly instead of failing.

Found on `worker/tests/runner_mint.test.ts`, whose D1 mock joins tenant and
repo on a NUL separator (a good choice -- the byte cannot occur in either
half, so no pair of values can collide on the joined key). The separator was
written as a RAW NUL inside a template literal. The fix is not to change the
separator: `\\u0000` in a JS/TS string or template literal is the SAME
character at runtime and leaves the file text.

Scope: tracked files only, and only those git itself does not already treat as
binary by declaration (`.gitattributes` `binary` / `-text`). Genuinely binary
content -- images, fixtures, compiled artifacts -- is skipped by extension, so
this never asks a PNG to stop being a PNG.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

# Extensions whose content is binary BY NATURE. A NUL here is the format, not a
# defect. Kept explicit rather than "guess from content", because guessing from
# content is exactly the heuristic this check exists to stop relying on.
BINARY_EXT = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".icns", ".bmp",
    ".pdf", ".zip", ".gz", ".tgz", ".bz2", ".xz", ".zst", ".tar",
    ".woff", ".woff2", ".ttf", ".otf", ".eot",
    ".wasm", ".so", ".dylib", ".dll", ".a", ".o", ".bin", ".exe",
    ".mp4", ".mov", ".webm", ".mp3", ".wav", ".ogg",
    ".pack", ".idx", ".db", ".sqlite", ".sqlite3", ".parquet",
    # Packaged distributions: a wheel/jar/egg is a zip, a crate is a tarball.
    ".whl", ".jar", ".egg", ".crate", ".deb", ".rpm", ".img", ".iso", ".pyc",
}


def tracked_files(root: Path) -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root, check=True, capture_output=True,
    ).stdout
    return [root / p.decode() for p in out.split(b"\0") if p]


def offending(paths: list[Path]) -> list[tuple[Path, int]]:
    """Return (path, count) for every text file holding a raw NUL."""
    hits: list[tuple[Path, int]] = []
    for p in paths:
        if p.suffix.lower() in BINARY_EXT:
            continue
        try:
            data = p.read_bytes()
        except (OSError, IsADirectoryError):
            continue  # submodule / symlink / deleted-in-worktree
        n = data.count(b"\x00")
        if n:
            hits.append((p, n))
    return hits


def self_test() -> int:
    """Prove the checker in BOTH directions before it is trusted.

    A gate that has only ever been observed green proves nothing: it must be
    shown to go red on the defect it claims to catch, and green on the
    lookalike it must not.
    """
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)

        bad = root / "bad.ts"
        bad.write_bytes(b'const k = `${a}\x00${b}`;\n')
        if not offending([bad]):
            failures.append("MISS: a raw NUL in a .ts file was not caught")

        # The CORRECT form: the escape sequence. Same character at runtime,
        # file stays text. Must NOT be flagged.
        good = root / "good.ts"
        good.write_bytes(b'const k = `${a}\\u0000${b}`;\n')
        if offending([good]):
            failures.append("FALSE POSITIVE: the `\\u0000` escape was flagged")

        # A real binary asset must be skipped by extension, not by luck.
        png = root / "asset.png"
        png.write_bytes(b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR")
        if offending([png]):
            failures.append("FALSE POSITIVE: a .png was flagged")

    for f in failures:
        print(f"  ❌ {f}")
    if failures:
        print(f"⛔ check_no_nul_bytes self-test FAILED ({len(failures)})")
        return 1
    print("✅ check_no_nul_bytes self-test passed (3 cases, both directions)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true",
                    help="prove the checker red-and-green, then exit")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    root = Path(
        subprocess.run(["git", "rev-parse", "--show-toplevel"],
                       check=True, capture_output=True, text=True).stdout.strip()
    )
    hits = offending(tracked_files(root))
    if not hits:
        print("✅ no raw NUL byte in any tracked text file")
        return 0

    print("⛔ raw NUL byte in tracked text file(s) — git treats these as BINARY,")
    print("   so `git diff` shows a byte count instead of a patch and every later")
    print("   change to them ships unreviewed.\n")
    for p, n in hits:
        print(f"   {p.relative_to(root)}: {n} raw NUL byte(s)")
    print("\n   Fix: write the escape (`\\u0000` in JS/TS, `\\0` in Rust) — same")
    print("   character at runtime, and the file stays text.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
