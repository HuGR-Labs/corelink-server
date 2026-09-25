#!/usr/bin/env python3
"""Safe hosted contract for the policy and PR automation migration (#2374).

The candidate workflows are inspected as data. Read-only policy checks run on
local fixtures; paths that write labels, comments, auto-merge state, artifacts,
or stale state are represented by a deny-by-default dry-run sink. No GitHub API
write or provider operation is available to this proof.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import re
import shutil
import subprocess
import sys
import textwrap
import tempfile
import threading
import tomllib
from pathlib import Path, PurePosixPath, PureWindowsPath
from urllib.parse import parse_qs, urlsplit

from verify_hosted_runner_migration_contract import ContractError, job_blocks


ROOT = Path(__file__).resolve().parents[1]
FILES = {
    ".github/workflows/bot-pr-has-checks.yml": {"audit"},
    ".github/workflows/coverage.yml": {"coverage"},
    ".github/workflows/dependabot-auto-merge.yml": {"auto-merge"},
    ".github/workflows/dependabot-policy-trust-boundary.yml": {"trust-boundary-teeth"},
    ".github/workflows/dependabot-policy.yml": {"sentinel", "policy-gate"},
    ".github/workflows/lockfile-diff.yml": {"lockfile-diff"},
    ".github/workflows/permission-matrix.yml": {"permission-matrix"},
    ".github/workflows/pr-labels.yml": {"label", "size"},
    ".github/workflows/stale.yml": {"stale"},
    ".github/workflows/welcome-first-pr.yml": {"welcome"},
}
MUTATION_MARKERS = {
    "auto-merge": "gh pr merge --auto",
    "lockfile-diff": "Post PR comment with diff",
    "label": "actions/labeler@",
    "size": "gh pr edit",
    "stale": "actions/stale@",
    "welcome": "gh api",
    "coverage": "actions/upload-artifact@",
}
SHA = re.compile(r"[0-9a-f]{40}")
POLICY_CARGO_DENY_COMMAND = (
    'cargo deny --manifest-path "$POLICY_TREE/Cargo.toml" --config "$DENY_CONFIG" check licenses bans'
)


def fail(message: str) -> None:
    raise ContractError(message)


def read_workflows(root: Path) -> dict[str, str]:
    result = {}
    for path in FILES:
        try:
            result[path] = (root / path).read_text(encoding="utf-8")
        except OSError as error:
            fail(f"{path}: unreadable: {error}")
    return result


def root_section(text: str, key: str, *, required: bool = True) -> tuple[str, ...]:
    lines = text.splitlines()
    try:
        start = next(index for index, line in enumerate(lines) if line == key + ":")
    except StopIteration:
        if required:
            fail(f"workflow missing {key!r} section")
        return ()
    selected = []
    for line in lines[start:]:
        if selected and line and not line[0].isspace() and not line.startswith("#"):
            break
        if line.lstrip().startswith("#") or not line.strip():
            continue
        selected.append(line.rstrip())
    return tuple(selected)


def job_setting(lines: list[str], setting: str) -> tuple[str, ...]:
    """Capture one job-level scalar, including folded `if:` continuations."""
    for index, line in enumerate(lines):
        match = re.match(r"^    " + re.escape(setting) + r":(?:\s*(.*))?$", line)
        if not match:
            continue
        result = [line.strip()]
        if match.group(1) in (None, "", "|", ">", "|-", ">-"):
            for continuation in lines[index + 1:]:
                if continuation.startswith("    ") and not continuation.startswith("      "):
                    break
                if continuation.strip() and not continuation.lstrip().startswith("#"):
                    result.append(continuation.strip())
        return tuple(result)
    return ()


def run_blocks(lines: list[str]) -> tuple[str, ...]:
    blocks = []
    index = 0
    while index < len(lines):
        line = lines[index]
        match = re.match(r"^(\s*)run:\s*(.*)$", line)
        if not match:
            index += 1
            continue
        base_indent = len(match.group(1))
        declaration = match.group(2)
        if declaration not in {"|", ">-", "|-", ">"}:
            blocks.append(declaration)
            index += 1
            continue
        index += 1
        body = []
        while index < len(lines):
            current = lines[index]
            if current.strip() and len(current) - len(current.lstrip()) <= base_indent:
                break
            if current.strip() and not current.lstrip().startswith("#"):
                body.append(current.strip())
            index += 1
        blocks.append("\n".join(body))
    return tuple(blocks)


def commands_preserve_intent(relative: str, job: str, original: tuple[str, ...], candidate: tuple[str, ...]) -> bool:
    return original == candidate


def step_blocks(lines: list[str]) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    active: str | None = None
    for line in lines:
        match = re.match(r"^      - name: (.+)$", line)
        if match:
            active = match.group(1)
            if active in result:
                fail(f"duplicate workflow step name: {active}")
            result[active] = [line]
        elif active is not None and (line.startswith("      - ") or (line and not line.startswith(" "))):
            active = None
        elif active is not None:
            result[active].append(line)
    return result


def step_value(lines: list[str], key: str, indent: int = 8) -> tuple[str, ...]:
    pattern = re.compile(r"^" + " " * indent + re.escape(key) + r":(?:\s*(.*))?$")
    for index, line in enumerate(lines):
        match = pattern.match(line)
        if not match:
            continue
        result = [line.strip()]
        if match.group(1) in (None, "", "|", ">", "|-", ">-"):
            for continuation in lines[index + 1:]:
                if continuation.strip() and len(continuation) - len(continuation.lstrip()) <= indent:
                    break
                if continuation.strip() and not continuation.lstrip().startswith("#"):
                    result.append(continuation.strip())
        return tuple(result)
    return ()


def action_reference(lines: list[str]) -> tuple[str, ...]:
    values = step_value(lines, "uses")
    if not values:
        return ()
    return (values[0].split("#", 1)[0].strip(),)


STEP_RENAMES = {
    (".github/workflows/coverage.yml", "coverage", "Install Rust toolchain (stable + llvm-tools-preview)"): "Install Rust toolchain 1.91.1 (+ llvm-tools-preview)",
    (".github/workflows/lockfile-diff.yml", "lockfile-diff", "Assert the workspace-pinned toolchain (rust-toolchain.toml)"): "Assert the workspace-pinned toolchain (rust-toolchain.toml)",
    (".github/workflows/dependabot-policy.yml", "policy-gate", "Use the workspace-pinned host toolchain (provisions nothing)"): "Select the workspace-pinned host toolchain",
}


def verify_step_semantics(relative: str, job: str, candidate: list[str], baseline: list[str], channel: str) -> None:
    old_steps, new_steps = step_blocks(baseline), step_blocks(candidate)
    allowed_added = {
        (".github/workflows/coverage.yml", "coverage"): {"Install Rust toolchain 1.91.1 (+ llvm-tools-preview)"},
        (".github/workflows/lockfile-diff.yml", "lockfile-diff"): {"Install the workspace-pinned Rust toolchain"},
        (".github/workflows/dependabot-policy.yml", "policy-gate"): {"Install the workspace-pinned Rust toolchain"},
    }.get((relative, job), set())
    mapped_names: set[str] = set()
    for old_name, old_step in old_steps.items():
        key = (relative, job, old_name)
        new_name = STEP_RENAMES.get(key, old_name)
        new_step = new_steps.get(new_name)
        if new_step is None:
            fail(f"{relative}:{job}: original step {old_name!r} is missing")
        mapped_names.add(new_name)
        old_uses, new_uses = action_reference(old_step), action_reference(new_step)
        if old_uses != new_uses:
            fail(f"{relative}:{job}:{old_name}: action pin changed")
        if not commands_preserve_intent(relative, job, run_blocks(old_step), run_blocks(new_step)):
            fail(f"{relative}:{job}:{old_name}: command changed")
        if step_value(old_step, "if") != step_value(new_step, "if"):
            fail(f"{relative}:{job}:{old_name}: step condition changed")
        if step_value(old_step, "env") != step_value(new_step, "env"):
            fail(f"{relative}:{job}:{old_name}: step environment changed")
        old_with = list(step_value(old_step, "with"))
        new_with = list(step_value(new_step, "with"))
        old_with = [line for line in old_with if line != "persist-credentials: false"]
        new_with = [line for line in new_with if line != "persist-credentials: false"]
        extra_toolchain = f"toolchain: {channel}"
        new_with = [line for line in new_with if line != extra_toolchain]
        if old_with == ["with:"]:
            old_with = []
        if new_with == ["with:"]:
            new_with = []
        if old_with != new_with:
            fail(f"{relative}:{job}:{old_name}: action inputs changed")
    unexpected = set(new_steps) - mapped_names - allowed_added
    if unexpected:
        fail(f"{relative}:{job}: unexpected new workflow steps: {sorted(unexpected)}")
    for name in allowed_added:
        step = new_steps.get(name)
        if step is None or action_reference(step) != ("uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8",):
            fail(f"{relative}:{job}: required hosted toolchain setup is missing or unpinned")
        with_values = step_value(step, "with")
        if "with:" not in with_values or f"toolchain: {channel}" not in with_values:
            fail(f"{relative}:{job}: toolchain setup does not match rust-toolchain.toml")
        if relative == ".github/workflows/coverage.yml" and "components: llvm-tools-preview" not in with_values:
            fail(f"{relative}:{job}: coverage toolchain is missing llvm-tools-preview")


def verify_baseline(candidate: Path, baseline: Path) -> None:
    """Reject changes to event, permission, serialization, or command semantics."""
    for relative, expected_jobs in FILES.items():
        candidate_text = (candidate / relative).read_text(encoding="utf-8")
        baseline_text = (baseline / relative).read_text(encoding="utf-8")
        for section in ("on", "permissions"):
            if root_section(candidate_text, section) != root_section(baseline_text, section):
                fail(f"{relative}: {section} changed from the immutable baseline")

        def concurrency_values(text: str) -> tuple[str, ...]:
            values = []
            for line in root_section(text, "concurrency", required=False):
                if re.match(r"^  (group|cancel-in-progress):", line):
                    values.append(line.strip())
            return tuple(values)

        if concurrency_values(candidate_text) != concurrency_values(baseline_text):
            fail(f"{relative}: concurrency behavior changed")

        candidate_jobs = job_blocks(candidate_text, relative)
        baseline_jobs = job_blocks(baseline_text, relative)
        if candidate_jobs.keys() != expected_jobs or baseline_jobs.keys() != expected_jobs:
            fail(f"{relative}: job inventory differs from the closed issue scope")
        for job in expected_jobs:
            for setting in ("if", "timeout-minutes", "permissions", "env", "needs", "strategy", "environment", "container", "services", "continue-on-error"):
                if job_setting(candidate_jobs[job], setting) != job_setting(baseline_jobs[job], setting):
                    fail(f"{relative}:{job}: {setting} changed from the baseline")
            if not commands_preserve_intent(relative, job, run_blocks(baseline_jobs[job]), run_blocks(candidate_jobs[job])):
                fail(f"{relative}:{job}: inline command changed from the baseline")
            candidate_secrets = sorted(re.findall(r"\$\{\{\s*secrets\.[^}]+}}", "\n".join(candidate_jobs[job])))
            baseline_secrets = sorted(re.findall(r"\$\{\{\s*secrets\.[^}]+}}", "\n".join(baseline_jobs[job])))
            if candidate_secrets != baseline_secrets:
                fail(f"{relative}:{job}: secret references changed")
            channel = re.search(r"(?m)^channel\s*=\s*[\"']([^\"']+)[\"']", (candidate / "rust-toolchain.toml").read_text(encoding="utf-8"))
            if not channel:
                fail("workspace rust-toolchain.toml has no pinned channel")
            verify_step_semantics(relative, job, candidate_jobs[job], baseline_jobs[job], channel.group(1))

def verify_inventory(root: Path) -> None:
    policy_text = (root / ".github/workflows/dependabot-policy.yml").read_text(encoding="utf-8")
    if POLICY_CARGO_DENY_COMMAND not in policy_text:
        fail("Dependabot policy must retain the pinned cargo-deny command unchanged")

    for relative, expected in FILES.items():
        jobs = job_blocks((root / relative).read_text(encoding="utf-8"), relative)
        if jobs.keys() != expected:
            fail(f"{relative}: job inventory drifted: {sorted(jobs)}")
        for job, lines in jobs.items():
            runner = [line.strip() for line in lines if line.startswith("    runs-on:")]
            if runner != ["runs-on: ubuntu-24.04"]:
                fail(f"{relative}:{job}: runner must be ubuntu-24.04")

    for relative, expected in FILES.items():
        jobs = job_blocks((root / relative).read_text(encoding="utf-8"), relative)
        for job, lines in jobs.items():
            for index, line in enumerate(lines):
                if "uses: actions/checkout@" not in line:
                    continue
                indent = len(line) - len(line.lstrip())
                step_indent = indent - 2
                end = len(lines)
                for cursor in range(index + 1, len(lines)):
                    if lines[cursor].startswith(" " * step_indent + "- "):
                        end = cursor
                        break
                block = "\n".join(lines[index:end])
                if not re.search(r"(?m)^\s+persist-credentials:\s*false\s*$", block):
                    fail(f"{relative}:{job}: checkout must disable credential persistence")
                for action in re.findall(r"(?m)^\s+uses:\s*([^\s#]+)", block):
                    if action.startswith("./"):
                        continue
                    reference = action.rsplit("@", 1)[-1]
                    if SHA.fullmatch(reference) is None:
                        fail(f"{relative}:{job}: action is not pinned to a full SHA: {action}")

    texts = read_workflows(root)
    for relative, job in (
        (".github/workflows/dependabot-auto-merge.yml", "auto-merge"),
        (".github/workflows/dependabot-policy.yml", "policy-gate"),
    ):
        block = "\n".join(job_blocks(texts[relative], relative)[job])
        if "github.actor == 'dependabot[bot]'" not in block or "github.event.pull_request.user.login == 'dependabot[bot]'" not in block:
            fail(f"{relative}:{job}: Dependabot-only boundary is missing")

    label_jobs = job_blocks(texts[".github/workflows/pr-labels.yml"], ".github/workflows/pr-labels.yml")
    for job in ("label", "size"):
        block = "\n".join(label_jobs[job])
        for association in ("OWNER", "MEMBER", "COLLABORATOR"):
            if f"author_association == '{association}'" not in block:
                fail(f"pr-labels.yml:{job}: trusted-association boundary lost {association}")

    trust = texts[".github/workflows/dependabot-policy-trust-boundary.yml"]
    policy = texts[".github/workflows/dependabot-policy.yml"]
    for required in ("working-directory: _base", "--untrusted-tree", "persist-credentials: false"):
        if required not in trust:
            fail(f"dependabot trust boundary lost {required!r}")
    if "working-directory: _pr-data" in policy or "working-directory: _pr-data" in trust:
        fail("untrusted PR data is used as an executable working directory")

    for job, marker in MUTATION_MARKERS.items():
        present = any(marker in text for text in texts.values())
        if not present:
            fail(f"mutation path marker for {job} disappeared")


def verify_bot_audit_fixture(root: Path) -> None:
    """Run the workflow's actual inline audit script against a local fake gh."""
    path = root / ".github/workflows/bot-pr-has-checks.yml"
    source = path.read_text(encoding="utf-8")
    block = job_blocks(source, str(path))["audit"]
    joined = "\n".join(block)
    marker = "python3 - <<'PY'"
    if marker not in joined:
        fail("bot-pr-has-checks audit no longer has the bounded inline script")
    body = joined.split(marker, 1)[1].split("\n          PY", 1)[0]
    audit_script = textwrap.dedent(body)

    with tempfile.TemporaryDirectory(prefix="i2374-audit-") as directory:
        tmp = Path(directory)
        script = tmp / "audit.py"
        script.write_text(audit_script, encoding="utf-8")
        fakebin = tmp / "bin"
        fakebin.mkdir()
        gh = fakebin / "gh"
        gh.write_text(fake_gh_source(), encoding="utf-8")
        gh.chmod(0o755)
        base_env = {
            **os.environ,
            "PATH": str(fakebin) + os.pathsep + os.environ.get("PATH", ""),
            "REPO": "HuGR-dev/corelink-server",
            "REPOSITORY_ID": "1232040291",
            "GITHUB_STEP_SUMMARY": str(tmp / "summary.md"),
            "GH_TRACE": str(tmp / "gh-trace.jsonl"),
        }
        missing = subprocess.run(
            [sys.executable, str(script)], env={**base_env, "FIXTURE_CHECKS": "0", "FIXTURE_STATUSES": "0"},
            capture_output=True, text=True,
        )
        if missing.returncode != 1 or "NO CHECKS" not in missing.stdout:
            fail("bot audit fixture did not fail a mature PR with no checks/statuses: "
                 f"rc={missing.returncode} stdout={missing.stdout[-250:]!r} stderr={missing.stderr[-250:]!r}")
        present = subprocess.run(
            [sys.executable, str(script)], env={**base_env, "FIXTURE_CHECKS": "1", "FIXTURE_STATUSES": "0"},
            capture_output=True, text=True,
        )
        if present.returncode != 0 or "present (not proof of success)" not in present.stdout:
            fail("bot audit fixture did not pass a PR with a present check run")


def verify_read_only_fixtures(root: Path) -> None:
    verify_bot_audit_fixture(root)
    subprocess.run(["bash", "scripts/test_dependabot_policy_trust_boundary.sh"], cwd=root, check=True)
    subprocess.run([sys.executable, "scripts/validate_permission_matrix.py", "--self-test"], cwd=root, check=True)
    subprocess.run([sys.executable, "scripts/validate_permission_matrix.py"], cwd=root, check=True)
    subprocess.run(["bash", "scripts/test_ci_use_host_toolchain.sh"], cwd=root, check=True)


def named_run_block(root: Path, relative: str, job: str, step_name: str) -> str:
    lines = job_blocks((root / relative).read_text(encoding="utf-8"), relative)[job]
    wanted = "      - name: " + step_name
    try:
        start = next(index for index, line in enumerate(lines) if line == wanted)
    except StopIteration:
        fail(f"{relative}:{job}: missing proof step {step_name!r}")
    run_index = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("        run:")), None)
    if run_index is None:
        fail(f"{relative}:{job}:{step_name}: missing run block")
    declaration = lines[run_index].split("run:", 1)[1].strip()
    body = []
    if declaration in {"|", ">-", "|-", ">"}:
        for line in lines[run_index + 1:]:
            if line.strip() and len(line) - len(line.lstrip()) <= 8:
                break
            body.append(line)
        return textwrap.dedent("\n".join(body))
    return declaration


def fake_gh_source() -> str:
    return """#!/usr/bin/env python3
import datetime, json, os, sys
args = sys.argv[1:]
joined = " ".join(args)
verb = args[0] if args else ""
sub = args[1] if len(args) > 1 else ""
write = ((verb == "pr" and sub in {"merge", "edit", "comment"}) or
         (verb == "issue" and sub == "comment") or
         (verb == "label" and sub == "create"))
record = {"args": args, "method": "write" if write else "read",
          "outcome": "denied" if write and os.environ.get("DENY_WRITES") == "1" else "simulated"}
with open(os.environ["GH_TRACE"], "a", encoding="utf-8") as stream:
    stream.write(json.dumps(record) + "\\n")
if write:
    # A successful response is simulated locally; it never contacts GitHub.
    if os.environ.get("DENY_WRITES") == "1":
        print("mock GitHub denied write: " + joined, file=sys.stderr)
        raise SystemExit(86)
    print("MOCK-GITHUB: " + joined)
    raise SystemExit(0)
if verb == "api":
    path = next((arg for arg in args if arg.startswith("repos/") or arg.startswith("search/")), args[-1])
    if path.endswith("/pulls?state=open&per_page=100"):
        created = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=30)).strftime("%Y-%m-%dT%H:%M:%SZ")
        print(json.dumps([{"number":314,"title":"fixture","user":{"login":"fixture"},"created_at":created,"head":{"ref":"bot/api-reference-sync-fixture","sha":"a"*40}}]))
    elif "/check-runs?" in path:
        print(json.dumps({"total_count": int(os.environ.get("FIXTURE_CHECKS", "1")), "check_runs": [
            {"name":"dco"},
            {"name":"PR gate (crate-scoped clippy + tests)"},
            {"name":"cargo-deny (license + advisories + sources + bans)"},
            {"name":"cargo-audit PR gate"},
            {"name":"secrets-matrix drift gate"}
        ]}))
    elif "/status?" in path:
        print(json.dumps({"total_count": int(os.environ.get("FIXTURE_STATUSES", "0"))}))
    elif "pulls/314/files" in path:
        print("src/example.rs\\t3\\t2")
    elif "search/issues" in path:
        print("1" if "--jq" in args else json.dumps({"total_count": 1}))
    else:
        raise SystemExit("unexpected read-only gh API path: " + path)
    raise SystemExit(0)
if verb in {"pr", "issue"} and sub == "view":
    if "--jq" in args:
        print("")
    else:
        print(json.dumps({"labels": [], "comments": []}))
    raise SystemExit(0)
raise SystemExit("unexpected gh command: " + joined)
"""


class LocalGitHubHandler(BaseHTTPRequestHandler):
    """In-memory REST surface used to execute the pinned labeler and stale actions."""

    requests: list[dict[str, object]] = []
    labels: set[str] = set()
    now = datetime.now(timezone.utc)

    def log_message(self, *_args: object) -> None:
        return

    def _send(self, status: int, payload: object) -> None:
        encoded = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _record(self) -> dict[str, object]:
        raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        try:
            body: object = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            body = raw.decode("utf-8", errors="replace")
        record = {"method": self.command, "path": urlsplit(self.path).path, "body": body}
        self.requests.append(record)
        return record

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._record()
        path = urlsplit(self.path).path
        if path.endswith("/pulls/314/files"):
            self._send(200, [{"filename": "infra/staging/topology.json", "additions": 3, "deletions": 2, "status": "modified"}])
        elif path.endswith("/pulls/314"):
            self._send(200, {"number": 314, "title": "fixture", "draft": False, "labels": [], "base": {"ref": "main"}, "head": {"ref": "fixture", "sha": "a" * 40}})
        elif path.endswith("/issues/314/labels"):
            self._send(200, [{"name": label} for label in sorted(self.labels)])
        elif path.endswith("/labels"):
            self._send(200, [{"name": label, "color": "ededed"} for label in sorted(self.labels)])
        elif path.endswith("/issues"):
            page = int(parse_qs(urlsplit(self.path).query).get("page", ["1"])[0])
            if page > 1:
                self._send(200, [])
                return
            old = lambda days: (self.now - timedelta(days=days)).strftime("%Y-%m-%dT%H:%M:%SZ")
            self._send(200, [
                {"number": 315, "title": "stale fixture", "body": "", "state": "open", "created_at": old(80), "updated_at": old(80), "labels": [], "assignees": [], "milestone": None, "pull_request": None},
                {"number": 316, "title": "close fixture", "body": "", "state": "open", "created_at": old(100), "updated_at": old(80), "labels": [{"name": "stale"}], "assignees": [], "milestone": None, "pull_request": None},
            ])
        elif "/comments" in path:
            self._send(200, [])
        else:
            self._send(200, {})

    def _write(self) -> None:
        record = self._record()
        method = str(record["method"])
        path = str(record["path"])
        body = record["body"]
        if path.endswith("/issues/314/labels") and isinstance(body, dict):
            self.labels.update(str(label) for label in body.get("labels", []))
        # Writes affect only this process-local fixture and are discarded on exit.
        payload = {"number": 314, "labels": [{"name": label} for label in sorted(self.labels)]}
        self._send(200 if method != "POST" else 201, payload)

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._write()

    def do_PUT(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._write()

    def do_PATCH(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._write()

    def do_DELETE(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._write()


def action_step_inputs(root: Path, relative: str, job: str, step_name: str) -> dict[str, str]:
    lines = job_blocks((root / relative).read_text(encoding="utf-8"), relative)[job]
    wanted = "      - name: " + step_name
    try:
        start = next(index for index, line in enumerate(lines) if line == wanted)
    except StopIteration:
        fail(f"{relative}:{job}: missing action step {step_name!r}")
    end = next((index for index in range(start + 1, len(lines)) if lines[index].startswith("      - ")), len(lines))
    step = lines[start:end]
    try:
        with_index = next(index for index, line in enumerate(step) if line == "        with:")
    except StopIteration:
        return {}
    inputs: dict[str, str] = {}
    index = with_index + 1
    while index < len(step) and (not step[index].strip() or step[index].startswith("          ")):
        match = re.match(r"^          ([a-zA-Z0-9_-]+):(?:\s*(.*))?$", step[index])
        if not match:
            index += 1
            continue
        key, value = match.group(1), (match.group(2) or "")
        if value in {"|", "|-", ">", ">-"}:
            body = []
            index += 1
            while index < len(step) and (not step[index].strip() or step[index].startswith("            ")):
                body.append(step[index][12:] if step[index].startswith("            ") else "")
                index += 1
            inputs[key] = "\n".join(body).rstrip()
            continue
        inputs[key] = value.strip("'\"")
        index += 1
    return inputs


def run_pinned_action(action_root: Path, action: str, root: Path, api_url: str, event_path: Path, tmp: Path) -> None:
    action_dir = action_root / action
    metadata = (action_dir / "action.yml").read_text(encoding="utf-8")
    main = re.search(r"(?m)^\s*main:\s*['\"]?([^\s'\"]+)", metadata)
    if not main:
        fail(f"{action}: action.yml does not declare a node entrypoint")
    entrypoint = action_dir / main.group(1)
    if not entrypoint.is_file():
        fail(f"{action}: pinned entrypoint is missing: {entrypoint}")
    output_path = tmp / f"{action}-output.txt"
    output_path.touch()
    env = {
        **os.environ,
        "GITHUB_API_URL": api_url,
        "GITHUB_SERVER_URL": api_url.rsplit("/api/v3", 1)[0],
        "GITHUB_REPOSITORY": "HuGR-dev/corelink-server",
        "GITHUB_EVENT_NAME": "pull_request_target",
        "GITHUB_EVENT_PATH": str(event_path),
        "GITHUB_WORKSPACE": str(root),
        "GITHUB_RUN_ID": "2374",
        "GITHUB_RUN_NUMBER": "1",
        "GITHUB_OUTPUT": str(output_path),
        "GITHUB_TOKEN": "fixture-token-never-valid-outside-local-mock",
        "INPUT_REPO-TOKEN": "fixture-token-never-valid-outside-local-mock",
    }
    workflow, job, step = {
        "labeler": (".github/workflows/pr-labels.yml", "label", "Apply labels"),
        "stale": (".github/workflows/stale.yml", "stale", "Mark + close stale items"),
    }[action]
    for key, value in action_step_inputs(root, workflow, job, step).items():
        env["INPUT_" + key.upper().replace("_", "-")] = value
    env["INPUT_REPO-TOKEN"] = "fixture-token-never-valid-outside-local-mock"
    result = subprocess.run(["node", str(entrypoint)], cwd=root, env=env, capture_output=True, text=True)
    if result.returncode != 0:
        fail(f"{action}: pinned action failed against local API mock: {result.stderr[-1000:]} {result.stdout[-500:]}; local requests={LocalGitHubHandler.requests[-12:]}")


def verify_pinned_mutating_actions(root: Path, action_root: Path) -> None:
    labels_file = root / ".github/labeler.yml"
    labels = set(re.findall(r"(?m)^\"([^\"]+)\":$", labels_file.read_text(encoding="utf-8")))
    event_path = root / "target/issue-2374-event.json"
    event_path.parent.mkdir(parents=True, exist_ok=True)
    event_path.write_text(json.dumps({
        "repository": {"full_name": "HuGR-dev/corelink-server"},
        "pull_request": {"number": 314, "title": "fixture", "draft": False, "labels": [], "base": {"ref": "main"}, "head": {"ref": "fixture", "sha": "a" * 40}},
    }), encoding="utf-8")
    LocalGitHubHandler.requests = []
    LocalGitHubHandler.labels = labels | {"size:XS", "stale"}
    server = ThreadingHTTPServer(("127.0.0.1", 0), LocalGitHubHandler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    api_url = f"http://127.0.0.1:{server.server_port}/api/v3"
    try:
        with tempfile.TemporaryDirectory(prefix="i2374-actions-") as directory:
            tmp = Path(directory)
            run_pinned_action(action_root, "labeler", root, api_url, event_path, tmp)
            run_pinned_action(action_root, "stale", root, api_url, event_path, tmp)
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=2)
        event_path.unlink(missing_ok=True)
    writes = [request for request in LocalGitHubHandler.requests if request["method"] in {"POST", "PUT", "PATCH", "DELETE"}]
    serialized = json.dumps(writes)
    all_requests = json.dumps(LocalGitHubHandler.requests)
    if "/pulls/314/files" not in all_requests or "area:infra" not in serialized:
        fail(f"pinned labeler did not send its expected fixture label request: {LocalGitHubHandler.requests}")
    if "/issues/315/labels" not in serialized or "/issues/316" not in serialized:
        fail("pinned stale action did not mark and close the aged issue fixtures")
    if any("api.github.com" in str(request) for request in LocalGitHubHandler.requests):
        fail("a pinned action escaped the local GitHub API mock")


def run_gh_script(script: str, fakebin: Path, trace: Path, env: dict[str, str], *, deny: bool) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", "-euo", "pipefail", "-c", script],
        env={**os.environ, **env, "PATH": str(fakebin) + os.pathsep + os.environ.get("PATH", ""),
             "GH_TRACE": str(trace), "DENY_WRITES": "1" if deny else "0"},
        capture_output=True, text=True,
    )


def verify_original_write_commands(root: Path) -> None:
    """Execute the workflow's actual mutating shell commands against local gh."""
    with tempfile.TemporaryDirectory(prefix="i2374-writes-") as directory:
        tmp = Path(directory)
        fakebin = tmp / "bin"
        fakebin.mkdir()
        gh = fakebin / "gh"
        gh.write_text(fake_gh_source(), encoding="utf-8")
        gh.chmod(0o755)
        trace = tmp / "gh-trace.jsonl"

        cases = [
            (".github/workflows/dependabot-auto-merge.yml", "auto-merge", "Enable auto-merge (security-patch only)",
             {"GITHUB_TOKEN":"fixture", "PR_URL":"https://example.invalid/pr/314", "DEP_NAMES":"fixture", "ECOSYSTEM":"cargo"}, [("pr", "merge")]),
            (".github/workflows/pr-labels.yml", "size", "Apply size:* label",
             {"GH_TOKEN":"fixture", "PR":"314", "REPO":"HuGR-dev/corelink-server"}, [("label", "create"), ("pr", "edit")]),
            (".github/workflows/welcome-first-pr.yml", "welcome", "Greet first-time contributor",
             {"GH_TOKEN":"fixture", "REPO":"HuGR-dev/corelink-server", "EVENT":"pull_request_target", "NUMBER":"314", "AUTHOR":"FixtureHuman", "ISSUE_MESSAGE":"fixture issue", "PR_MESSAGE":"fixture PR"}, [("pr", "comment")]),
            (".github/workflows/lockfile-diff.yml", "lockfile-diff", "Post PR comment with diff + cargo-deny summary",
             {"GITHUB_TOKEN":"fixture", "PR_NUMBER":"314", "REPO":"HuGR-dev/corelink-server", "TOTAL_CHANGED":"1", "DENY_EXIT":"0"}, [("pr", "comment")]),
        ]
        for relative, job, step, env, expected_writes in cases:
            trace.unlink(missing_ok=True)
            script = named_run_block(root, relative, job, step)
            script = script.replace("${{github.server_url}}", "https://github.com").replace("${{github.run_id}}", "1")
            if relative == ".github/workflows/lockfile-diff.yml":
                (tmp / "deny-output.txt").write_text("fixture cargo-deny output\n", encoding="utf-8")
            completed = run_gh_script(script, fakebin, trace, env, deny=False)
            if completed.returncode != 0:
                fail(f"{relative}:{step}: local GitHub fixture failed: {completed.stderr[-500:]}")
            records = [line for line in trace.read_text(encoding="utf-8").splitlines() if line]
            if not records:
                fail(f"{relative}:{step}: fake GitHub did not record a request")
            observed = [(item["args"][0], item["args"][1]) for item in map(json.loads, records) if item["method"] == "write"]
            if observed != expected_writes:
                fail(f"{relative}:{step}: expected mocked write requests {expected_writes}, got {observed}")

            trace.unlink(missing_ok=True)
            negative = run_gh_script(script, fakebin, trace, env, deny=True)
            denied = [json.loads(line) for line in trace.read_text(encoding="utf-8").splitlines() if line]
            denied_writes = [record for record in denied if record["method"] == "write"]
            if not denied_writes or any(record["outcome"] != "denied" for record in denied_writes):
                fail(f"{relative}:{step}: negative control did not stop at a write request")
            # A helper may deliberately degrade a comment failure to a warning;
            # the decisive control is that every attempted write was denied.
            if negative.returncode == 0 and relative != ".github/workflows/lockfile-diff.yml":
                fail(f"{relative}:{step}: write-denial negative control was ignored")

        eligibility = named_run_block(root, ".github/workflows/dependabot-auto-merge.yml", "auto-merge", "Compute auto-merge eligibility")
        output_path = tmp / "eligibility-output.txt"
        selected = subprocess.run(["bash", "-euo", "pipefail", "-c", eligibility],
                                  env={**os.environ, "UPDATE_TYPE": "version-update:semver-patch",
                                       "PR_TITLE": "[security] fixture update", "LABELS_JSON": "[]",
                                       "GITHUB_OUTPUT": str(output_path)}, capture_output=True, text=True)
        decision = output_path.read_text(encoding="utf-8") if output_path.is_file() else ""
        if selected.returncode != 0 or "eligible=true" not in decision or "is_security=true" not in decision:
            fail("Dependabot auto-merge decision command did not select the security-patch fixture")

        sentinel = named_run_block(root, ".github/workflows/dependabot-policy.yml", "sentinel", "Pass-through (non-dependabot PR)")
        passed = subprocess.run(["bash", "-euo", "pipefail", "-c", sentinel], capture_output=True, text=True)
        if passed.returncode != 0 or "policy gate not applicable" not in passed.stdout:
            fail("Dependabot sentinel command did not execute its non-Dependabot pass path")

        # Execute the lockfile generator itself over disposable before/after inputs.
        diff_root = tmp / "lockfiles"
        (diff_root / "base").mkdir(parents=True)
        (diff_root / "head").mkdir()
        (diff_root / "base/Cargo.lock").write_text('[[package]]\nname = "fixture"\nversion = "1.0.0"\n', encoding="utf-8")
        (diff_root / "head/Cargo.lock").write_text('[[package]]\nname = "fixture"\nversion = "1.0.0"\n\n[[package]]\nname = "fixture-added"\nversion = "2.0.0"\n', encoding="utf-8")
        generator = named_run_block(root, ".github/workflows/lockfile-diff.yml", "lockfile-diff", "Generate Cargo.lock diff")
        generated = subprocess.run(["bash", "-euo", "pipefail", "-c", generator], cwd=diff_root,
                                   env={**os.environ, "GITHUB_OUTPUT": str(diff_root / "output"),
                                        "GITHUB_STEP_SUMMARY": str(diff_root / "summary")},
                                   capture_output=True, text=True)
        if generated.returncode != 0 or "+1 added" not in generated.stdout:
            fail(f"lockfile-diff generator failed on disposable lockfiles: {generated.stderr[-500:]} {generated.stdout[-300:]}")
        if "total_changed=1" not in (diff_root / "output").read_text(encoding="utf-8"):
            fail("lockfile-diff generator did not emit the expected change count")

        # Execute the policy-gate status-check command against a two-commit PR fixture.
        git_root = tmp / "policy-pr"
        git_root.mkdir()
        subprocess.run(["git", "init", "-q", str(git_root)], check=True)
        subprocess.run(["git", "-C", str(git_root), "config", "user.name", "issue-2374 fixture"], check=True)
        subprocess.run(["git", "-C", str(git_root), "config", "user.email", "fixture@example.invalid"], check=True)
        (git_root / "Cargo.lock").write_text("fixture base\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(git_root), "add", "Cargo.lock"], check=True)
        subprocess.run(["git", "-C", str(git_root), "commit", "-qm", "base"], check=True)
        (git_root / "Cargo.lock").write_text("fixture candidate\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(git_root), "commit", "-qam", "fixture dependency update"], check=True)
        trace.unlink(missing_ok=True)
        required_check = named_run_block(root, ".github/workflows/dependabot-policy.yml", "policy-gate", "Verify required checks are present")
        checked = run_gh_script(required_check, fakebin, trace,
                                {"GH_TOKEN": "fixture", "REPO": "HuGR-dev/corelink-server",
                                 "PR_HEAD_SHA": "a" * 40, "UNTRUSTED_TREE": str(git_root)}, deny=False)
        if checked.returncode != 0 or "All 5 resolved required status checks are present" not in checked.stdout:
            fail(f"Dependabot policy check command failed against fixture API: {checked.stderr[-500:]} {checked.stdout[-500:]}")

        if shutil.which("cargo-deny"):
            trace.unlink(missing_ok=True)
            lock_deny = named_run_block(root, ".github/workflows/lockfile-diff.yml", "lockfile-diff", "Run cargo-deny (license summary)")
            deny_output = tmp / "lockfile-deny-output.txt"
            deny_run = subprocess.run(["bash", "-euo", "pipefail", "-c", lock_deny], cwd=root,
                                      env={**os.environ, "GITHUB_OUTPUT": str(deny_output)}, capture_output=True, text=True)
            if deny_run.returncode != 0 or not re.search(r"(?m)^exit_code=\d+$", deny_output.read_text(encoding="utf-8")):
                fail(f"lockfile cargo-deny command did not produce its workflow outputs: {deny_run.stderr[-500:]}")


def _safe_fixture_target_path(policy_root: Path, manifest_dir: Path, raw_path: object, label: str) -> Path:
    if not isinstance(raw_path, str) or not raw_path or "\\" in raw_path:
        fail(f"{label}: Cargo target path is not a safe relative path")
    posix_path = PurePosixPath(raw_path)
    windows_path = PureWindowsPath(raw_path)
    if posix_path.is_absolute() or windows_path.is_absolute() or windows_path.drive or ".." in posix_path.parts:
        fail(f"{label}: Cargo target path escapes the disposable policy tree")
    if not posix_path.parts:
        fail(f"{label}: Cargo target path is empty")

    policy_root_resolved = policy_root.resolve(strict=True)
    manifest_dir_resolved = manifest_dir.resolve(strict=True)
    if not manifest_dir_resolved.is_relative_to(policy_root_resolved):
        fail(f"{label}: manifest directory escaped the disposable policy tree")
    target = manifest_dir.joinpath(*posix_path.parts)
    current = policy_root
    for part in target.relative_to(policy_root).parts[:-1]:
        current = current / part
        if current.is_symlink():
            fail(f"{label}: symlink in Cargo target path")
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.resolve().is_relative_to(policy_root_resolved):
        fail(f"{label}: Cargo target resolved outside the disposable policy tree")
    if target.is_symlink() or target.exists():
        fail(f"{label}: unexpected existing Cargo target in the manifest-only fixture")
    return target


def materialize_minimal_cargo_targets(policy_root: Path) -> tuple[int, int]:
    """Add valid source targets to the disposable tree without changing copied manifests."""
    manifests = sorted(policy_root.rglob("Cargo.toml"))
    if not manifests:
        fail("Cargo policy fixture contains no copied manifests")

    package_count = 0
    target_count = 0
    for manifest in manifests:
        try:
            document = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as error:
            fail(f"copied Cargo manifest {manifest.relative_to(policy_root)} is invalid: {error}")
        package = document.get("package")
        if package is None:
            if not isinstance(document.get("workspace"), dict):
                fail(f"copied Cargo manifest {manifest.relative_to(policy_root)} is neither a package nor a workspace")
            continue
        if not isinstance(package, dict) or not isinstance(package.get("name"), str):
            fail(f"copied Cargo manifest {manifest.relative_to(policy_root)} has no valid package name")
        package_count += 1
        manifest_dir = manifest.parent
        declared: dict[str, str] = {}

        library = document.get("lib")
        if library is not None:
            if not isinstance(library, dict):
                fail(f"{manifest.relative_to(policy_root)}: [lib] must be a table")
            relative = library.get("path", "src/lib.rs")
            if not isinstance(relative, str):
                fail(f"{manifest.relative_to(policy_root)}: [lib] target path is not text")
            declared[relative] = "lib"
        elif package.get("autolib", True):
            declared["src/lib.rs"] = "lib"

        target_kinds = (
            ("bin", "src/bin"),
            ("example", "examples"),
            ("test", "tests"),
            ("bench", "benches"),
        )
        for kind, directory in target_kinds:
            entries = document.get(kind, [])
            if not isinstance(entries, list) or any(not isinstance(entry, dict) for entry in entries):
                fail(f"{manifest.relative_to(policy_root)}: {kind} targets must be array-of-tables")
            for entry in entries:
                name = entry.get("name")
                if not isinstance(name, str) or not name:
                    fail(f"{manifest.relative_to(policy_root)}: explicit {kind} target has no name")
                if "path" in entry:
                    relative = entry["path"]
                elif kind == "bin" and name == package["name"]:
                    relative = "src/main.rs"
                else:
                    relative = f"{directory}/{name}.rs"
                if not isinstance(relative, str):
                    fail(f"{manifest.relative_to(policy_root)}: explicit {kind} target path is not text")
                if relative in declared:
                    fail(f"{manifest.relative_to(policy_root)}: duplicate Cargo target path {relative!r}")
                declared[relative] = kind

        if not declared:
            for target_key, directory, filename, kind in (
                ("autobins", "src", "main.rs", "bin"),
                ("autoexamples", "examples", "issue-2374-fixture.rs", "example"),
                ("autotests", "tests", "issue-2374-fixture.rs", "test"),
                ("autobenches", "benches", "issue-2374-fixture.rs", "bench"),
            ):
                if package.get(target_key, True):
                    declared[f"{directory}/{filename}"] = kind
                    break
        if not declared:
            fail(f"{manifest.relative_to(policy_root)}: package declares no Cargo targets")

        for relative, kind in sorted(declared.items()):
            label = f"{manifest.relative_to(policy_root)} [{kind}]"
            target = _safe_fixture_target_path(policy_root, manifest_dir, relative, label)
            contents = "pub fn issue_2374_fixture_target() {}\n" if kind == "lib" else "fn main() {}\n"
            target.write_text(contents, encoding="utf-8")
            target.chmod(0o444)
            target_count += 1

    if package_count == 0 or target_count == 0:
        fail("Cargo policy fixture has no package manifests with materialized targets")
    print(f"materialized {target_count} minimal Cargo targets for {package_count} copied package manifests")
    return package_count, target_count


def verify_policy_gate_cargo_deny(root: Path, baseline: Path) -> None:
    """Run both original Dependabot policy cargo-deny steps on isolated PR data."""
    if not shutil.which("cargo-deny"):
        fail("cargo-deny is not installed in the hosted proof job")
    with tempfile.TemporaryDirectory(prefix="i2374-policy-") as directory:
        tmp = Path(directory)
        env = {
            **os.environ,
            "UNTRUSTED_TREE": str(root),
            "POLICY_TREE": str(tmp / "policy-tree"),
            "CARGO_HOME": str(tmp / "cargo-home"),
            "DENY_CONFIG": str(tmp / "deny.toml"),
            "HOME": str(tmp / "home"),
        }
        prepare = named_run_block(root, ".github/workflows/dependabot-policy.yml", "policy-gate", "Prepare isolated Cargo policy tree (PR data only)")
        prepared = subprocess.run(["bash", "-euo", "pipefail", "-c", prepare], cwd=baseline, env=env, capture_output=True, text=True)
        if prepared.returncode != 0:
            fail(f"Dependabot policy fixture preparation failed: {prepared.stderr[-1000:]}")
        materialize_minimal_cargo_targets(Path(env["POLICY_TREE"]))
        policy = named_run_block(root, ".github/workflows/dependabot-policy.yml", "policy-gate", "Run cargo-deny licenses (fail-closed)")
        checked = subprocess.run(["bash", "-euo", "pipefail", "-c", policy], cwd=baseline, env=env, capture_output=True, text=True)
        if checked.returncode != 0:
            fail(f"Dependabot policy cargo-deny command failed: {checked.stderr[-1000:]} {checked.stdout[-500:]}")


def verify_negative_controls(root: Path, baseline: Path | None) -> None:
    with tempfile.TemporaryDirectory(prefix="i2374-negative-") as directory:
        temp_root = Path(directory)
        for relative in FILES:
            target = temp_root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((root / relative).read_bytes())
        (temp_root / "rust-toolchain.toml").write_bytes((root / "rust-toolchain.toml").read_bytes())

        mutations = [
            (".github/workflows/bot-pr-has-checks.yml", "runs-on: ubuntu-24.04", "runs-on: corelink", verify_inventory),
            (".github/workflows/coverage.yml", "persist-credentials: false", "persist-credentials: true", verify_inventory),
            (".github/workflows/dependabot-policy.yml", "github.event.pull_request.user.login == 'dependabot[bot]'", "github.event.pull_request.user.login != 'dependabot[bot]'", verify_inventory),
            (".github/workflows/pr-labels.yml", "author_association == 'OWNER'", "author_association == 'EXTERNAL'", verify_inventory),
            (".github/workflows/bot-pr-has-checks.yml", "workflow_dispatch: {}", "workflow_dispatch: { types: [completed] }", verify_baseline),
            (".github/workflows/permission-matrix.yml", "  contents: read", "  contents: write", verify_baseline),
            (".github/workflows/lockfile-diff.yml", "gh', 'pr', 'comment", "gh', 'pr', 'edit", verify_baseline),
        ]
        for relative, old, new, check in mutations:
            target = temp_root / relative
            original = target.read_text(encoding="utf-8")
            if old not in original:
                fail(f"negative-control fixture lost mutation anchor {old!r} in {relative}")
            target.write_text(original.replace(old, new, 1), encoding="utf-8")
            try:
                if check is verify_baseline:
                    if baseline is None:
                        # These cases are exercised whenever the hosted job
                        # supplies its exact immutable base checkout.
                        target.write_text(original, encoding="utf-8")
                        continue
                    check(temp_root, baseline)
                else:
                    check(temp_root)
            except ContractError:
                target.write_text(original, encoding="utf-8")
                continue
            fail(f"negative control escaped: {relative} mutation {old!r}")


def self_test(root: Path, baseline: Path | None) -> None:
    verify_inventory(root)
    verify_negative_controls(root, baseline)
    verify_read_only_fixtures(root)
    verify_original_write_commands(root)


def verify_coverage_receipt(root: Path) -> None:
    artifact_id = os.environ.get("COVERAGE_ARTIFACT_ID", "")
    artifact_url = os.environ.get("COVERAGE_ARTIFACT_URL", "")
    artifact_digest = os.environ.get("COVERAGE_ARTIFACT_DIGEST", "")
    if not artifact_id.isdigit():
        fail("coverage artifact receipt is missing a numeric artifact id")
    if not artifact_url.startswith("https://github.com/") or "/actions/runs/" not in artifact_url:
        fail("coverage artifact receipt URL does not identify this GitHub Actions run")
    if re.fullmatch(r"[0-9a-f]{64}", artifact_digest) is None:
        fail("coverage artifact receipt is missing the SHA-256 digest")
    if not (root / "target/coverage/html/index.html").is_file():
        fail("coverage HTML report is missing after the upload step")
    if not (root / "target/coverage/SUMMARY.txt").is_file():
        fail("coverage summary is missing after the upload step")
    print(f"coverage artifact receipt: PASS id={artifact_id} sha256={artifact_digest}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--baseline-root", type=Path)
    parser.add_argument("--actions-root", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--verify-coverage-receipt", action="store_true")
    args = parser.parse_args()
    if args.verify_coverage_receipt:
        try:
            verify_coverage_receipt(args.root)
        except ContractError as error:
            print(f"issue-2374 hosted proof: FAIL: {error}", file=sys.stderr)
            return 1
        return 0
    try:
        if args.self_test:
            if args.expected_head:
                fail("--self-test cannot be combined with --expected-head")
            self_test(args.root, args.baseline_root)
        else:
            if not args.expected_head or SHA.fullmatch(args.expected_head) is None:
                fail("expected-head must be a full lowercase SHA")
            actual = subprocess.run(
                ["git", "-C", str(args.root), "rev-parse", "HEAD"],
                check=True, capture_output=True, text=True,
            ).stdout.strip()
            if actual != args.expected_head:
                fail(f"candidate HEAD {actual} does not equal {args.expected_head}")
            verify_inventory(args.root)
            if args.baseline_root:
                verify_baseline(args.root, args.baseline_root)
                verify_negative_controls(args.root, args.baseline_root)
            verify_read_only_fixtures(args.root)
            verify_original_write_commands(args.root)
            if not args.actions_root:
                fail("exact hosted proof requires --actions-root with the pinned labeler and stale sources")
            verify_pinned_mutating_actions(args.root, args.actions_root)
            if not args.baseline_root:
                fail("exact hosted proof requires --baseline-root for Dependabot policy execution")
            verify_policy_gate_cargo_deny(args.root, args.baseline_root)
    except (ContractError, subprocess.CalledProcessError) as error:
        print(f"issue-2374 hosted proof: FAIL: {error}", file=sys.stderr)
        return 1
    print("issue-2374 hosted proof: PASS (original workflow commands executed against a disposable GitHub CLI mock; write requests remained local)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
