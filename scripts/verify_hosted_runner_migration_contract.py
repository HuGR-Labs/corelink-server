#!/usr/bin/env python3
"""Static, closed-world admission contract for shared hosted-runner bundles.

This verifier reads workflow YAML as inert text.  It never imports candidate
code or starts a build, deployment, provider operation, release, publication,
load test, or other mutation.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path


class ContractError(RuntimeError):
    pass


HOSTED = {"ubuntu-24.04", "macos-15", "macos-15-intel"}
INVENTORIES = {
    "bundle-2367": {
        ".github/workflows/corelink-auth-pat.yml": {"pr-gate"},
        ".github/workflows/corelink-client-verify.yml": {"pr-gate", "standalone-build", "wasm-build", "cbindgen-header-stable", "fuzz-smoke", "fuzz-nightly", "mutants-nightly"},
        ".github/workflows/corelink-hash.yml": {"pr-gate", "wasm-build", "fuzz-smoke", "fuzz-nightly", "mutants-nightly"},
        ".github/workflows/corelink-meta.yml": {"pr-gate", "wasm-build", "fuzz-smoke", "fuzz-nightly", "mutants-nightly"},
        ".github/workflows/corelink-reapi.yml": {"pr-gate", "wasm-build", "mutants-nightly"},
        ".github/workflows/corelink-worker.yml": {"pr-gate", "wasm-build", "fuzz-smoke", "fuzz-nightly", "mutants-nightly"},
        ".github/workflows/ffi-matrix-ci.yml": {"rust-unit", "python-matrix", "go-matrix", "js-matrix", "cross-language-verify"},
    },
    "bundle-2368": {
        ".github/workflows/mutation-nightly.yml": {"mutants", "aggregate"},
        ".github/workflows/mutation-pr.yml": {"mutants-diff"},
        ".github/workflows/nightly.yml": {"tlc-extended", "proptest-extended", "fuzz-matrix", "mutants-workspace"},
        ".github/workflows/proptest-density-gate.yml": {"density-gate"},
        ".github/workflows/region_pinning.yml": {"migration-check", "clippy-and-existing-tests", "proptest-30k", "adversarial-tests", "rb-region-leak-dry-run", "workflow-yaml-smoke", "region-pinning-gate", "proptest-100k-nightly"},
        ".github/workflows/tenant-path.yml": {"pr-gate", "fuzz-smoke", "fuzz-nightly", "mutants-nightly"},
        ".github/workflows/workspace-lint.yml": {"clippy-workspace"},
    },
    "bundle-2369": {
        ".github/workflows/bazel-starter-ci.yml": {"bazel-cache-hit", "negative-scenarios", "weekly-benchmark"},
        ".github/workflows/cas-canary.yml": {"fabric", "hosted"},
        ".github/workflows/fabric-soak-proof.yml": {"soak"},
        ".github/workflows/issue-1670-fleet-classification.yml": {"classify-bounded-disk-failures"},
        ".github/workflows/load-test-nightly.yml": {"k6-staging", "baseline-regression"},
        ".github/workflows/perf-nightly.yml": {"bench", "rotate-baseline"},
        ".github/workflows/perf-production-evidence.yml": {"collect"},
        ".github/workflows/perf-regression.yml": {"perf-regression"},
        ".github/workflows/proof-runs-on-corelink.yml": {"remote-cache-hit"},
        ".github/workflows/runner-fleet-health.yml": {"fleet"},
        ".github/workflows/smoke-install.yml": {"observe"},
    },
    "bundle-2370": {
        ".github/workflows/cargo-audit.yml": {"cargo-audit-pr", "cargo-audit-daily"},
        ".github/workflows/cas_foundation.yml": {"workspace-build-test", "cargo-deny", "sbom-cyclonedx", "cosign-sign", "reproducible-build-smoke"},
        ".github/workflows/container-build-push-prod.yml": {"build-push"},
        ".github/workflows/gitleaks.yml": {"gitleaks"},
        ".github/workflows/license-policy.yml": {"license-policy"},
        ".github/workflows/manual-install-recipes.yml": {"linux-x86_64-tarball-recipe"},
        ".github/workflows/pnpm-audit.yml": {"pnpm-audit"},
        ".github/workflows/release-notes.yml": {"generate"},
        ".github/workflows/reproducible-build.yml": {"two-leg-diff"},
        ".github/workflows/sbom-consolidated.yml": {"sbom-consolidated"},
        ".github/workflows/sbom.yml": {"sbom-generate", "sbom-ntia-validate", "sbom-tsa-attest", "sbom-dt-ingest", "sbom-release-upload"},
    },
    "bundle-2371": {
        ".github/workflows/admin-ui-deploy.yml": {"build-and-deploy"},
        ".github/workflows/cf-deploy-prod.yml": {"gate-secrets-checklist", "gate-cf-secrets-populated", "deploy", "migrate-d1"},
        ".github/workflows/docs-deploy.yml": {"build-and-deploy"},
        ".github/workflows/signup-worker-deploy.yml": {"deploy"},
    },
    "bundle-2372": {
        ".github/workflows/api-reference-sync.yml": {"drift-check", "regenerate-and-pr"},
        ".github/workflows/openapi-validate.yml": {"lint-and-validate", "breaking-change-detection"},
        ".github/workflows/subprocessors-sync.yml": {"drift-check", "regenerate-and-pr"},
    },
    "bundle-2373": {
        ".github/workflows/admin-ui-ci.yml": {"ci"},
        ".github/workflows/admin-ui-e2e.yml": {"legacy-e2e", "critical-e2e"},
        ".github/workflows/e2e-browser-prod.yml": {"browser"},
        ".github/workflows/e2e-clerk-signup.yml": {"gate", "e2e"},
        ".github/workflows/e2e-prod.yml": {"playwright"},
        ".github/workflows/e2e-stripe-checkout.yml": {"gate", "e2e"},
    },
    "bundle-2374": {
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
    },
    "bundle-2375": {
        ".github/workflows/ac-bucket-acl-cron.yml": {"audit"},
        ".github/workflows/audit-archive-lag.yml": {"archive-lag"},
        ".github/workflows/audit-chain-daily-verify.yml": {"smoke-verify", "seven-day-verify"},
        ".github/workflows/b044-orphan-teardown.yml": {"b044-contract"},
        ".github/workflows/compliance-weekly.yml": {"digest"},
        ".github/workflows/dpa-legal-review.yml": {"flag-legal-review"},
        ".github/workflows/dr-drill-monthly.yml": {"dr-drill"},
        ".github/workflows/legal-changes-review.yml": {"flag-legal-review"},
        ".github/workflows/pentest-findings-sync.yml": {"validate-findings", "notify"},
        ".github/workflows/production-deployability.yml": {"verify"},
        ".github/workflows/tls-floor-drift.yml": {"tls-floor"},
    },
    "bundle-2376": {
        ".github/workflows/backup-daily-verify.yml": {"verify"},
        ".github/workflows/backup-daily.yml": {"backup"},
        ".github/workflows/billing-aggregate-runner.yml": {"aggregate"},
        ".github/workflows/billing-reconcile-daily.yml": {"reconcile"},
        ".github/workflows/byok_kill_switch_drill_weekly.yml": {"kill-switch-drill"},
        ".github/workflows/byok_matrix_weekly.yml": {"byok-boundary-static", "byok-matrix-16", "byok-proptest-100k", "byok-adversarial", "byok-fips-status-check"},
        ".github/workflows/neon-shadow-reconcile-daily.yml": {"reconcile"},
    },
}
SHA = re.compile(r"[0-9a-f]{40}")
JOB = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
RUNNER = re.compile(r"^    runs-on:\s*(.*?)\s*(?:#.*)?$")


def fail(message: str) -> None:
    raise ContractError(message)


def job_blocks(text: str, path: str) -> dict[str, list[str]]:
    lines = text.splitlines()
    try:
        start = next(index for index, line in enumerate(lines) if line == "jobs:") + 1
    except StopIteration:
        fail(f"{path}: missing jobs mapping")
    jobs: dict[str, list[str]] = {}
    current: str | None = None
    for line in lines[start:]:
        match = JOB.match(line)
        if match:
            current = match.group(1)
            if current in jobs:
                fail(f"{path}: duplicate job {current}")
            jobs[current] = []
        elif current is not None:
            jobs[current].append(line)
    if not jobs:
        fail(f"{path}: no jobs found")
    return jobs


def require_credentialless_checkout(lines: list[str], path: str, job: str) -> None:
    for index, line in enumerate(lines):
        if "uses: actions/checkout@" not in line:
            continue
        indent = len(line) - len(line.lstrip())
        step_indent = indent - 2
        end = len(lines)
        for cursor in range(index + 1, len(lines)):
            candidate = lines[cursor]
            if candidate.startswith(" " * step_indent + "- "):
                end = cursor
                break
        step = "\n".join(lines[index:end])
        if not re.search(r"(?m)^\s+persist-credentials:\s*false\s*(?:#.*)?$", step):
            fail(f"{path}:{job}: checkout must set persist-credentials: false")


def validate(root: Path, inventory: str) -> None:
    try:
        files = INVENTORIES[inventory]
    except KeyError as error:
        fail(f"unknown inventory {inventory!r}")
        raise AssertionError from error
    for relative, expected_jobs in files.items():
        path = root / relative
        try:
            jobs = job_blocks(path.read_text(encoding="utf-8"), relative)
        except OSError as error:
            fail(f"{relative}: unreadable: {error}")
        if jobs.keys() != expected_jobs:
            fail(
                f"{relative}: closed job inventory drifted; "
                f"expected {sorted(expected_jobs)}, got {sorted(jobs)}"
            )
        for job, lines in jobs.items():
            runner_lines = [match.group(1).strip() for line in lines if (match := RUNNER.match(line))]
            if len(runner_lines) != 1:
                fail(f"{relative}:{job}: expected exactly one runs-on selector")
            runner = runner_lines[0]
            if runner not in HOSTED:
                fail(f"{relative}:{job}: disallowed runner {runner!r}")
            require_credentialless_checkout(lines, relative, job)


def assert_exact_head(root: Path, expected_head: str) -> None:
    if SHA.fullmatch(expected_head) is None:
        fail("expected head must be a full lowercase SHA")
    actual = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"], check=True, capture_output=True, text=True
    ).stdout.strip()
    if actual != expected_head:
        fail(f"candidate checkout is {actual}, expected {expected_head}")


def fixture(root: Path, inventory: str) -> None:
    for relative, jobs in INVENTORIES[inventory].items():
        body = ["name: fixture", "on: pull_request", "permissions:", "  contents: read", "jobs:"]
        for job in sorted(jobs):
            body.extend((f"  {job}:", "    runs-on: ubuntu-24.04", "    steps:",
                         "      - uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
                         "        with:", "          persist-credentials: false"))
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("\n".join(body) + "\n", encoding="utf-8")


def expect_rejected(root: Path, inventory: str, old: str, new: str) -> None:
    target = root / next(iter(INVENTORIES[inventory]))
    original = target.read_text(encoding="utf-8")
    if old not in original:
        fail(f"fixture lost mutation anchor {old!r}")
    target.write_text(original.replace(old, new, 1), encoding="utf-8")
    try:
        validate(root, inventory)
    except ContractError:
        pass
    else:
        fail(f"mutation escaped hosted runner contract: {old!r}")
    target.write_text(original, encoding="utf-8")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for inventory in INVENTORIES:
            fixture(root, inventory)
            validate(root, inventory)
            expect_rejected(root, inventory, "runs-on: ubuntu-24.04", "runs-on: corelink")
            expect_rejected(root, inventory, "runs-on: ubuntu-24.04", "runs-on: self-hosted")
            expect_rejected(root, inventory, "persist-credentials: false", "persist-credentials: true")
            target = root / next(iter(INVENTORIES[inventory]))
            original = target.read_text(encoding="utf-8")
            target.write_text(
                original + "  uncontracted-job:\n    runs-on: ubuntu-24.04\n    steps: []\n",
                encoding="utf-8",
            )
            try:
                validate(root, inventory)
            except ContractError:
                pass
            else:
                fail("uncontracted hosted job escaped the closed inventory")
            target.write_text(original, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--inventory", choices=tuple(INVENTORIES))
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            if args.root or args.inventory or args.expected_head:
                fail("--self-test cannot be combined with candidate arguments")
            self_test()
        elif args.root and args.inventory and args.expected_head:
            assert_exact_head(args.root, args.expected_head)
            validate(args.root, args.inventory)
        else:
            fail("pass --self-test or --root, --inventory, and --expected-head")
    except (ContractError, subprocess.CalledProcessError) as error:
        print(f"hosted-runner contract: FAIL: {error}", file=sys.stderr)
        return 1
    print("hosted-runner contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
