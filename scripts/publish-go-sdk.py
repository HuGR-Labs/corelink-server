#!/usr/bin/env python3
"""Publish sdks/go as a static Go module proxy under apps/docs/static/goproxy.

The GOPROXY protocol is four files per version -- @v/list, .info, .mod and
.zip -- so a module can be served from any static host, the same trick that
lets the Python SDK ship as a PEP 503 index instead of a PyPI release.

The zip is written with fixed timestamps so a rebuild is byte-identical and a
drift gate can compare it. Every entry inside must be prefixed
"<module>@<version>/" or the go command rejects the archive.
"""
import hashlib
import pathlib
import sys
import zipfile

MODULE = "go.corelink.humangr.com/corelink-go"
VERSION = "v0.1.0"
# Fixed so the archive is reproducible. It is the module's publication date,
# not a build clock.
STAMP = (2026, 8, 24, 0, 0, 0)
ISO = "2026-08-24T00:00:00Z"

root = pathlib.Path(__file__).resolve().parent.parent
src = root / "sdks" / "go"
out = root / "apps" / "docs" / "static" / "goproxy" / MODULE / "@v"
out.mkdir(parents=True, exist_ok=True)

# Only what a consumer compiles: no tests, no local build junk.
members = sorted(
    p for p in src.rglob("*")
    if p.is_file()
    and p.suffix in {".go", ".mod", ".sum"}
    and not p.name.endswith("_test.go")
)
if not members:
    sys.exit("no Go sources found under sdks/go")

zip_path = out / f"{VERSION}.zip"
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
    for m in members:
        arc = f"{MODULE}@{VERSION}/{m.relative_to(src).as_posix()}"
        info = zipfile.ZipInfo(arc, date_time=STAMP)
        info.external_attr = 0o644 << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, m.read_bytes())

(out / f"{VERSION}.mod").write_bytes((src / "go.mod").read_bytes())
(out / f"{VERSION}.info").write_text(f'{{"Version":"{VERSION}","Time":"{ISO}"}}\n')
(out / "list").write_text(f"{VERSION}\n")

digest = hashlib.sha256(zip_path.read_bytes()).hexdigest()
print(f"published {MODULE}@{VERSION}")
print(f"  files in zip : {len(members)}")
print(f"  zip sha256   : {digest}")
