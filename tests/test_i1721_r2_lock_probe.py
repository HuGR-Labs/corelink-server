from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/issue-1721-r2-lock-proof.yml"
PROBE = ROOT / "scripts/probe_i1721_r2_lock.sh"
PREFLIGHT = ROOT / "scripts/validate_i1721_r2_dispatch.sh"

FAKE_CLI = r'''
import json, os, shutil, sys, time
from pathlib import Path

kind, *args = sys.argv[1:]
root = Path(os.environ["FAKE_R2_ROOT"])
run = f'{os.environ["GITHUB_RUN_ID"]}-{os.environ["GITHUB_RUN_ATTEMPT"]}'
state = root / f"corelink/issue-1721/{run}/terraform.tfstate"
lock = Path(str(state) + ".tflock")
if kind == "aws":
    op = args[1]
    key = args[args.index("--key") + 1]
    obj = root / key
    if op == "head-object":
        if obj.exists(): sys.exit(0)
        print("Not Found", file=sys.stderr); sys.exit(1)
    if op == "get-object":
        shutil.copyfile(obj, args[-1]); sys.exit(0)
    if op == "delete-object":
        obj.unlink(missing_ok=True); sys.exit(0)
    raise SystemExit("unexpected aws operation")

chdir = next(a.split("=", 1)[1] for a in args if a.startswith("-chdir="))
tf = [a for a in args if not a.startswith("-chdir=")]
if tf[0] == "init": sys.exit(0)
if tf[:2] == ["state", "push"]:
    state.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(tf[2], state); sys.exit(0)
if tf[:2] == ["state", "pull"]:
    print(state.read_text()); sys.exit(0)
if tf[0] == "plan" and "first.tfplan" in " ".join(tf):
    lock.parent.mkdir(parents=True, exist_ok=True)
    lock.write_text(json.dumps({"ID":"LOCK-TEST-ID"}))
    time.sleep(0.4)
    if os.environ.get("FAKE_PLAN_FAIL") == "1":
        print("SENSITIVE_RAW_PLAN_SENTINEL", file=sys.stderr); lock.unlink(); sys.exit(1)
    lock.unlink(); sys.exit(0)
if tf[0] == "plan":
    if lock.exists():
        print("Error acquiring the state lock: ID LOCK-TEST-ID", file=sys.stderr); sys.exit(1)
    sys.exit(0)
raise SystemExit("unexpected terraform operation")
'''


def base_env(**updates: str) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        GITHUB_REPOSITORY="HuGR-dev/corelink-server",
        GITHUB_REF="refs/heads/main",
        REF_PROTECTED="true",
        CONFIRM="i1721-r2-lock-proof",
        GITHUB_SHA="a" * 40,
        EXPECTED_SHA="a" * 40,
    )
    env.update(updates)
    return env


class I1721R2LockProbeTests(unittest.TestCase):
    def test_hosted_contract_remains_staging_only_and_pinned(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        probe = PROBE.read_text(encoding="utf-8")
        preflight = workflow.split("  preflight:", 1)[1].split("  proof:", 1)[0]
        preflight_steps = preflight.split("    steps:\n", 1)[1]
        checkout = "Checkout exact dispatched main commit without write credentials"
        validator = "Fail closed before staging environment access"
        self.assertIn("pull_request:", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("environment: staging", workflow)
        self.assertIn("runs-on: ubuntu-24.04", workflow)
        self.assertIn("terraform_version: '1.11.4'", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", preflight)
        self.assertIn("ref: ${{ github.sha }}", preflight)
        self.assertIn("token: ''", preflight)
        self.assertIn("persist-credentials: false", preflight)
        self.assertIn("bash scripts/validate_i1721_r2_dispatch.sh", preflight)
        self.assertLess(preflight_steps.index(checkout), preflight_steps.index(validator))
        self.assertIn("'scripts/validate_i1721_r2_dispatch.sh'", workflow)
        self.assertIn("corelink-terraform-staging-state", probe)
        self.assertIn("corelink/issue-1721/$run/terraform.tfstate", probe)
        self.assertIn("use_lockfile=true", probe)
        self.assertNotIn("terraform apply", probe.lower())
        self.assertNotIn("-lock=false", probe)

    def test_dispatch_rejects_wrong_branch_and_wrong_sha(self) -> None:
        for changes in ({"GITHUB_REF": "refs/heads/feature"}, {"EXPECTED_SHA": "b" * 40}):
            with self.subTest(changes=changes):
                result = subprocess.run(["bash", str(PREFLIGHT)], env=base_env(**changes), check=False)
                self.assertNotEqual(result.returncode, 0)

    def test_staging_credentials_have_no_production_secret_fallback(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        proof = text.split("  proof:", 1)[1]
        self.assertIn("secrets.STAGING_TF_BACKEND_ACCESS_KEY_ID", proof)
        self.assertIn("secrets.STAGING_TF_BACKEND_SECRET_ACCESS_KEY", proof)
        self.assertNotIn("secrets.TF_BACKEND_ACCESS_KEY_ID", proof)
        self.assertNotIn("secrets.TF_BACKEND_SECRET_ACCESS_KEY", proof)

    def test_probe_fails_before_network_when_credentials_are_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / "network-called"
            bin_dir = Path(tmp) / "bin"
            bin_dir.mkdir()
            for command in ("aws", "terraform"):
                executable = bin_dir / command
                executable.write_text(f"#!/bin/sh\ntouch '{marker}'\n", encoding="utf-8")
                executable.chmod(0o755)
            env = {
                "PATH": f"{bin_dir}:{os.environ['PATH']}",
                "RUNNER_TEMP": tmp,
                "GITHUB_RUN_ID": "1",
                "GITHUB_RUN_ATTEMPT": "1",
                "TF_BACKEND_ENDPOINT": "https://" + "a" * 32 + ".r2.cloudflarestorage.com",
                "TF_BACKEND_BUCKET": "corelink-terraform-staging-state",
            }
            result = subprocess.run(["bash", str(PROBE)], env=env, check=False, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(marker.exists())

    def run_fake_probe(self, fail_plan: bool) -> tuple[subprocess.CompletedProcess[bytes], dict, str]:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        fake = root / "fake_cli.py"
        fake.write_text(textwrap.dedent(FAKE_CLI), encoding="utf-8")
        for command, kind in (("aws", "aws"), ("terraform", "terraform")):
            executable = bin_dir / command
            executable.write_text(f"#!/bin/sh\nexec python3 '{fake}' {kind} \"$@\"\n", encoding="utf-8")
            executable.chmod(0o755)
        run_temp = root / "runner-temp"
        run_temp.mkdir()
        env = base_env(
            PATH=f"{bin_dir}:{os.environ['PATH']}",
            RUNNER_TEMP=str(run_temp),
            GITHUB_RUN_ID="123",
            GITHUB_RUN_ATTEMPT="1",
            TF_BACKEND_ENDPOINT="https://" + "a" * 32 + ".r2.cloudflarestorage.com",
            TF_BACKEND_BUCKET="corelink-terraform-staging-state",
            AWS_ACCESS_KEY_ID="TEST_ONLY_ACCESS_KEY_SENTINEL",
            AWS_SECRET_ACCESS_KEY="TEST_ONLY_SECRET_SENTINEL",
            FAKE_R2_ROOT=str(root / "objects"),
        )
        if fail_plan:
            env["FAKE_PLAN_FAIL"] = "1"
        result = subprocess.run(["bash", str(PROBE)], env=env, check=False, capture_output=True)
        receipt_path = run_temp / "issue-1721-r2-lock-receipt.json"
        return result, json.loads(receipt_path.read_text()), root.name

    def test_successful_probe_proves_contention_release_cleanup_and_redaction(self) -> None:
        result, receipt, _ = self.run_fake_probe(fail_plan=False)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertTrue(receipt["remote_state_written"])
        self.assertTrue(receipt["remote_state_readback"])
        self.assertTrue(receipt["native_lock_observed"])
        self.assertTrue(receipt["same_lock_contention_rejected"])
        self.assertTrue(receipt["normal_exit_released_lock"])
        self.assertTrue(receipt["exact_probe_objects_cleaned"])
        self.assertNotIn("TEST_ONLY_", json.dumps(receipt))
        self.assertNotIn("SENSITIVE_", json.dumps(receipt))

    def test_false_success_stays_failed_and_still_cleans_only_run_objects(self) -> None:
        result, receipt, _ = self.run_fake_probe(fail_plan=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotEqual(receipt["exit_status"], 0)
        self.assertTrue(receipt["exact_probe_objects_cleaned"])
        self.assertNotIn("TEST_ONLY_", json.dumps(receipt))
        self.assertNotIn("SENSITIVE_RAW_PLAN_SENTINEL", json.dumps(receipt))


if __name__ == "__main__":
    unittest.main()
