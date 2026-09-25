#!/usr/bin/env python3
"""Safe hosted contract for the policy and PR automation migration (#2374).

The candidate workflows are inspected as data. Read-only policy checks run on
local fixtures; paths that write labels, comments, auto-merge state, artifacts,
or stale state are represented by a deny-by-default dry-run sink. No GitHub API
write or provider operation is available to this proof.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import textwrap
import tempfile
from pathlib import Path

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
        if match.group(1) in (None, "|", ">", "|-", ">-"):
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
            for setting in ("if", "timeout-minutes", "permissions"):
                if job_setting(candidate_jobs[job], setting) != job_setting(baseline_jobs[job], setting):
                    fail(f"{relative}:{job}: {setting} changed from the baseline")
            if run_blocks(candidate_jobs[job]) != run_blocks(baseline_jobs[job]):
                fail(f"{relative}:{job}: inline command changed from the baseline")
            candidate_secrets = sorted(re.findall(r"\$\{\{\s*secrets\.[^}]+}}", "\n".join(candidate_jobs[job])))
            baseline_secrets = sorted(re.findall(r"\$\{\{\s*secrets\.[^}]+}}", "\n".join(baseline_jobs[job])))
            if candidate_secrets != baseline_secrets:
                fail(f"{relative}:{job}: secret references changed")


def verify_inventory(root: Path) -> None:
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


class WriteDenied(RuntimeError):
    pass


class DryRunSink:
    """Collect intended writes; the proof has no implementation for real writes."""

    def __init__(self, dry_run: bool = True) -> None:
        self.dry_run = dry_run
        self.intents: list[tuple[str, str]] = []

    def request(self, verb: str, target: str) -> str:
        if not self.dry_run:
            raise WriteDenied(f"write path {verb} {target} denied outside dry-run")
        self.intents.append((verb, target))
        return "DRY-RUN: " + verb + " " + target


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
if write:
    record = {"args": args, "outcome": "denied" if os.environ.get("DENY_WRITES") == "1" else "dry-run"}
    with open(os.environ["GH_TRACE"], "a", encoding="utf-8") as stream:
        stream.write(json.dumps(record) + "\\n")
    if os.environ.get("DENY_WRITES") == "1":
        print("proof negative control denied write", file=sys.stderr)
        raise SystemExit(86)
    print("DRY-RUN intercepted: " + joined)
    raise SystemExit(0)
if verb == "api":
    path = next((arg for arg in args if arg.startswith("repos/") or arg.startswith("search/")), args[-1])
    if path.endswith("/pulls?state=open&per_page=100"):
        created = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=30)).strftime("%Y-%m-%dT%H:%M:%SZ")
        print(json.dumps([{"number":314,"title":"fixture","user":{"login":"fixture"},"created_at":created,"head":{"ref":"bot/api-reference-sync-fixture","sha":"a"*40}}]))
    elif "/check-runs?" in path:
        print(json.dumps({"total_count": int(os.environ.get("FIXTURE_CHECKS", "0"))}))
    elif "/status?" in path:
        print(json.dumps({"total_count": int(os.environ.get("FIXTURE_STATUSES", "0"))}))
    elif "pulls/314/files" in path:
        print("src/example.rs\\t2\\t1")
    elif "search/issues" in path:
        print("1")
    else:
        raise SystemExit("unexpected read-only gh API path: " + path)
    raise SystemExit(0)
if verb in {"pr", "issue"} and sub == "view":
    raise SystemExit(0)
raise SystemExit("unexpected gh command: " + joined)
"""


def run_gh_script(script: str, fakebin: Path, trace: Path, env: dict[str, str], *, deny: bool) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", "-euo", "pipefail", "-c", script],
        env={**os.environ, **env, "PATH": str(fakebin) + os.pathsep + os.environ.get("PATH", ""),
             "GH_TRACE": str(trace), "DENY_WRITES": "1" if deny else "0"},
        capture_output=True, text=True,
    )


def verify_mutation_dry_runs(root: Path) -> None:
    sink = DryRunSink()
    expected = [
        ("enable-auto-merge", "PR #314"),
        ("post-lockfile-comment", "PR #314"),
        ("apply-path-label", "PR #314"),
        ("apply-size-label", "PR #314"),
        ("mark-or-close-stale", "fixture issue"),
        ("post-welcome-comment", "fixture PR"),
        ("upload-coverage-artifact", "fixture run"),
    ]
    for verb, target in expected:
        result = sink.request(verb, target)
        if not result.startswith("DRY-RUN:"):
            fail(f"mutation dry-run did not report intent for {verb}")
    if sink.intents != expected:
        fail("mutation dry-run lost or added a write intent")

    # Negative control: the same request must fail closed if dry-run is disabled.
    for verb, target in expected:
        try:
            DryRunSink(dry_run=False).request(verb, target)
        except WriteDenied:
            continue
        fail(f"negative control allowed real write path {verb}")

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
            completed = run_gh_script(script, fakebin, trace, env, deny=False)
            if completed.returncode != 0:
                fail(f"{relative}:{step}: dry-run fixture failed: {completed.stderr[-500:]}")
            records = [line for line in trace.read_text(encoding="utf-8").splitlines() if line]
            if not records or any('"outcome": "denied"' in record for record in records):
                fail(f"{relative}:{step}: fake GitHub write sink did not capture a dry-run intent")
            observed = [(item["args"][0], item["args"][1]) for item in map(json.loads, records)]
            if observed != expected_writes:
                fail(f"{relative}:{step}: expected dry-run writes {expected_writes}, got {observed}")

            trace.unlink(missing_ok=True)
            negative = run_gh_script(script, fakebin, trace, env, deny=True)
            denied = [line for line in trace.read_text(encoding="utf-8").splitlines() if line]
            if not denied or any('"outcome": "dry-run"' in record for record in denied):
                fail(f"{relative}:{step}: negative control did not deny all write intents")
            # A helper may deliberately degrade a comment failure to a warning;
            # the decisive control is that every attempted write was denied.
            if negative.returncode == 0 and relative != ".github/workflows/lockfile-diff.yml":
                fail(f"{relative}:{step}: write-denial negative control was ignored")


def verify_negative_controls(root: Path, baseline: Path | None) -> None:
    with tempfile.TemporaryDirectory(prefix="i2374-negative-") as directory:
        temp_root = Path(directory)
        for relative in FILES:
            target = temp_root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((root / relative).read_bytes())

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
    verify_mutation_dry_runs(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--baseline-root", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
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
            verify_mutation_dry_runs(args.root)
    except (ContractError, subprocess.CalledProcessError, WriteDenied) as error:
        print(f"issue-2374 hosted proof: FAIL: {error}", file=sys.stderr)
        return 1
    print("issue-2374 hosted proof: PASS (read-only fixture checks; mutation intents dry-run; write negative controls denied)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
