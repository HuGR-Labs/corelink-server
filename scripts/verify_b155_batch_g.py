#!/usr/bin/env python3
"""Explicit-target semantic checks for the B-155 batch-G backlog records.

The checks in this module deliberately do not shell out to grep.  Each target
is named in code, comments are removed before executable-source assertions,
and missing or ambiguous populations fail closed.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
IDS = {
    "B-100": "verify_b100",
    "B-109": "verify_b109",
    "B-110": "verify_b110",
    "B-111": "verify_b111",
    "B-116": "verify_b116",
    "B-117": "verify_b117",
    "B-120": "verify_b120",
    "B-126": "verify_b126",
    "B-130": "verify_b130",
    "B-137": "verify_b137",
}


class CheckError(RuntimeError):
    pass


def required(path: str) -> Path:
    candidate = ROOT / path
    if not candidate.is_file():
        raise CheckError(f"missing explicit target: {path}")
    return candidate


def text(path: str) -> str:
    return required(path).read_text(encoding="utf-8")


def files(root: str, suffixes: set[str]) -> list[Path]:
    directory = ROOT / root
    if not directory.is_dir():
        raise CheckError(f"missing explicit target: {root}")
    result = sorted(
        p for p in directory.rglob("*")
        if p.is_file() and p.suffix.lower() in suffixes
        and "node_modules" not in p.parts
    )
    if not result:
        raise CheckError(f"empty explicit population: {root}")
    return result


def active_lines(path: str) -> list[str]:
    """Return source lines outside comments without treating string URLs as comments."""
    raw = text(path).splitlines()
    result: list[str] = []
    block = False
    for line in raw:
        out: list[str] = []
        quote: str | None = None
        escaped = False
        i = 0
        while i < len(line):
            pair = line[i : i + 2]
            if block:
                if pair == "*/":
                    block = False
                    i += 2
                else:
                    i += 1
                continue
            if quote:
                out.append(line[i])
                if escaped:
                    escaped = False
                elif line[i] == "\\":
                    escaped = True
                elif line[i] == quote:
                    quote = None
                i += 1
                continue
            if pair == "/*":
                block = True
                i += 2
                continue
            if pair == "//":
                break
            if line[i] in "'\"`":
                quote = line[i]
            out.append(line[i])
            i += 1
        current = "".join(out)
        stripped = current.lstrip()
        if stripped.startswith(("#", "<!--", "--")):
            continue
        if current.strip():
            result.append(current)
    return result


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise CheckError(message)


def rust_tokens(source: str) -> list[tuple[str, str]]:
    """Tokenize enough Rust to distinguish executable calls from bait text."""
    tokens: list[tuple[str, str]] = []
    i = 0
    block_depth = 0
    while i < len(source):
        if block_depth:
            if source.startswith("/*", i):
                block_depth += 1
                i += 2
            elif source.startswith("*/", i):
                block_depth -= 1
                i += 2
            else:
                i += 1
            continue
        if source.startswith("//", i):
            newline = source.find("\n", i + 2)
            i = len(source) if newline < 0 else newline + 1
            continue
        if source.startswith("/*", i):
            block_depth = 1
            i += 2
            continue
        raw = re.match(r"r(?P<hashes>#+)?\"", source[i:])
        if raw:
            hashes = raw.group("hashes") or ""
            marker = '"' + hashes
            start = i + len(raw.group(0))
            end = source.find(marker, start)
            if end < 0:
                raise CheckError("unterminated Rust raw string")
            tokens.append(("string", source[start:end]))
            i = end + len(marker)
            continue
        # Rust lifetimes/labels (`'a`, `'static`, `'retry`) are not character
        # literals and must not make the bounded lexer search for a quote.
        if source[i] == "'" and i + 1 < len(source) and re.match(r"[A-Za-z_]", source[i + 1]):
            tokens.append(("punct", "'"))
            i += 1
            continue
        if source[i] in "\"'":
            quote = source[i]
            start = i + 1
            i = start
            escaped = False
            while i < len(source):
                char = source[i]
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == quote:
                    break
                i += 1
            if i >= len(source):
                raise CheckError("unterminated Rust literal")
            if quote == '"':
                tokens.append(("string", source[start:i]))
            i += 1
            continue
        ident = re.match(r"[A-Za-z_][A-Za-z0-9_]*", source[i:])
        if ident:
            value = ident.group(0)
            tokens.append(("ident", value))
            i += len(value)
            continue
        if source[i].isspace():
            i += 1
            continue
        tokens.append(("punct", source[i]))
        i += 1
    if block_depth:
        raise CheckError("unterminated Rust block comment")
    return tokens


def executable_router_routes(source: str) -> list[str]:
    """Return route literals mounted by the real `fn router` body only."""
    tokens = rust_tokens(source)
    depth = 0
    router_body_depth: int | None = None
    pending_router = False
    routes: list[str] = []
    for index, (kind, value) in enumerate(tokens):
        next_value = tokens[index + 1][1] if index + 1 < len(tokens) else None
        if kind == "ident" and value == "fn" and next_value == "router":
            pending_router = True
        if pending_router and value == "{":
            router_body_depth = depth + 1
            pending_router = False
        if router_body_depth == depth and kind == "ident" and value == "route":
            previous = tokens[index - 1][1] if index else None
            if previous == "." and index + 3 < len(tokens):
                if tokens[index + 1][1] == "(" and tokens[index + 2][0] == "string":
                    routes.append(tokens[index + 2][1])
        if value == "{":
            depth += 1
        elif value == "}":
            if router_body_depth == depth:
                router_body_depth = None
            depth -= 1
    return routes


def route_parser_mutation_self_test() -> None:
    route = "/v1/test"
    cases = (
        ("executable", f'pub fn router() -> Router {{ Router::new().route("{route}", get(h)) }}', True),
        ("line-comment", f'pub fn router() -> Router {{ Router::new() // .route("{route}", get(h))\n }}', False),
        ("raw-string", f'pub fn router() -> Router {{ let bait = r#".route("{route}", get(h))"#; Router::new() }}', False),
        ("raw-string-no-hash", f'pub fn router() -> Router {{ let bait = r".route(\"{route}\", get(h))"; Router::new() }}', False),
        ("dead-function", f'fn dead() -> Router {{ Router::new().route("{route}", get(h)) }}\npub fn router() -> Router {{ Router::new() }}', False),
    )
    for name, source, expected in cases:
        actual = route in executable_router_routes(source)
        assert_true(actual == expected, f"route reachability mutation survived: {name}")


def verify_b100() -> None:
    candidates = files("apps", {".ts", ".tsx", ".md", ".mdx"})
    bad = [p for p in candidates if re.search(r"[A-Za-z0-9._%+-]+@corelink\.example", p.read_text(encoding="utf-8"))]
    assert_true(not bad, "placeholder @corelink.example remains in explicit apps population")
    notices = sorted((ROOT / "apps/admin-ui/src/content").glob("privacy-notice.*"))
    assert_true(notices, "privacy notice population is empty")
    for notice in notices:
        body = notice.read_text(encoding="utf-8")
        assert_true("privacy@humangr.com" in body or "dpo@humangr.com" in body, f"contact missing: {notice}")


def verify_b109() -> None:
    source = "\n".join(active_lines("crates/corelink-container/src/origin_timing.rs"))
    assert_true('parts.push(format!("oother;dur={other_ms}"))' in source, "oother emission is not executable")
    for phase in ("opat", "oquota", "ostore", "oaccounting", "oargon", "opermit", "ortier", "oaudit", "oratelimit"):
        assert_true(f'("{phase}",' in source, f"origin phase missing: {phase}")
    print("open: oother remains the explicit origin-timing residue")


def workflow_runs_on(path: str) -> list[str]:
    # YAML comments are line comments; do not use the language-source helper,
    # whose `//` rule would also truncate URL-like YAML values.
    return [
        match.group(1)
        for raw in text(path).splitlines()
        if not raw.lstrip().startswith("#")
        if (match := re.match(r"^\s*runs-on:\s*(\S+)", raw))
    ]


def packet_item(item_id: str) -> dict[str, object]:
    packet = json.loads(text("docs/handoff/2026-09-05-owner-action-packets-b008-b154.json"))
    items = packet.get("items")
    assert_true(isinstance(items, list), "owner packet items population missing")
    matches = [item for item in items if isinstance(item, dict) and item.get("id") == item_id]
    assert_true(len(matches) == 1, f"owner packet population for {item_id} is not unique")
    return matches[0]


def verify_b110() -> None:
    lanes = ("cas_foundation", "coverage", "ffi-matrix-ci", "mutation-nightly")
    for lane in lanes:
        values = workflow_runs_on(f".github/workflows/{lane}.yml")
        assert_true(values, f"runner population missing: {lane}")
        assert_true(all(value == "corelink" for value in values), f"non-corelink runner present: {lane}")
    values = workflow_runs_on(".github/workflows/semgrep.yml")
    assert_true(len(values) == 1, "semgrep runner population is not exactly one")
    assert_true(not re.search(r"ubuntu|macos|windows", values[0], re.I), "semgrep returned to hosted runner")
    assert_true(packet_item("B-110").get("status") == "done", "B-110 owner packet is not done")
    print("done: four lanes use corelink and semgrep remains outside hosted capacity")


def verify_b111() -> None:
    item = packet_item("B-111")
    assert_true(item.get("status") == "open", "B-111 owner packet is not open")
    procedure = " ".join(str(value) for value in item.get("procedure", []))
    required_names = (
        "APPLE_DEVELOPER_ID", "APPLE_DEVELOPER_ID_PASSWORD", "APPLE_TEAM_ID",
        "APPLE_NOTARIZATION_API_KEY", "APPLE_NOTARIZATION_KEY_ID",
        "APPLE_NOTARIZATION_ISSUER", "APPLE_DEVELOPER_ID_FINGERPRINT",
        "WINDOWS_CODE_SIGNING_CERT", "WINDOWS_CODE_SIGNING_PASSWORD",
        "WINDOWS_CODE_SIGNING_FINGERPRINT", "WINDOWS_CODE_SIGNING_SUBJECT",
    )
    for name in required_names:
        assert_true(name in procedure, f"B-111 packet lost required secret name: {name}")
    assert_true("never pass secret values" in procedure, "B-111 packet lost secret boundary")
    print("open: signing prerequisites remain owner-controlled")


def verify_b116() -> None:
    route_parser_mutation_self_test()
    routes = executable_router_routes(text("crates/corelink-container/src/routes/dpa_accept.rs"))
    assert_true(routes.count("/v1/onboarding/dpa-accept") == 1, "DPA route wiring is missing or ambiguous")
    spec = json.loads("{}") if False else None
    import yaml
    contract = yaml.safe_load(text("openapi/corelink-v1.yaml"))
    assert_true("/v1/onboarding/dpa-accept" in contract.get("paths", {}), "DPA route missing from OpenAPI")
    docs = files("apps/docs", {".md", ".mdx"})
    ghost = [p for p in docs if "/v1/dpa/accept" in p.read_text(encoding="utf-8")]
    assert_true(not ghost, "phantom /v1/dpa/accept remains in published docs")


def verify_b117() -> None:
    route_parser_mutation_self_test()
    routes = executable_router_routes(text("crates/corelink-container/src/routes/customer/part-00.rs"))
    for route in ("/v1/customer/account/delete", "/v1/customer/account/export"):
        assert_true(routes.count(route) == 1, f"route wiring missing or ambiguous: {route}")
        docs = files("apps/docs", {".md", ".mdx"})
        assert_true(any(route in p.read_text(encoding="utf-8") for p in docs), f"documentation missing: {route}")


def verify_b120() -> None:
    assert_true(not (ROOT / "apps/docs/docs/reference/api/endpoints/post-v1-enterprise-inquire.mdx").exists(), "phantom enterprise page exists")
    for path in ("crates", "worker/src", "apps/docs", "openapi"):
        root = ROOT / path
        assert_true(root.is_dir(), f"explicit target missing: {path}")
        hits = [p for p in root.rglob("*") if p.is_file() and "/v1/enterprise/inquire" in p.read_text(encoding="utf-8", errors="replace")]
        assert_true(not hits, f"enterprise inquiry remains in published/source target: {path}")
    assert_true("corelink-enterprise-inquiry" not in text("crates/corelink-container/Cargo.toml"), "enterprise dependency remains")


def code_files() -> list[Path]:
    suffixes = {".rs", ".ts", ".tsx", ".py", ".js", ".jsx", ".mjs", ".c", ".h", ".cc", ".cpp", ".cxx", ".go", ".java", ".kt", ".kts", ".rb", ".php", ".swift", ".scala", ".cs"}
    result = [Path(line) for line in subprocess.check_output(["git", "ls-tree", "-r", "--name-only", "HEAD"], cwd=ROOT, text=True).splitlines() if Path(line).suffix in suffixes and not any(part in {"node_modules", "target", ".open-next", ".wrangler"} for part in Path(line).parts)]
    assert_true(result, "code population is empty")
    return result


def verify_b126() -> None:
    population = code_files()
    # A zero result is meaningful only for a nonempty, closed Git-tracked
    # source population.  Empty or bypassed populations fail closed.
    assert_true(population, "code population is empty")
    oversized = [p for p in population if sum(1 for _ in (ROOT / p).open(encoding="utf-8")) > 1000]
    assert_true(not oversized, "file-size census found oversized code files: " + ", ".join(map(str, oversized)))
    print(f"done: closed code population has no files above 1000 lines ({len(population)} files)")


def verify_b130() -> None:
    result = subprocess.run([sys.executable, "scripts/validate_api_surface.py"], cwd=ROOT, stdout=subprocess.DEVNULL, check=False)
    assert_true(result.returncode == 0, "API surface validator failed")


def verify_b137() -> None:
    names = ("byok_kill_switch_drill_weekly", "byok_matrix_weekly", "dr-drill-monthly", "nightly", "perf-nightly")
    for name in names:
        lines = active_lines(f".github/workflows/{name}.yml")
        try:
            start = next(i for i, line in enumerate(lines) if line.strip() == "concurrency:")
        except StopIteration as error:
            raise CheckError(f"concurrency block missing: {name}") from error
        block = lines[start + 1 :]
        block = block[: next((i for i, line in enumerate(block) if line and not line.startswith((" ", "\t"))), len(block))]
        joined = "\n".join(block)
        assert_true("group:" in joined, f"concurrency group missing: {name}")
        assert_true("github.event_name" in joined and "github.ref" in joined, f"concurrency group is not event/ref scoped: {name}")


CHECKS = {
    "B-100": verify_b100, "B-109": verify_b109, "B-110": verify_b110, "B-111": verify_b111,
    "B-116": verify_b116, "B-117": verify_b117, "B-120": verify_b120, "B-126": verify_b126,
    "B-130": verify_b130, "B-137": verify_b137,
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--id", choices=sorted(CHECKS), required=True)
    args = parser.parse_args()
    try:
        CHECKS[args.id]()
    except (CheckError, OSError, ValueError, KeyError) as error:
        print(f"B-155 batch-G {args.id}: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"B-155 batch-G {args.id}: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
