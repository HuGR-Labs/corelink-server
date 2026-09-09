#!/usr/bin/env python3
"""Fetch and safely install the repository-pinned GitHub CLI release."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import stat
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

VERSION = "2.79.0"
RELEASE_BASE = f"https://github.com/cli/cli/releases/download/v{VERSION}/"
CHECKSUMS = Path(__file__).with_name(f"gh_{VERSION}_checksums.json")


class InstallError(ValueError):
    pass


def checksums() -> dict[str, str]:
    value = json.loads(CHECKSUMS.read_text(encoding="utf-8"))
    if value.get("version") != VERSION or not isinstance(value.get("assets"), dict):
        raise InstallError("pinned gh checksum manifest is malformed")
    result = value["assets"]
    for name, digest in result.items():
        if not isinstance(name, str) or not isinstance(digest, str) or len(digest) != 64:
            raise InstallError("pinned gh checksum manifest contains malformed entries")
        int(digest, 16)
    return result


def binary_checksums() -> dict[str, str]:
    value = json.loads(CHECKSUMS.read_text(encoding="utf-8"))
    result = value.get("binaries")
    if value.get("version") != VERSION or not isinstance(result, dict):
        raise InstallError("pinned gh binary checksum manifest is malformed")
    for name, digest in result.items():
        if not isinstance(name, str) or not isinstance(digest, str) or len(digest) != 64:
            raise InstallError("pinned gh binary checksum manifest contains malformed entries")
        int(digest, 16)
    return result


def _arch(machine: str) -> str:
    normalized = machine.lower()
    return {
        "x86_64": "amd64",
        "amd64": "amd64",
        "aarch64": "arm64",
        "arm64": "arm64",
        "i386": "386",
        "i686": "386",
        "armv6l": "armv6",
    }.get(normalized, "")


def asset_for(system: str | None = None, machine: str | None = None) -> str:
    system = (system or platform.system()).lower()
    machine = _arch(machine or platform.machine())
    if not machine:
        raise InstallError("unsupported runner architecture")
    if system == "linux":
        extension = "tar.gz"
        label = "linux"
    elif system == "darwin":
        extension = "zip"
        label = "macOS"
    elif system == "windows":
        extension = "zip"
        label = "windows"
    else:
        raise InstallError("unsupported runner operating system")
    name = f"gh_{VERSION}_{label}_{machine}.{extension}"
    if name not in checksums():
        raise InstallError(f"no pinned gh checksum for {name}")
    return name


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_digest(path: Path, expected: str) -> None:
    actual = sha256_file(path)
    if actual != expected:
        raise InstallError(f"pinned gh archive digest mismatch: expected {expected}, got {actual}")


def _safe_relative(name: str) -> Path:
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or any(part in ("", ".", "..") for part in path.parts):
        raise InstallError(f"unsafe archive member: {name!r}")
    return Path(*path.parts)


def _safe_target(root: Path, name: str) -> Path:
    relative = _safe_relative(name)
    target = (root / relative).resolve()
    root = root.resolve()
    if os.path.commonpath((str(root), str(target))) != str(root):
        raise InstallError(f"archive member escapes extraction root: {name!r}")
    return target


def _copy_archive_member(root: Path, name: str, reader, mode: int = 0o755) -> Path:
    target = _safe_target(root, name)
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() or target.is_symlink():
        raise InstallError(f"duplicate archive member: {name!r}")
    with target.open("xb") as output:
        while True:
            chunk = reader.read(1024 * 1024)
            if not chunk:
                break
            output.write(chunk)
    target.chmod(mode)
    return target


def _extract_tar(archive: Path, root: Path, expected_member: str) -> Path:
    with tarfile.open(archive, "r:gz") as stream:
        selected = None
        for member in stream.getmembers():
            _safe_target(root, member.name)
            if member.issym() or member.islnk() or not (member.isdir() or member.isreg()):
                raise InstallError(f"unsupported or link archive member: {member.name!r}")
            if member.name == expected_member:
                if selected is not None:
                    raise InstallError("duplicate pinned gh binary in archive")
                selected = member
        if selected is None:
            raise InstallError("pinned gh archive has no expected binary")
        source = stream.extractfile(selected)
        if source is None:
            raise InstallError("pinned gh binary is unreadable")
        return _copy_archive_member(root, selected.name, source)


def _extract_zip(archive: Path, root: Path, expected_member: str) -> Path:
    with zipfile.ZipFile(archive) as stream:
        selected = None
        seen: set[str] = set()
        for info in stream.infolist():
            if info.filename in seen:
                raise InstallError(f"duplicate archive member: {info.filename!r}")
            seen.add(info.filename)
            _safe_target(root, info.filename.rstrip("/")) if info.filename.rstrip("/") else None
            mode = (info.external_attr >> 16) & 0o170000
            if mode == stat.S_IFLNK:
                raise InstallError(f"symlink archive member: {info.filename!r}")
            if info.filename == expected_member:
                if selected is not None:
                    raise InstallError("duplicate pinned gh binary in archive")
                selected = info
        if selected is None:
            raise InstallError("pinned gh archive has no expected binary")
        with stream.open(selected) as source:
            return _copy_archive_member(root, selected.filename, source)


def verify_binary(path: Path) -> Path:
    """Accept only an absolute, non-symlink binary with the pinned digest."""
    if not path.is_absolute() or path.is_symlink() or not path.is_file():
        raise InstallError("CORELINK_GH_BIN must be an absolute regular file")
    asset = asset_for()
    verify_digest(path, binary_checksums()[asset])
    return path


def install(output: Path, download_root: Path | None = None) -> Path:
    asset = asset_for()
    expected = checksums()[asset]
    root = (download_root or Path(tempfile.mkdtemp(prefix="corelink-gh-")).resolve())
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    archive = root / asset
    request = urllib.request.Request(RELEASE_BASE + asset, headers={"User-Agent": "corelink-pinned-gh"})
    with urllib.request.urlopen(request, timeout=60) as response, archive.open("xb") as stream:
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            stream.write(chunk)
    verify_digest(archive, expected)
    extract_root = root / "extract"
    extract_root.mkdir(mode=0o700)
    directory = f"gh_{VERSION}_{asset.split('_', 2)[2].rsplit('.', 2)[0]}"
    if asset.startswith("gh_2.79.0_windows_"):
        expected_member = "bin/gh.exe"
    else:
        expected_member = f"{directory}/bin/gh"
    if asset.endswith(".tar.gz"):
        source = _extract_tar(archive, extract_root, expected_member)
    else:
        source = _extract_zip(archive, extract_root, expected_member)
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise InstallError("pinned gh output already exists")
    output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    output.write_bytes(source.read_bytes())
    output.chmod(0o700)
    verify_binary(output)
    return output


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(install(args.output))
    except (OSError, InstallError, urllib.error.URLError) as exc:
        raise SystemExit(f"pinned gh install failed: {exc}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
