#!/usr/bin/env python3
"""Regression checks for Terraform drift's Cloudflare auth contract."""

from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/terraform-drift.yml"
SECRETS_CHECKLIST = ROOT / "docs/internal/secrets-checklist.md"
ACTIVE_TERRAFORM_FILES = (
    "infra/terraform/README.md",
    "infra/terraform/main.tf",
    "infra/terraform/regions/variables.tf",
    "infra/terraform/modules/cloudflare-base/README.md",
    "infra/terraform/modules/cloudflare-base/variables.tf",
    "infra/terraform/modules/corelink-region/variables.tf",
    "infra/terraform/modules/cloudflare-storage/variables.tf",
    "infra/terraform/environments/staging/main.tf",
    "infra/terraform/environments/staging/variables.tf",
)


def workflow_step(workflow: str, step_label: str) -> str:
    """Return one named workflow step, bounded by its neighboring steps."""
    lines = workflow.splitlines()
    start = next(
        index
        for index, line in enumerate(lines)
        if line.startswith("      - name: ") and step_label in line
    )
    end = next(
        (
            index
            for index in range(start + 1, len(lines))
            if lines[index].startswith("      - name: ")
        ),
        len(lines),
    )
    return "\n".join(lines[start:end])


def step_input_envs(workflow: str, step_label: str) -> dict[str, str]:
    """Return secrets/variables mapped in one named step's env block."""
    step = workflow_step(workflow, step_label).splitlines()
    env_start = step.index("        env:") + 1

    mappings: dict[str, str] = {}
    for line in step[env_start:]:
        if not line.startswith("          "):
            if line.strip():
                break
            continue
        match = re.fullmatch(
            r"          ([A-Za-z][A-Za-z0-9_]*): \$\{\{ (secrets|vars)\.([A-Z][A-Z0-9_]*) \}\}",
            line,
        )
        if match:
            mappings[match.group(1)] = f"{match.group(2)}.{match.group(3)}"
    return mappings


class TerraformDriftAuthContractTests(unittest.TestCase):
    def test_workflow_maps_supported_repository_secrets(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        expected_cloudflare = {
            "CLOUDFLARE_API_TOKEN": "secrets.CF_TERRAFORM_DRIFT_API_TOKEN",
            "CLOUDFLARE_ACCOUNT_ID": "secrets.CF_ACCOUNT_ID",
            "TF_VAR_cf_account_id": "secrets.CF_ACCOUNT_ID",
            "TF_VAR_cf_zone_id": "vars.CF_ZONE_ID",
        }
        self.assertEqual(
            workflow.count("${{ secrets.CF_TERRAFORM_DRIFT_API_TOKEN }}"), 1
        )
        self.assertEqual(workflow.count("${{ vars.CF_ZONE_ID }}"), 1)
        self.assertNotIn("${{ secrets.CF_API_TOKEN }}", workflow)
        for name in expected_cloudflare:
            binding = f"          {name}: ${{{{"
            self.assertEqual(workflow.count(binding), 1)
        self.assertIn("provision and verify the dedicated read-only token", workflow)
        self.assertIn("cannot verify live secret existence", workflow)

        plan_env = step_input_envs(workflow, "Terraform plan")
        for name, secret in expected_cloudflare.items():
            with self.subTest(step="Terraform plan", name=name):
                self.assertEqual(plan_env.get(name), secret)

        plan_step = workflow_step(workflow, "Terraform plan")
        for required in (
            '"${CLOUDFLARE_API_TOKEN:-}"',
            '"${TF_VAR_cf_account_id:-}"',
            '"${TF_VAR_cf_zone_id:-}"',
        ):
            self.assertIn(required, plan_step)
        self.assertIn("CF_ZONE_ID repository variable", plan_step)
        self.assertIn('echo "::error::Required Cloudflare inputs missing', plan_step)
        self.assertIn("TF_EXIT=1", plan_step)
        self.assertIn('case "${TF_EXIT}" in', plan_step)
        plan_run = plan_step.partition("        run: |\n")[2]
        self.assertTrue(plan_run, "Terraform plan step must have a run script")
        script_lines = [
            line[10:]
            for line in plan_run.splitlines()
            if line.startswith("          ")
        ]
        self.assertEqual(
            script_lines[-1].strip(),
            'exit "${TF_EXIT}"',
            "Terraform plan must propagate its final exit code",
        )

        init_env = step_input_envs(workflow, "Terraform init")
        self.assertNotIn("CLOUDFLARE_API_TOKEN", init_env)
        self.assertNotIn("CLOUDFLARE_ACCOUNT_ID", init_env)
        self.assertNotIn("TF_VAR_cf_account_id", init_env)
        self.assertNotIn("TF_VAR_cf_zone_id", init_env)
        self.assertEqual(
            init_env.get("TF_BACKEND_BUCKET"), "secrets.TF_BACKEND_BUCKET"
        )
        self.assertEqual(
            init_env.get("TF_BACKEND_ENDPOINT"), "secrets.TF_BACKEND_ENDPOINT"
        )

    def test_workflow_has_no_old_client_credentials_or_oidc_permission(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertNotIn("secrets.CF_API_TOKEN", workflow)
        self.assertNotIn("CF_CLIENT_ID", workflow)
        self.assertNotIn("CF_CLIENT_SECRET", workflow)
        self.assertNotRegex(workflow, r"(?m)^\s*id-token\s*:")

    def test_workflow_stays_plan_only(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertRegex(workflow, r"(?m)^\s*terraform plan\s*\\")
        self.assertNotRegex(workflow, r"(?m)^\s*terraform apply(?:\s|$)")

    def test_active_terraform_files_do_not_claim_cloudflare_uses_github_oidc(self) -> None:
        contents = {
            path: (ROOT / path).read_text(encoding="utf-8")
            for path in ACTIVE_TERRAFORM_FILES
        }
        for path, text in contents.items():
            with self.subTest(path=path):
                self.assertNotIn("OIDC-bound credentials", text)
                self.assertNotIn("OIDC-bound only", text)
                self.assertNotIn("OIDC-bound)", text)
                self.assertNotIn("CF_API_TOKEN injected via OIDC", text)
                self.assertNotIn("CF_API_TOKEN injected via GitHub OIDC", text)
                self.assertNotRegex(
                    text,
                    re.compile(
                        r"(?:Cloudflare|CF_API_TOKEN|CF_ACCOUNT_ID|cf_account_id)"
                        r"[^\n]{0,120}OIDC|"
                        r"OIDC[^\n]{0,120}(?:Cloudflare|CF_API_TOKEN|CF_ACCOUNT_ID|cf_account_id)",
                        re.IGNORECASE,
                    ),
                )

        staging_variables = contents[
            "infra/terraform/environments/staging/variables.tf"
        ]
        self.assertIn(
            "Worker-secret TF_VAR_* inputs come from an OIDC-bound vault read",
            staging_variables,
        )
        self.assertNotIn("All TF_VAR_* sensitive inputs", staging_variables)

    def test_secrets_checklist_matches_the_drift_auth_contract(self) -> None:
        checklist_lines = SECRETS_CHECKLIST.read_text(encoding="utf-8").splitlines()

        def row(number: int) -> str:
            return next(
                line
                for line in checklist_lines
                if line.startswith(f"| {number} |")
            )

        account_id = row(49)
        self.assertIn("`CF_ACCOUNT_ID`", account_id)
        self.assertIn(".github/workflows/terraform-drift.yml", account_id)

        api_token = row(50)
        self.assertIn("`CF_API_TOKEN`", api_token)
        self.assertNotIn(".github/workflows/terraform-drift.yml", api_token)

        drift_token = row(273)
        terraform_readme = (ROOT / "infra/terraform/README.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("`CF_TERRAFORM_DRIFT_API_TOKEN`", drift_token)
        self.assertIn(".github/workflows/terraform-drift.yml", drift_token)
        self.assertIn("plan provider step only", drift_token)
        self.assertIn("read-only", drift_token.lower())
        self.assertIn("`CF_ZONE_ID` repository variable", drift_token)
        self.assertIn("`api.humangr.com`", drift_token)
        self.assertIn("account scope restricted to the one `CF_ACCOUNT_ID`", drift_token)
        self.assertIn("zone scope restricted to `CF_ZONE_ID`", drift_token)
        exact_scopes = (
            "Workers R2 Storage Read",
            "D1 Read",
            "Workers KV Storage Read",
            "Workers Scripts Read",
            "DNS Read",
            "Workers Routes Read",
        )

        def allowed_scope_set(text: str) -> set[str]:
            normalized = re.sub(r"\s+", " ", text)
            account_match = re.search(
                r"account scope restricted to the one `CF_ACCOUNT_ID` with (.*?); zone scope",
                normalized,
                flags=re.IGNORECASE,
            )
            zone_match = re.search(
                r"zone scope restricted to `CF_ZONE_ID` `?\(`api\.humangr\.com`\)`? with "
                r"(.*?)\.\s*(?:grant )?no edit/write",
                normalized,
                flags=re.IGNORECASE,
            )
            if account_match is None or zone_match is None:
                self.fail("Could not parse the account and zone allowed-scope lists")

            def parse_list(scope_list: str) -> set[str]:
                return {
                    item.strip().strip("`").strip()
                    for item in re.split(r"\s*,\s*(?:and\s+)?|\s+and\s+", scope_list)
                    if item.strip().strip("`").strip()
                }

            return parse_list(account_match.group(1)) | parse_list(zone_match.group(1))

        for source_name, source in (
            ("secrets checklist row 273", drift_token),
            ("Terraform README", terraform_readme),
        ):
            with self.subTest(source=source_name):
                self.assertEqual(
                    allowed_scope_set(source),
                    set(exact_scopes),
                    f"{source_name} must allow exactly the six read scopes",
                )
        for scope in exact_scopes:
            with self.subTest(scope=scope):
                self.assertIn(f"`{scope}`", drift_token)
                self.assertIn(f"`{scope}`", terraform_readme)
        for text in (drift_token, terraform_readme):
            normalized = re.sub(r"\s+", " ", text.lower())
            self.assertIn("no edit/write", normalized)
            self.assertIn("api tokens read", normalized)
            self.assertIn("api tokens write", normalized)
            self.assertIn("token management", normalized)
            self.assertIn("access", normalized)
            self.assertIn("pages", normalized)
            self.assertIn("user permissions", normalized)
            self.assertIn("account-scoped", normalized)
            self.assertIn("cannot be", normalized)
            self.assertIn(
                "account scope restricted to the one `cf_account_id`", normalized
            )
            self.assertIn("zone scope restricted to `cf_zone_id`", normalized)
        self.assertIn("cannot be restricted to one bucket", drift_token)
        self.assertIn("narrowed to one bucket", terraform_readme)
        self.assertIn("pre-run requirement", drift_token.lower())
        self.assertIn("CF_ZONE_ID", terraform_readme)
        self.assertIn("non-secret GitHub repository", terraform_readme)
        self.assertIn("secret/variable existence and scopes are not verified", drift_token.lower())

        for number in (51, 52, 228, 229):
            with self.subTest(row=number):
                retired_row = row(number).lower()
                self.assertIn("retired/unused after #1720", retired_row)
                self.assertIn("no active consumers", retired_row)
                self.assertIn("live secret state unchanged", retired_row)
                self.assertIn("do not delete or rotate under #1720", retired_row)


if __name__ == "__main__":
    unittest.main()
