#!/usr/bin/env python3
"""Static and mutation checks for the regional Terraform drift contract."""
from __future__ import annotations

import os
import re
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/terraform-drift.yml"
PLAN_SCRIPT = ROOT / "scripts/terraform_drift_plan.sh"
REGIONS = ("wnam", "enam", "weur", "sam")


class TerraformDriftStateContractTests(unittest.TestCase):
    def test_each_region_is_an_independent_root_with_native_locking(self) -> None:
        for region in REGIONS:
            root = ROOT / "infra/terraform/regions" / region
            self.assertTrue(root.is_dir(), region)
            terraform = (root / "backend.tf").read_text(encoding="utf-8")
            main = (root / "main.tf").read_text(encoding="utf-8")
            self.assertIn('backend "s3"', terraform)
            self.assertIn("use_lockfile                = true", terraform)
            self.assertRegex(terraform, r"skip_s3_checksum\s*=\s*true")
            self.assertIn(f'key                         = "corelink/{region}/terraform.tfstate"', terraform)
            self.assertNotRegex(terraform, r"\b(access_key|secret_key)\s*=")

            modules = re.findall(r'^module "([^"]+)"\s*\{', main, flags=re.MULTILINE)
            self.assertEqual(modules, [region])
            self.assertNotIn('module "', "\n".join(
                path.read_text(encoding="utf-8")
                for path in root.glob("*.tf")
                if path.name != "main.tf"
            ))

    def test_workflow_uses_dynamic_regional_root_and_fails_closed(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn('run: scripts/terraform_drift_plan.sh "${{ matrix.region }}" "infra/terraform/regions/${{ matrix.region }}"', workflow)
        self.assertIn('terraform -chdir="${root}" init', workflow)
        self.assertIn('-backend-config="use_lockfile=true"', workflow)
        self.assertIn('-backend-config="endpoints={s3=\\"${TF_BACKEND_ENDPOINT}\\"}"', workflow)
        self.assertNotIn("cat >", workflow)
        self.assertNotIn("creating stub", workflow.lower())
        self.assertNotIn("-lock=false", workflow)
        self.assertNotRegex(workflow, r"(?m)^\s*terraform apply(?:\s|$)")
        self.assertIn("AWS_ACCESS_KEY_ID: ${{ secrets.TF_BACKEND_ACCESS_KEY_ID }}", workflow)
        self.assertIn("AWS_SECRET_ACCESS_KEY: ${{ secrets.TF_BACKEND_SECRET_ACCESS_KEY }}", workflow)
        self.assertNotIn("-backend-config=\"access_key=", workflow)
        self.assertNotIn("-backend-config=\"secret_key=", workflow)
        self.assertIn("Required R2 backend inputs missing before init", workflow)

    def test_actions_are_sha_pinned(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for line in workflow.splitlines():
            if "uses: " in line:
                self.assertRegex(line, r"uses: [^@\s]+@[0-9a-f]{40}")

    def _run_plan(self, terraform_exit: int, *, tee_exit: int = 0, missing: str | None = None):
        with tempfile.TemporaryDirectory() as tmp:
            temp = Path(tmp)
            bin_dir = temp / "bin"
            bin_dir.mkdir()
            called = temp / "called"
            fake_tf = bin_dir / "terraform"
            fake_tf.write_text(
                f"#!/bin/sh\ntouch {called}\nexit {terraform_exit}\n",
                encoding="utf-8",
            )
            fake_tf.chmod(fake_tf.stat().st_mode | stat.S_IXUSR)
            if tee_exit:
                fake_tee = bin_dir / "tee"
                fake_tee.write_text(
                    f"#!/bin/sh\ncat\nexit {tee_exit}\n", encoding="utf-8"
                )
                fake_tee.chmod(fake_tee.stat().st_mode | stat.S_IXUSR)
            root = temp / "root"
            root.mkdir()
            output = temp / "github-output"
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{bin_dir}:{env['PATH']}",
                    "GITHUB_OUTPUT": str(output),
                    "TF_BACKEND_BUCKET": "state-bucket",
                    "TF_BACKEND_ENDPOINT": "https://account.r2.cloudflarestorage.com",
                    "AWS_ACCESS_KEY_ID": "r2-key-id",
                    "AWS_SECRET_ACCESS_KEY": "r2-secret-value",
                    "CLOUDFLARE_API_TOKEN": "cf-provider-token",
                    "TF_VAR_cf_account_id": "account-id",
                    "TF_VAR_cf_zone_id": "zone-id",
                }
            )
            if missing:
                env.pop(missing, None)
            result = subprocess.run(
                [str(PLAN_SCRIPT), "wnam", str(root)],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )
            contents = output.read_text(encoding="utf-8") if output.exists() else ""
            return result, contents, called.exists()

    def test_plan_preserves_detailed_exit_codes(self) -> None:
        for expected in (0, 1, 2):
            with self.subTest(terraform_exit=expected):
                result, output, called = self._run_plan(expected)
                self.assertEqual(result.returncode, expected)
                self.assertTrue(called)
                self.assertIn(f"exitcode={expected}\n", output)
                self.assertIn(f"terraform_exitcode={expected}\n", output)
                self.assertNotIn("r2-secret-value", result.stdout + result.stderr + output)

    def test_plan_maps_unexpected_exit_without_echoing_raw_output(self) -> None:
        result, output, called = self._run_plan(7)
        self.assertEqual(result.returncode, 1)
        self.assertTrue(called)
        self.assertIn("exitcode=1\n", output)
        self.assertIn("terraform_exitcode=7\n", output)
        self.assertEqual(result.stdout, "")

    def test_missing_backend_credential_stops_before_plan(self) -> None:
        result, output, called = self._run_plan(0, missing="AWS_SECRET_ACCESS_KEY")
        self.assertEqual(result.returncode, 1)
        self.assertFalse(called)
        self.assertIn("exitcode=1\n", output)
        self.assertIn("AWS_SECRET_ACCESS_KEY", result.stderr)
        self.assertNotIn("r2-secret-value", result.stdout + result.stderr + output)


if __name__ == "__main__":
    unittest.main()
