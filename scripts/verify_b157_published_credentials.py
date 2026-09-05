#!/usr/bin/env python3
"""Fail-closed census for B-157 credentials across the published surface."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ROOTS = ("apps/docs", "marketing", "legal")
ROOT_FILES = ("README.md",)
EXTENSIONS = {".md", ".mdx", ".html", ".htm"}
EXCLUDED = {".git", ".docusaurus", ".wrangler", "build", "dist", "node_modules"}
REMOTE_HEADER = re.compile(r"--remote_header=Authorization=Bearer")
CURL_ARGV = re.compile(
    r"\bcurl\b.*(?:--header|-H).*Authorization:\s*Bearer.*"
    r"(?:\$\{?CORELINK_PAT\}?|CORELINK_PAT)",
    re.IGNORECASE,
)
HELPER = re.compile(
    r"^\s*build(?::[A-Za-z0-9_-]+)?\s+--credential_helper=([^\s]+)"
)
CONFIG_CURL = re.compile(
    r"\bcurl\b.*--config\s+(?:-|/dev/fd/3)(?:\s+3)?\s*<<EOF\s*$"
)
SAFE_HEADER = re.compile(r'^\s*header\s*=\s*"Authorization: Bearer \$\{CORELINK_PAT\}"\s*$')


def published_files(root: Path) -> list[Path]:
    paths: list[Path] = []
    for rel in ROOT_FILES:
        path = root / rel
        if path.is_file() and path.suffix.lower() in EXTENSIONS:
            paths.append(path)
    for rel in ROOTS:
        directory = root / rel
        if not directory.is_dir():
            continue
        for path in directory.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in EXTENSIONS:
                continue
            if any(part in EXCLUDED for part in path.relative_to(directory).parts):
                continue
            paths.append(path)
    return sorted(set(paths))


def census(root: Path) -> dict[str, object]:
    files = published_files(root)
    findings: list[dict[str, object]] = []
    helper_lines: list[dict[str, object]] = []
    stdin_curls: list[dict[str, object]] = []
    for path in files:
        rel = path.relative_to(root).as_posix()
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            line_number = index + 1
            if REMOTE_HEADER.search(line):
                findings.append({"kind": "remote_header", "path": rel, "line": line_number})
            if "curl" in line:
                end = index
                while end + 1 < len(lines) and lines[end].rstrip().endswith("\\"):
                    end += 1
                command = " ".join(lines[index : end + 1])
                if CURL_ARGV.search(command):
                    findings.append({"kind": "curl_argv", "path": rel, "line": line_number})
                if CONFIG_CURL.search(command):
                    block = lines[end + 1 : end + 5]
                    normalized = [entry.strip() for entry in block]
                    valid = (
                        len(normalized) >= 2
                        and SAFE_HEADER.match(normalized[0])
                        and normalized[1] == "EOF"
                    ) or (
                        len(normalized) >= 3
                        and normalized[0].startswith('url = "')
                        and SAFE_HEADER.match(normalized[1])
                        and normalized[2] == "EOF"
                    )
                    if not valid:
                        findings.append({"kind": "malformed_stdin_config", "path": rel, "line": line_number})
                    else:
                        stdin_curls.append({"path": rel, "line": line_number})
            match = HELPER.match(line)
            if match:
                value = match.group(1)
                helper_lines.append({"path": rel, "line": line_number, "value": value})
                if not value.startswith("corelink-api.humangr.com="):
                    findings.append({"kind": "unscoped_helper", "path": rel, "line": line_number})
    return {
        "files": [path.relative_to(root).as_posix() for path in files],
        "helper_lines": helper_lines,
        "stdin_curls": stdin_curls,
        "findings": findings,
    }


def verify(root: Path = ROOT) -> tuple[dict[str, object], list[str]]:
    result = census(root)
    failures = [
        f"HALT: {entry['kind']} at {entry['path']}:{entry['line']}"
        for entry in result["findings"]
        if isinstance(entry, dict)
    ]
    if not result["files"]:
        failures.append("HALT: published credential census has no files")
    if not result["helper_lines"]:
        failures.append("HALT: no host-scoped credential helper directives found")
    return result, failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    result, failures = verify(Path(args.root).resolve())
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print(
            f"B-157 credential census: files={len(result['files'])} "
            f"helpers={len(result['helper_lines'])} stdin_curls={len(result['stdin_curls'])} "
            f"remote_header=0 curl_argv=0"
        )
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
