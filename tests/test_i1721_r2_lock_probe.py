from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github/workflows/issue-1721-r2-lock-proof.yml").read_text(encoding="utf-8")
PROBE = (ROOT / "scripts/probe_i1721_r2_lock.sh").read_text(encoding="utf-8")


class I1721R2LockProbeContractTests(unittest.TestCase):
    def test_pr_contract_is_hosted_and_live_probe_is_main_only(self) -> None:
        self.assertIn("pull_request:", WORKFLOW)
        self.assertIn("workflow_dispatch:", WORKFLOW)
        self.assertIn('[[ "$GITHUB_REPOSITORY" == "HuGR-dev/corelink-server" ]]', WORKFLOW)
        self.assertIn('[[ "$GITHUB_REF" == "refs/heads/main" && "$REF_PROTECTED" == "true" ]]', WORKFLOW)
        self.assertIn('[[ "$CONFIRM" == "i1721-r2-lock-proof" ]]', WORKFLOW)
        self.assertIn('[[ "$EXPECTED_SHA" =~ ^[0-9a-f]{40}$ && "$EXPECTED_SHA" == "$GITHUB_SHA" ]]', WORKFLOW)
        self.assertIn("proof:\n    if: github.event_name == 'workflow_dispatch' && needs.preflight.result == 'success'", WORKFLOW)
        self.assertIn("environment: staging", WORKFLOW)
        self.assertIn("runs-on: ubuntu-24.04", WORKFLOW)
        self.assertNotIn("runs-on: corelink", WORKFLOW)

    def test_live_probe_isolated_read_only_and_proves_exact_lock(self) -> None:
        self.assertIn("corelink-terraform-staging-state", PROBE)
        self.assertIn("corelink/issue-1721/$run/terraform.tfstate", PROBE)
        self.assertIn("use_lockfile=true", PROBE)
        self.assertIn("terraform state push", PROBE)
        self.assertIn("terraform state pull", PROBE)
        self.assertIn("Error acquiring the state lock", PROBE)
        self.assertIn('grep -Fq "$lock_id"', PROBE)
        self.assertIn('trap finish EXIT', PROBE)
        self.assertIn('object_absent "$state_key"', PROBE)
        self.assertIn("exact_probe_objects_cleaned", PROBE)
        self.assertIn("if-no-files-found: error", WORKFLOW)
        self.assertNotIn("terraform apply", PROBE.lower())
        self.assertNotIn("-lock=false", PROBE)
        self.assertNotIn("AWS_ACCESS_KEY_ID", PROBE[PROBE.index("python3 - \"$receipt\""):])


if __name__ == "__main__":
    unittest.main()
