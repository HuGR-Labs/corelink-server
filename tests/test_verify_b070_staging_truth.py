from __future__ import annotations

import importlib.util
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b070_staging_truth.py"
spec = importlib.util.spec_from_file_location("b070_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B070StagingTruthTests(unittest.TestCase):
    def copy_fixture(self) -> tempfile.TemporaryDirectory[str]:
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        paths = {
            verifier.ROOT_WORKER_CONFIG,
            verifier.PROD_WORKFLOW,
            verifier.BACKLOG,
            verifier.DEPLOYMENT_DOC,
            verifier.CHANGELOG,
            verifier.ANALYTICS_WORKER_CONFIG,
            verifier.ANALYTICS_WORKER_PACKAGE,
            *(path for path, _ in verifier.CLAIM_SURFACES.values()),
        }
        for relative in paths:
            source = ROOT / relative
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        return temp

    def run_cli(self, root: Path, expect: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(root), "--expect", expect],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_repaired_tree_is_done(self) -> None:
        self.assertEqual(verifier.assess(ROOT), [])
        result = self.run_cli(ROOT, "done")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)

    def test_original_and_renamed_staging_tables_reopen(self) -> None:
        for table in ("staging", "stage-canary"):
            with self.subTest(table=table):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / verifier.ROOT_WORKER_CONFIG
                    path.write_text(
                        path.read_text(encoding="utf-8")
                        + f'\n[env.{table}]\nname = "corelink-{table}"\n',
                        encoding="utf-8",
                    )
                    gaps = verifier.assess(Path(temp.name))
                    self.assertTrue(
                        any(gap.startswith("staging-like-env:") for gap in gaps), gaps
                    )

    def test_staging_workflow_wiring_reopens(self) -> None:
        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.PROD_WORKFLOW
            path.write_text(
                path.read_text(encoding="utf-8")
                + "\n      - name: mutated staging deploy\n        run: wrangler deploy --env staging\n",
                encoding="utf-8",
            )
            gaps = verifier.assess(Path(temp.name))
            self.assertIn("staging-workflow-deploy", gaps)

    def test_production_matrix_and_env_are_not_vacuous(self) -> None:
        temp = self.copy_fixture()
        with temp:
            workflow = Path(temp.name) / verifier.PROD_WORKFLOW
            workflow.write_text(
                workflow.read_text(encoding="utf-8").replace(
                    ',"prod-syd"]', "]", 1
                ),
                encoding="utf-8",
            )
            gaps = verifier.assess(Path(temp.name))
            self.assertIn("production-workflow-matrix", gaps)

        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.ROOT_WORKER_CONFIG
            text = path.read_text(encoding="utf-8")
            mutated = re.sub(
                r"(?ms)^\[env\.prod-syd\].*?(?=^\[env\.prod-(?:sam|lhr|nrt)\]|\Z)",
                "",
                text,
                count=1,
            )
            self.assertNotEqual(mutated, text)
            path.write_text(mutated, encoding="utf-8")
            gaps = verifier.assess(Path(temp.name))
            self.assertIn("production-env-missing:prod-syd", gaps)

    def test_production_topology_fingerprint_catches_each_binding_family(self) -> None:
        mutations = {
            "routes": ("corelink-api.humangr.com/*", "mutated.corelink-api.humangr.com/*"),
            "r2_buckets": ("corelink-cas-prod", "mutated-cas-prod"),
            "kv_namespaces": ("56f8e99f36ad4ec2aa3b876562409ccc", "00000000000000000000000000000000"),
            "d1_databases": ("d64742ea-e102-40b2-a844-ff02e3f94562", "00000000-0000-0000-0000-000000000000"),
            "durable_objects": ("class_name = \"RolloutController\"", "class_name = \"MutatedController\""),
            "containers": (
                f"corelink-prod-corelinkserver-prod:{verifier.EXPECTED_PRODUCTION_CONTAINER_TAG}",
                "corelink-prod-corelinkserver-prod:mutated",
            ),
        }
        for family, (old, new) in mutations.items():
            with self.subTest(family=family):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / verifier.ROOT_WORKER_CONFIG
                    text = path.read_text(encoding="utf-8")
                    start = text.index("[env.prod]\n")
                    end = text.index("[env.prod-sam]\n", start)
                    prod_block = text[start:end]
                    self.assertIn(old, text)
                    self.assertIn(old, prod_block)
                    if family == "d1_databases":
                        old_line = f'database_id = "{old}"'
                        new_line = f'database_id = "{new}"'
                        self.assertIn(old_line, prod_block)
                        mutated_block = prod_block.replace(old_line, new_line, 1)
                    else:
                        mutated_block = prod_block.replace(old, new, 1)
                    path.write_text(
                        text[:start] + mutated_block + text[end:],
                        encoding="utf-8",
                    )
                    self.assertIn(
                        f"production-topology:prod:{family}",
                        verifier.assess(Path(temp.name)),
                    )

    def test_production_container_pin_is_independent_and_fails_closed(self) -> None:
        current = verifier.EXPECTED_PRODUCTION_CONTAINER_TAG
        mutations = {
            "fleet-wide-stale": (current, "b90b245df-r1", 5),
            "single-region-mismatch": (
                f"corelink-prod-syd-corelinkserver-prod:{current}",
                "corelink-prod-syd-corelinkserver-prod:deadbeef0-r1",
                1,
            ),
            "malformed-tag": (
                f"corelink-prod-corelinkserver-prod:{current}",
                "corelink-prod-corelinkserver-prod:latest",
                1,
            ),
        }
        for reason, (old, new, expected_gaps) in mutations.items():
            with self.subTest(reason=reason):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / verifier.ROOT_WORKER_CONFIG
                    text = path.read_text(encoding="utf-8")
                    self.assertEqual(text.count(old), expected_gaps)
                    path.write_text(text.replace(old, new), encoding="utf-8")
                    gaps = verifier.assess(Path(temp.name))
                    container_gaps = [
                        gap
                        for gap in gaps
                        if gap.startswith("production-topology:")
                        and gap.endswith(":containers")
                    ]
                    self.assertEqual(len(container_gaps), expected_gaps, gaps)

    def test_runbook_r2_inventory_matches_all_prod_bindings_and_population(self) -> None:
        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / "specs/_runbooks/RB-GA-CUTOVER.md"
            text = path.read_text(encoding="utf-8")
            mutated = text.replace("corelink-ac-eu", "corelink-ac-lhr", 1)
            self.assertNotEqual(mutated, text)
            path.write_text(mutated, encoding="utf-8")
            gaps = verifier.assess(Path(temp.name))
            self.assertIn("runbook-r2-inventory-mismatch", gaps)
            self.assertIn("runbook-r2-inventory-duplicate", gaps)

        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.ROOT_WORKER_CONFIG
            text = path.read_text(encoding="utf-8")
            start = text.index("[env.prod-lhr]\n")
            end = text.index("[env.prod-nrt]\n", start)
            block = text[start:end]
            mutated_block = block.replace(
                'bucket_name = "corelink-ac-eu"',
                'bucket_name = "corelink-ac-lhr"',
                1,
            )
            self.assertNotEqual(mutated_block, block)
            path.write_text(text[:start] + mutated_block + text[end:], encoding="utf-8")
            gaps = verifier.assess(Path(temp.name))
            self.assertIn("production-r2-population:18", gaps)
            self.assertIn("runbook-r2-inventory-mismatch", gaps)

        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / "specs/_runbooks/RB-GA-CUTOVER.md"
            text = path.read_text(encoding="utf-8")
            marker = "  corelink-manifest-nrt corelink-manifest-syd\n"
            self.assertIn(marker, text)
            path.write_text(
                text.replace(marker, marker + "  corelink-r2-extra\n", 1),
                encoding="utf-8",
            )
            gaps = verifier.assess(Path(temp.name))
            self.assertIn("runbook-r2-population:20", gaps)
            self.assertIn("runbook-r2-extra:corelink-r2-extra", gaps)

    def test_workflow_matrix_is_connected_to_deploy_invocation(self) -> None:
        mutations = {
            "workflow-matrix-output": (
                'matrix={"env":["prod","prod-sam","prod-lhr","prod-nrt","prod-syd"]}',
                'matrix={"env":["prod"]}',
            ),
            "workflow-matrix-strategy": (
                "matrix: ${{ fromJson(needs.gate-secrets-checklist.outputs.matrix) }}",
                "matrix: ${{ fromJson(needs.other.outputs.matrix) }}",
            ),
            "workflow-matrix-env": (
                "WRANGLER_ENV: ${{ matrix.env }}",
                "WRANGLER_ENV: prod",
            ),
            "workflow-env-deploy": (
                'bash scripts/deploy-container-prod.sh --apply --env "${WRANGLER_ENV}"',
                'bash scripts/deploy-container-prod.sh --apply --env prod',
            ),
        }
        for reason, (old, new) in mutations.items():
            with self.subTest(reason=reason):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / verifier.PROD_WORKFLOW
                    text = path.read_text(encoding="utf-8")
                    if reason in {"workflow-matrix-env", "workflow-env-deploy"}:
                        start = text.index("\n  deploy:\n")
                        prefix, deploy = text[:start], text[start:]
                        self.assertIn(old, deploy)
                        text = prefix + deploy.replace(old, new, 1)
                    elif reason == "workflow-matrix-strategy":
                        start = text.index("\n  deploy:\n")
                        prefix, deploy = text[:start], text[start:]
                        self.assertIn(old, deploy)
                        text = prefix + deploy.replace(old, new, 1)
                    else:
                        self.assertIn(old, text)
                        text = text.replace(old, new, 1)
                    path.write_text(text, encoding="utf-8")
                    self.assertIn(reason, verifier.assess(Path(temp.name)))

    def test_root_staging_runner_mode_fails_closed(self) -> None:
        result = subprocess.run(
            ["bash", str(ROOT / "scripts/d1-migration-runner.sh"), "--env", "staging", "--dry-run"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("one of dev|prod", result.stderr)

    def test_implicit_staging_runbook_command_reopens(self) -> None:
        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / "specs/_runbooks/RB-D1-MIGRATION-APPLY.md"
            text = path.read_text(encoding="utf-8")
            path.write_text(text + "\nwrangler d1 export corelink_d1_wnam --env staging\n", encoding="utf-8")
            self.assertIn("d1-runbook-staged-rollout-claim", verifier.assess(Path(temp.name)))

    def test_ga_cutover_retired_staging_ranges_and_invalid_env_reopen(self) -> None:
        mutations = (
            "for purpose in audit ac main staging-1 staging-2; do",
            "against staging for full 24h",
            "dedup ratio within ±5% of staging baseline",
            "wrangler deploy --gradual=1 --env=production",
        )
        for addition in mutations:
            with self.subTest(addition=addition):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / "specs/_runbooks/RB-GA-CUTOVER.md"
                    path.write_text(
                        path.read_text(encoding="utf-8") + "\n" + addition + "\n",
                        encoding="utf-8",
                    )
                    self.assertIn(
                        "ga-cutover-live-staging-claim",
                        verifier.assess(Path(temp.name)),
                    )

    def test_ga_cutover_production_procedure_cannot_be_deleted(self) -> None:
        for marker in verifier.GA_CUTOVER_REQUIRED_MARKERS:
            with self.subTest(marker=marker):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / "specs/_runbooks/RB-GA-CUTOVER.md"
                    text = path.read_text(encoding="utf-8")
                    self.assertIn(marker, text)
                    path.write_text(text.replace(marker, "", 1), encoding="utf-8")
                    self.assertIn(
                        f"ga-cutover-required-marker:{marker}",
                        verifier.assess(Path(temp.name)),
                    )

    def test_unrelated_analytics_worker_staging_sentinel_reopens(self) -> None:
        mutations = {
            "unrelated-worker-staging-name": (
                verifier.ANALYTICS_WORKER_CONFIG,
                'name = "corelink-analytics-staging"',
                'name = "corelink-analytics-mutated"',
            ),
            "unrelated-worker-staging-environment": (
                verifier.ANALYTICS_WORKER_CONFIG,
                'ENVIRONMENT = "staging"',
                'ENVIRONMENT = "prod"',
            ),
            "unrelated-worker-staging-d1": (
                verifier.ANALYTICS_WORKER_CONFIG,
                '[[env.staging.d1_databases]]\nbinding = "ANALYTICS_DB"',
                '[[env.staging.d1_databases]]\nbinding = "MUTATED_DB"',
            ),
            "unrelated-worker-staging-observability": (
                verifier.ANALYTICS_WORKER_CONFIG,
                "[env.staging.observability]\nenabled = true\nhead_sampling_rate = 1",
                "[env.staging.observability]\nenabled = false\nhead_sampling_rate = 1",
            ),
            "unrelated-worker-staging-trigger": (
                verifier.ANALYTICS_WORKER_CONFIG,
                '[env.staging.triggers]\ncrons = [\n    "0 8 * * 1"\n]',
                '[env.staging.triggers]\ncrons = [\n    "0 9 * * 1"\n]',
            ),
            "unrelated-worker-staging-deploy": (
                verifier.ANALYTICS_WORKER_PACKAGE,
                '"deploy:staging": "wrangler deploy --env staging"',
                '"deploy:staging": "wrangler deploy --env prod"',
            ),
            "unrelated-worker-staging-schema": (
                verifier.ANALYTICS_WORKER_PACKAGE,
                '"schema:apply:staging": "wrangler d1 execute corelink-analytics-staging --file src/schema.sql --remote"',
                '"schema:apply:staging": "wrangler d1 execute corelink-analytics-prod --file src/schema.sql --remote"',
            ),
        }
        for reason, (relative, old, new) in mutations.items():
            with self.subTest(reason=reason):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / relative
                    text = path.read_text(encoding="utf-8")
                    self.assertIn(old, text)
                    path.write_text(text.replace(old, new, 1), encoding="utf-8")
                    self.assertIn(reason, verifier.assess(Path(temp.name)))

    def test_each_named_claim_family_reopens(self) -> None:
        mutations = {
            "wrangler-staging-mirror-claim": "# 1:1 mirror of prod\n",
            "wrangler-staging-comment-claim": "# ALL envs include [env.staging]\n",
            "d1-runner-staged-rollout-claim": "# canonical entrypoint for staged D1 schema rollouts\n",
            "d1-runner-root-staging-mode": "# --env staging\n",
            "residency-promotion-claim": "# gates promotion to production\n",
            "d1-runbook-staged-rollout-claim": "## 3. Apply procedure (dev → staging → prod canary)\n",
            "ga-cutover-live-staging-claim": "### 2.1 Deploy to staging\n",
        }
        for reason, addition in mutations.items():
            with self.subTest(reason=reason):
                temp = self.copy_fixture()
                with temp:
                    relative, _ = verifier.CLAIM_SURFACES[reason]
                    path = Path(temp.name) / relative
                    path.write_text(
                        path.read_text(encoding="utf-8") + "\n" + addition,
                        encoding="utf-8",
                    )
                    self.assertIn(reason, verifier.assess(Path(temp.name)))

    def test_docs_and_narrative_cannot_be_marked_done_without_guard_evidence(self) -> None:
        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.DEPLOYMENT_DOC
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "no staging leg", "a staging leg", 1
                ),
                encoding="utf-8",
            )
            self.assertTrue(
                any(gap.startswith("deployment-doc:") for gap in verifier.assess(Path(temp.name)))
            )

        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.CHANGELOG
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "no automatic staging-to-production promotion", "promotion", 1
                ),
                encoding="utf-8",
            )
            self.assertTrue(
                any(gap.startswith("changelog:") for gap in verifier.assess(Path(temp.name)))
            )

    def test_backlog_status_and_verifier_wiring_are_required(self) -> None:
        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.BACKLOG
            text = path.read_text(encoding="utf-8")
            start = text.index("### B-070")
            before, item = text[:start], text[start:]
            item = item.replace("status: done", "status: open", 1)
            path.write_text(before + item, encoding="utf-8")
            self.assertIn("backlog-b070-not-done", verifier.assess(Path(temp.name)))

        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.BACKLOG
            text = path.read_text(encoding="utf-8")
            self.assertIn("verify_b070_staging_truth.py --expect done", text)
            path.write_text(
                text.replace("verify_b070_staging_truth.py --expect done", "echo done", 1),
                encoding="utf-8",
            )
            self.assertIn("backlog-b070-verifier-wiring", verifier.assess(Path(temp.name)))

    def test_missing_input_is_an_instrument_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(Path(directory))


if __name__ == "__main__":
    unittest.main()
