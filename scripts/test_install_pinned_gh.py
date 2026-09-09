#!/usr/bin/env python3
"""Offline adversarial tests for the pinned GitHub CLI supply chain."""

from __future__ import annotations

import io
import os
import stat
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import install_pinned_gh as pinned
import verify_b102_b108_evidence as verifier


def expect_error(callable_, label: str) -> None:
    try:
        callable_()
    except (pinned.InstallError, verifier.EvidenceError):
        return
    raise AssertionError(f"mutation was accepted: {label}")


def unsafe_tar(root: Path, name: str, symlink: bool = False) -> None:
    archive = root / "unsafe.tar.gz"
    with tarfile.open(archive, "w:gz") as stream:
        info = tarfile.TarInfo(name)
        if symlink:
            info.type = tarfile.SYMTYPE
            info.linkname = "/etc/passwd"
        else:
            payload = b"forged"
            info.size = len(payload)
        stream.addfile(info, None if symlink else io.BytesIO(payload))
    expect_error(
        lambda: pinned._extract_tar(archive, root / "out", "gh_2.79.0_linux_amd64/bin/gh"),
        f"unsafe tar member {name}",
    )


def unsafe_zip(root: Path, name: str, symlink: bool = False) -> None:
    archive = root / "unsafe.zip"
    with zipfile.ZipFile(archive, "w") as stream:
        info = zipfile.ZipInfo(name)
        if symlink:
            info.external_attr = (stat.S_IFLNK | 0o777) << 16
        stream.writestr(info, b"forged")
    expect_error(
        lambda: pinned._extract_zip(archive, root / "out", "gh_2.79.0_macOS_amd64/bin/gh"),
        f"unsafe zip member {name}",
    )


def main() -> int:
    assert pinned.asset_for("linux", "x86_64") == "gh_2.79.0_linux_amd64.tar.gz"
    assert pinned.asset_for("darwin", "arm64") == "gh_2.79.0_macOS_arm64.zip"
    assert pinned.asset_for("windows", "AMD64") == "gh_2.79.0_windows_amd64.zip"
    with tempfile.TemporaryDirectory(prefix="corelink-gh-tests-") as directory:
        root = Path(directory)
        archive = root / "archive"
        archive.write_bytes(b"not the pinned release")
        expect_error(lambda: pinned.verify_digest(archive, "0" * 64), "wrong release hash")
        fake = root / "gh"
        fake.write_bytes(b"gh version 2.79.0 fake PATH shim")
        fake.chmod(0o700)
        expect_error(lambda: pinned.verify_binary(fake), "wrong explicit binary hash")
        expect_error(lambda: pinned.verify_binary(Path("relative-gh")), "relative explicit binary")
        unsafe_tar(root, "../escape")
        unsafe_tar(root, "gh_2.79.0_linux_amd64/bin/gh", symlink=True)
        unsafe_zip(root, "../escape")
        unsafe_zip(root, "gh_2.79.0_macOS_amd64/bin/gh", symlink=True)
        path_shim = root / "path-shim"
        path_shim.mkdir()
        (path_shim / "gh").write_bytes(b"gh version 2.79.0 shim")
        (path_shim / "gh").chmod(0o700)
        with patch.dict(os.environ, {"CORELINK_GH_BIN": str(path_shim / "gh"), "PATH": str(path_shim)}):
            expect_error(lambda: verifier.verify_attestation_with_gh({}, b"", {}, []), "PATH/env verifier shim")
        with patch.dict(
            os.environ,
            {
                "GH_TOKEN": "required-token",
                "GH_ENTERPRISE_TOKEN": "enterprise-token",
                "GITHUB_ENTERPRISE_TOKEN": "enterprise-token",
                "GH_HOST": "evil.example",
                "GH_CONFIG_DIR": "/tmp/evil-config",
                "GITHUB_API_URL": "https://evil.example/api/v3",
                "GITHUB_GRAPHQL_URL": "https://evil.example/graphql",
                "HTTP_PROXY": "http://evil-proxy",
                "HTTPS_PROXY": "http://evil-proxy",
                "ALL_PROXY": "http://evil-proxy",
                "NO_PROXY": "github.com",
                "SSL_CERT_FILE": "/tmp/evil-ca.pem",
                "SSL_CERT_DIR": "/tmp/evil-ca",
                "REQUESTS_CA_BUNDLE": "/tmp/evil-bundle.pem",
                "GIT_CONFIG_GLOBAL": "/tmp/evil-gitconfig",
                "GIT_SSH_COMMAND": "ssh -o ProxyCommand=evil",
            },
        ):
            environment = verifier.gh_environment(root / "home", root / "config")
        assert environment["GH_TOKEN"] == "required-token"
        assert environment["GH_HOST"] == "github.com"
        assert "GH_ENTERPRISE_TOKEN" not in environment
        assert "GITHUB_ENTERPRISE_TOKEN" not in environment
        assert "GITHUB_API_URL" not in environment
        assert "GITHUB_GRAPHQL_URL" not in environment
        assert environment["GITHUB_SERVER_URL"] == "https://github.com"
        for poisoned in (
            "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY", "SSL_CERT_FILE", "SSL_CERT_DIR",
            "REQUESTS_CA_BUNDLE", "GIT_SSH_COMMAND",
        ):
            assert poisoned not in environment
        assert environment["GH_CONFIG_DIR"] == str(root / "config")
        assert environment["HOME"] == str(root / "home")
        assert environment["PATH"] == os.defpath
        assert environment["GIT_CONFIG_GLOBAL"] == os.devnull
        assert environment["GIT_CONFIG_NOSYSTEM"] == "1"
        assert environment["LC_ALL"] == "C"
    print("pinned gh supply-chain mutations: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
