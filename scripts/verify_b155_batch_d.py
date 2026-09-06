#!/usr/bin/env python3
"""Fail-closed semantic checks for the six B-155 batch-D records.

The checks read the named source files directly.  They retain literals needed
by the contract, remove comments, and discard statically dead ``if false``
blocks before looking for executable markers.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class CheckError(ValueError):
    pass


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise CheckError(f"missing or non-regular source: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise CheckError(f"cannot read source {relative}: {error}") from error


def _strip_comments(text: str, syntax: str) -> str:
    """Remove comments while preserving strings and line positions."""
    out: list[str] = []
    quote: str | None = None
    # Rust apostrophes also denote lifetimes (``&'a T``), not just character
    # literals.  The bounded source contracts only need double-quoted strings
    # protected from comment markers; treating every apostrophe as a quote
    # would fail closed on ordinary lifetime syntax.
    quote_chars = {'"', "`"} if syntax == "rust" else ({"'", '"', "`"} if syntax == "typescript" else set())
    block = False
    line = False
    i = 0
    while i < len(text):
        if line:
            if text[i] == "\n":
                line = False
                out.append("\n")
            else:
                out.append(" ")
            i += 1
            continue
        if block:
            if text.startswith("*/", i):
                block = False
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        if quote is not None:
            ch = text[i]
            out.append(ch)
            if ch == "\\" and i + 1 < len(text):
                out.append(text[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if syntax == "markdown" and text.startswith("<!--", i):
            # Markdown comments use a separate terminator.
            end = text.find("-->", i + 4)
            if end < 0:
                raise CheckError("unterminated markdown comment")
            hidden = text[i : end + 3]
            out.extend("\n" if ch == "\n" else " " for ch in hidden)
            i = end + 3
            continue
        if syntax in {"rust", "typescript"} and text.startswith("//", i):
            line = True
            out.extend("  ")
            i += 2
            continue
        if syntax in {"rust", "typescript"} and text.startswith("/*", i):
            block = True
            out.extend("  ")
            i += 2
            continue
        if syntax in {"toml", "yaml", "shell"} and text[i] == "#":
            line = True
            out.append(" ")
            i += 1
            continue
        if syntax == "rust" and text[i] == "'":
            # Keep Rust character literals atomic so a character such as `"`
            # cannot be mistaken for the start of a double-quoted string.
            end = i + 1
            if end < len(text) and text[end] == "\\":
                end += 2
            else:
                end += 1
            if end < len(text) and text[end] == "'":
                out.extend(text[i : end + 1])
                i = end + 1
                continue
        if text[i] in quote_chars:
            quote = text[i]
        out.append(text[i])
        i += 1
    if quote is not None or block or line:
        raise CheckError(f"unterminated {syntax} comment/string")
    return "".join(out)


def _strip_dead_if_false(text: str) -> str:
    """Blank balanced ``if false { ... }`` blocks, failing closed on syntax."""
    pattern = re.compile(r"\bif\s*(?:\(\s*false\s*\)|false)\s*\{")
    while True:
        match = pattern.search(text)
        if match is None:
            return text
        opening = text.find("{", match.start(), match.end())
        depth = 0
        quote: str | None = None
        escaped = False
        closing = None
        for index in range(opening, len(text)):
            ch = text[index]
            if quote is not None:
                if escaped:
                    escaped = False
                elif ch == "\\":
                    escaped = True
                elif ch == quote:
                    quote = None
                continue
            if ch in {"'", '"', "`"}:
                quote = ch
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    closing = index + 1
                    break
        if closing is None or quote is not None:
            raise CheckError("unbalanced dead if(false) block")
        text = text[: match.start()] + " " * (closing - match.start()) + text[closing:]


def _active(root: Path, relative: str, syntax: str) -> str:
    return _strip_dead_if_false(_strip_comments(_read(root, relative), syntax))


def _top_level_mapping(text: str, key: str) -> str:
    """Return one top-level YAML mapping body, rejecting relocated bait."""
    lines = text.splitlines()
    starts = [
        index for index, line in enumerate(lines)
        if re.fullmatch(rf"{re.escape(key)}\s*:", line)
    ]
    if len(starts) != 1:
        raise CheckError(f"{key}: expected one top-level mapping")
    start = starts[0] + 1
    end = start
    while end < len(lines) and (not lines[end] or lines[end][0].isspace()):
        end += 1
    return "\n".join(lines[start:end])


def _strip_strings(text: str, syntax: str) -> str:
    """Blank quoted literals while retaining executable source structure."""
    if syntax != "rust":
        return text
    out: list[str] = []
    quote: str | None = None
    i = 0
    while i < len(text):
        if quote is not None:
            if text[i] == "\\" and i + 1 < len(text):
                out.extend("  ")
                i += 2
                continue
            if text[i] == quote:
                out.append(" ")
                quote = None
            else:
                out.append("\n" if text[i] == "\n" else " ")
            i += 1
            continue
        if text[i] == '"':
            quote = text[i]
            out.append(" ")
            i += 1
            continue
        if text[i] == "'":
            end = i + 1
            if end < len(text) and text[end] == "\\":
                end += 2
            else:
                end += 1
            if end < len(text) and text[end] == "'":
                out.extend(" " * (end - i + 1))
                i = end + 1
                continue
        out.append(text[i])
        i += 1
    if quote is not None:
        raise CheckError("unterminated rust string")
    return "".join(out)


def _require(text: str, pattern: str, label: str, *, count: int | None = None) -> None:
    found = re.findall(pattern, text, re.MULTILINE)
    if count is not None and len(found) != count:
        raise CheckError(f"{label}: expected {count}, found {len(found)}")
    if count is None and not found:
        raise CheckError(f"{label}: active marker missing")


def check_b136(root: Path) -> None:
    text = _active(root, ".github/workflows/backlog-verify.yml", "yaml")
    concurrency = _top_level_mapping(text, "concurrency")
    groups = re.findall(r"(?m)^  group\s*:\s*(.+)$", concurrency)
    if len(groups) != 1 or "github.event_name" not in groups[0] or "github.ref" not in groups[0]:
        raise CheckError("B-136: concurrency group must be unique and event/ref scoped")
    cancels = re.findall(r"(?m)^  cancel-in-progress\s*:\s*true\s*$", concurrency)
    if len(cancels) != 1:
        raise CheckError("B-136: cancel-in-progress true is not uniquely active")


def check_b141(root: Path) -> None:
    labels = _active(root, ".github/workflows/pr-labels.yml", "yaml")
    ratchet = _active(root, ".github/workflows/file-size-ratchet.yml", "yaml")
    welcome = _active(root, ".github/workflows/welcome-first-pr.yml", "yaml")
    for association in ("OWNER", "MEMBER", "COLLABORATOR"):
        _require(labels, rf"author_association\s*==\s*['\"]{association}['\"]", f"B-141 {association}")
    if re.search(r"(?m)author_association", ratchet):
        raise CheckError("B-141: ratchet has an actor gate")
    _require(ratchet, r"(?m)^\s*persist-credentials\s*:\s*false\s*$", "B-141 persist credentials")
    _require(ratchet, r"(?m)^\s*contents\s*:\s*read\s*$", "B-141 read permission")
    _require(welcome, r"(?m)^\s*runs-on:\s*\[self-hosted,\s*mac,\s*corelink-builder\]\s*$", "B-141 welcome pool")


def check_b159(root: Path) -> None:
    locales = (
        "apps/docs/docs/integrations/sccache-cargo.md",
        "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/integrations/sccache-cargo.md",
        "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/integrations/sccache-cargo.md",
        "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/integrations/sccache-cargo.md",
    )
    methods = ("GET", "PUT", "HEAD", "PROPFIND", "MKCOL", "DELETE")
    for relative in locales:
        text = _active(root, relative, "markdown")
        for method in methods:
            _require(text, rf"(?m)^\|\s*`{method}`\s*\|", f"B-159 {relative} {method}", count=1)
        for phrase in ("read-only", "Nur-Lesen", "solo lectura", "somente leitura"):
            if phrase.lower() in text.lower():
                break
        else:
            raise CheckError(f"B-159 {relative}: read-only explanation missing")
        if "Cache errors" not in text or "sccache --show-stats" not in text or ".sccache_check" not in text:
            raise CheckError(f"B-159 {relative}: health diagnostics missing")
        if not re.search(r"(?i)(probe-only|ausschließlich für die Sonde bestimmt|exclusiva de la sonda|exclusiva da sonda)", text):
            raise CheckError(f"B-159 {relative}: probe-only explanation missing")
        forbidden = re.compile(
            r"(?i)(internal cleanup|internal-only|not as a public|nicht nur intern|"
            r"no solo interna|no como un método público|limpieza de control interno|"
            r"não apenas interna|não como método público)"
        )
        lines = text.splitlines()
        for index, line in enumerate(lines):
            context = " ".join(lines[max(0, index - 1) : index + 2])
            if forbidden.search(line) and not re.search(r"(?i)\b(not|nicht|no|não)\b", context):
                raise CheckError(f"B-159 {relative}: DELETE was reduced to internal cleanup")
    runtime = _active(root, "crates/corelink-container/src/routes/cargo/part-00.rs", "rust")
    runtime_code = _strip_strings(runtime, "rust")
    # These patterns are deliberately tied to executable request dispatch or
    # calls.  A string literal mentioning a method is not a route proof.
    for method in ("GET", "PUT", "HEAD"):
        _require(runtime_code, rf"\bMethod::{method}\b\s*(?:\||=>|[,)])", f"B-159 runtime {method}")
    _require(runtime, r'req\.method\(\)\s*\.as_str\(\)\s*==\s*"PROPFIND"', "B-159 runtime PROPFIND")
    _require(runtime, r'req\.method\(\)\s*\.as_str\(\)\s*==\s*"MKCOL"', "B-159 runtime MKCOL")
    _require(runtime, r"req\.method\(\)\s*==\s*Method::DELETE", "B-159 runtime DELETE")
    _require(runtime_code, r"\basync\s+fn\s+handle_delete\b", "B-159 runtime DELETE handler")
    _require(runtime_code, r"\.resolve_with_capability\s*\(", "B-159 runtime capability resolver")
    tests = _active(root, "crates/corelink-container/src/routes/cargo/tests-00-00.rs", "rust")
    _require(_strip_strings(tests, "rust"), r"\b(?:async\s+)?fn\s+(?:normal_pat_can_still_delete_an_artifact|delete_existing_key_is_204_and_removes_it)\b", "B-159 DELETE test")


def check_b164(root: Path) -> None:
    doctor = _active(root, "tools/cli/src/doctor.rs", "rust")
    doctor_code = _strip_strings(doctor, "rust")
    start = doctor_code.find("async fn check_quota")
    if start < 0:
        raise CheckError("B-164: check_quota function missing")
    depth = 0
    opening = doctor_code.find("{", start)
    for index in range(opening, len(doctor_code)):
        depth += doctor_code[index] == "{"
        depth -= doctor_code[index] == "}"
        if depth == 0:
            block = doctor[start : index + 1]
            code_block = doctor_code[start : index + 1]
            break
    else:
        raise CheckError("B-164: unbalanced check_quota function")
    _require(code_block, r"CliError::HttpStatus\s*\{\s*status:\s*401\s*\}", "B-164 HTTP 401")
    for status, code in ((401, "COR_AUTH_INVALID"), (403, "COR_AUTH_FORBIDDEN"), (429, "COR_RATE_LIMITED")):
        arm = re.search(
            rf"CliError::HttpStatus\s*\{{\s*status:\s*{status}\s*\}}\s*=>\s*\(",
            code_block,
        )
        if arm is None:
            raise CheckError(f"B-164 {code} status arm missing")
        next_arm = re.search(r"CliError::HttpStatus\s*\{", code_block[arm.end() :])
        end = arm.end() + (next_arm.start() if next_arm else len(code_block))
        if not re.search(rf'"{re.escape(code)}"', block[arm.start() : end]):
            raise CheckError(f"B-164 {code} status arm missing")
    error_match = re.search(r"(?m)Err\(err\)\s*=>\s*\{", code_block)
    if error_match is None:
        raise CheckError("B-164: wildcard error arm replaced the status map")
    error_arm = block[error_match.start() :]
    if '"COR_QUOTA_EXCEEDED"' in error_arm:
        raise CheckError("B-164: transport/status error maps to quota exhaustion")
    client = _active(root, "tools/cli/src/client.rs", "rust")
    client_code = _strip_strings(client, "rust")
    _require(client_code, r"\b(?:async\s+)?fn\s+get_json_preserves_http_status_without_response_body\b", "B-164 client status test")
    for marker in (
        "check_quota_unauthorized_is_auth_failure",
        "check_quota_forbidden_is_scope_failure",
        "check_quota_rate_limited_is_not_quota_exhaustion",
        "check_quota_transport_is_net_unreachable",
    ):
        _require(doctor_code, rf"\b(?:async\s+)?fn\s+{re.escape(marker)}\b", f"B-164 test {marker}")
    _require(client_code, r"\b(?:async\s+)?fn\s+get_json_malformed_success_is_decode_error\b", "B-164 client decode test")
    for relative in (
        "apps/docs/docs/integrations/npm.md",
        "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/integrations/npm.md",
        "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/integrations/npm.md",
        "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/integrations/npm.md",
    ):
        text = _active(root, relative, "markdown")
        if re.search(r"npm\s+config\s+get\s+@scope:registry", text):
            raise CheckError(f"B-164 {relative}: scoped registry query remains")
        for location in ("project", "user"):
            _require(
                text,
                rf"npm\s+config\s+get\s+registry\s+--location={location}",
                f"B-164 {relative} {location} registry",
                count=1,
            )


CHECKS = {"B-136": check_b136, "B-141": check_b141, "B-159": check_b159, "B-164": check_b164}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--id", choices=tuple(CHECKS), required=True)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    try:
        CHECKS[args.id](args.root.resolve())
    except (CheckError, OSError, UnicodeDecodeError) as error:
        print(f"B-155 batch-D {args.id}: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"B-155 batch-D {args.id}: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
