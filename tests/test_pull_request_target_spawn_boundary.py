"""Structural and mutation tests for the B-141 runner/permission boundary."""

from __future__ import annotations

import copy
import re
import unittest
from pathlib import Path

import yaml


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = REPO_ROOT / ".github" / "workflows"
WORKFLOW_PATHS = (*WORKFLOWS.glob("*.yml"), *WORKFLOWS.glob("*.yaml"))
TRUSTED_ASSOCIATIONS = {"OWNER", "MEMBER", "COLLABORATOR"}
UNTRUSTED_ASSOCIATIONS = {"CONTRIBUTOR", "FIRST_TIME_CONTRIBUTOR", "FIRST_TIMER", "NONE"}
ALL_PULL_REQUEST_TARGET_WORKFLOWS = {
    "dependabot-auto-merge.yml",
    "dependabot-policy.yml",
    "file-size-ratchet.yml",
    "pr-labels.yml",
    "welcome-first-pr.yml",
}
EXPECTED_JOBS = {
    "dependabot-auto-merge.yml": {"auto-merge"},
    "dependabot-policy.yml": {"sentinel", "policy-gate"},
    "file-size-ratchet.yml": {"ratchet"},
    "pr-labels.yml": {"label", "size"},
    "welcome-first-pr.yml": {"welcome"},
}
EXPECTED_RUNNERS = {
    "dependabot-auto-merge.yml": {
        "auto-merge": "corelink",
    },
    "dependabot-policy.yml": {
        "sentinel": ["self-hosted", "mac", "corelink-builder"],
        "policy-gate": "corelink",
    },
    "file-size-ratchet.yml": {
        "ratchet": "corelink",
    },
    "pr-labels.yml": {
        "label": "corelink",
        "size": "corelink",
    },
    "welcome-first-pr.yml": {
        "welcome": ["self-hosted", "mac", "corelink-builder"],
    },
}
EXPECTED_PERMISSIONS = {
    "dependabot-auto-merge.yml": {"contents": "write", "pull-requests": "write"},
    "dependabot-policy.yml": {
        "contents": "read",
        "pull-requests": "read",
        "checks": "read",
    },
    "file-size-ratchet.yml": {"contents": "read"},
    "pr-labels.yml": {
        "contents": "read",
        "pull-requests": "write",
        "issues": "write",
    },
    "welcome-first-pr.yml": {"issues": "write", "pull-requests": "write"},
}


def load_workflow(name: str) -> dict:
    loaded = yaml.safe_load((WORKFLOWS / name).read_text(encoding="utf-8"))
    assert isinstance(loaded, dict), f"{name} must be a workflow mapping"
    return loaded


def triggers_pull_request_target(workflow: dict) -> bool:
    # PyYAML 1.1 resolves YAML's unquoted `on` as True.
    triggers = workflow.get("on", workflow.get(True))
    return isinstance(triggers, dict) and "pull_request_target" in triggers


def normalized(expression: object) -> str:
    return " ".join(str(expression).split())


def assert_actor_gate(test: unittest.TestCase, job: dict, name: str) -> None:
    test.assertEqual(job.get("runs-on"), "corelink")
    test.assertIsInstance(job.get("if"), str, f"{name} needs a job-level actor gate")
    condition = normalized(job["if"])
    test.assertTrue(condition, f"{name} must gate fabric allocation before runs-on")
    for association in TRUSTED_ASSOCIATIONS:
        test.assertIn(
            f"github.event.pull_request.author_association == '{association}'",
            condition,
        )
    for association in UNTRUSTED_ASSOCIATIONS:
        test.assertNotIn(f"'{association}'", condition)
    test.assertEqual(condition.count("author_association"), len(TRUSTED_ASSOCIATIONS))


def assert_dependabot_gate(test: unittest.TestCase, job: dict, name: str) -> None:
    test.assertEqual(job.get("runs-on"), "corelink")
    test.assertIsInstance(job.get("if"), str, f"{name} needs a job-level actor gate")
    condition = normalized(job["if"])
    test.assertIn("github.actor == 'dependabot[bot]'", condition)
    test.assertIn(
        "github.event.pull_request.user.login == 'dependabot[bot]'",
        condition,
    )
    test.assertEqual(condition.count("github.actor"), 1)
    test.assertNotIn("github.actor != 'dependabot[bot]'", condition)


def assert_labels_boundary(test: unittest.TestCase, workflow: dict) -> None:
    jobs = workflow.get("jobs")
    test.assertIsInstance(jobs, dict)
    test.assertEqual(set(jobs), {"label", "size"})
    for name in ("label", "size"):
        assert_actor_gate(test, jobs[name], name)


def assert_permissions(test: unittest.TestCase, workflow: dict, name: str) -> None:
    test.assertEqual(workflow.get("permissions"), EXPECTED_PERMISSIONS[name])


def assert_runners(test: unittest.TestCase, workflow: dict, name: str) -> None:
    for job_name, expected in EXPECTED_RUNNERS[name].items():
        test.assertEqual(workflow["jobs"][job_name].get("runs-on"), expected)


def assert_file_size_boundary(test: unittest.TestCase, workflow: dict) -> None:
    jobs = workflow.get("jobs")
    test.assertIsInstance(jobs, dict)
    test.assertEqual(set(jobs), {"ratchet"})
    assert_actor_gate(test, jobs["ratchet"], "ratchet")
    test.assertIn("github.event_name != 'pull_request_target'", normalized(jobs["ratchet"]["if"]))


def assert_welcome_boundary(test: unittest.TestCase, workflow: dict) -> None:
    test.assertTrue(triggers_pull_request_target(workflow))
    triggers = workflow.get("on", workflow.get(True))
    test.assertIn("issues", triggers)
    test.assertEqual(set(workflow.get("jobs", {})), {"welcome"})
    welcome = workflow["jobs"]["welcome"]
    test.assertEqual(welcome.get("runs-on"), ["self-hosted", "mac", "corelink-builder"])
    test.assertEqual(welcome.get("timeout-minutes"), 5)
    test.assertNotIn("if", welcome)


class PullRequestTargetSpawnBoundaryTest(unittest.TestCase):
    def test_pull_request_target_population_is_closed(self) -> None:
        actual = {
            path.name
            for path in WORKFLOW_PATHS
            # A top-level trigger key is cheap to census; parse only the small
            # population it identifies so this focused gate does not spend
            # tens of seconds loading every large workflow document.
            if re.search(r"^  pull_request_target:", path.read_text(encoding="utf-8"), re.MULTILINE)
        }
        self.assertEqual(actual, ALL_PULL_REQUEST_TARGET_WORKFLOWS)

    def test_population_jobs_and_permissions_are_explicit(self) -> None:
        for name in ALL_PULL_REQUEST_TARGET_WORKFLOWS:
            workflow = load_workflow(name)
            self.assertEqual(set(workflow.get("jobs", {})), EXPECTED_JOBS[name])
            assert_permissions(self, workflow, name)
            assert_runners(self, workflow, name)

    def test_every_ephemeral_job_is_fail_closed(self) -> None:
        for name in ALL_PULL_REQUEST_TARGET_WORKFLOWS:
            workflow = load_workflow(name)
            jobs = workflow["jobs"]
            for job_name, job in jobs.items():
                if job.get("runs-on") != "corelink":
                    continue
                if name == "dependabot-auto-merge.yml" or (
                    name == "dependabot-policy.yml" and job_name == "policy-gate"
                ):
                    assert_dependabot_gate(self, job, f"{name}:{job_name}")
                else:
                    assert_actor_gate(self, job, f"{name}:{job_name}")

            # The two public-purpose lanes must have a non-fabric boundary.
            if name == "dependabot-policy.yml":
                sentinel = jobs["sentinel"]
                self.assertEqual(
                    sentinel.get("runs-on"),
                    ["self-hosted", "mac", "corelink-builder"],
                )
                self.assertEqual(sentinel.get("if"), "github.actor != 'dependabot[bot]'")
            if name == "welcome-first-pr.yml":
                welcome = jobs["welcome"]
                self.assertEqual(
                    welcome.get("runs-on"),
                    ["self-hosted", "mac", "corelink-builder"],
                )

    def test_fabric_jobs_allow_only_trusted_associations(self) -> None:
        assert_labels_boundary(self, load_workflow("pr-labels.yml"))
        assert_file_size_boundary(self, load_workflow("file-size-ratchet.yml"))

    def test_welcome_preserves_first_timer_purpose_without_fabric_spawn(self) -> None:
        assert_welcome_boundary(self, load_workflow("welcome-first-pr.yml"))

    def test_mutations_reopen_the_boundary(self) -> None:
        labels = load_workflow("pr-labels.yml")
        for name in ("label", "size"):
            with self.subTest(mutant=f"remove {name} actor gate"):
                mutant = copy.deepcopy(labels)
                del mutant["jobs"][name]["if"]
                with self.assertRaises(AssertionError):
                    assert_labels_boundary(self, mutant)

            with self.subTest(mutant=f"widen {name} actor gate"):
                mutant = copy.deepcopy(labels)
                mutant["jobs"][name]["if"] += (
                    " || github.event.pull_request.author_association == 'CONTRIBUTOR'"
                )
                with self.assertRaises(AssertionError):
                    assert_labels_boundary(self, mutant)

        file_size = load_workflow("file-size-ratchet.yml")
        for label, mutation in (
            ("remove ratchet actor gate", lambda job: job.pop("if")),
            (
                "widen ratchet actor gate",
                lambda job: job.__setitem__(
                    "if",
                    job["if"]
                    + " || github.event.pull_request.author_association == 'CONTRIBUTOR'",
                ),
            ),
        ):
            with self.subTest(mutant=label):
                mutant = copy.deepcopy(file_size)
                mutation(mutant["jobs"]["ratchet"])
                with self.assertRaises(AssertionError):
                    assert_file_size_boundary(self, mutant)

        for name, job_name in (
            ("dependabot-auto-merge.yml", "auto-merge"),
            ("dependabot-policy.yml", "policy-gate"),
        ):
            workflow = load_workflow(name)
            with self.subTest(mutant=f"remove {name}:{job_name} actor gate"):
                mutant = copy.deepcopy(workflow)
                del mutant["jobs"][job_name]["if"]
                with self.assertRaises(AssertionError):
                    assert_dependabot_gate(self, mutant["jobs"][job_name], job_name)

            with self.subTest(mutant=f"widen {name}:{job_name} actor gate"):
                mutant = copy.deepcopy(workflow)
                mutant["jobs"][job_name]["if"] += " || github.actor == 'attacker'"
                # The trusted bot identity must remain conjunctive; changing it
                # to a broad actor predicate is caught by the structural check.
                with self.assertRaises(AssertionError):
                    assert_dependabot_gate(self, mutant["jobs"][job_name], job_name)

        policy = load_workflow("dependabot-policy.yml")
        with self.subTest(mutant="move policy sentinel to fabric"):
            mutant = copy.deepcopy(policy)
            mutant["jobs"]["sentinel"]["runs-on"] = "corelink"
            with self.assertRaises(AssertionError):
                assert_runners(self, mutant, "dependabot-policy.yml")

        for name in ALL_PULL_REQUEST_TARGET_WORKFLOWS:
            with self.subTest(mutant=f"broaden {name} permissions"):
                mutant = load_workflow(name)
                # Every target lane needs pull-request metadata; changing this
                # shared scope to read is a representative permission drift.
                expected_pr = EXPECTED_PERMISSIONS[name].get("pull-requests")
                mutant["permissions"]["pull-requests"] = (
                    "read" if expected_pr == "write" else "write"
                )
                with self.assertRaises(AssertionError):
                    assert_permissions(self, mutant, name)

        welcome = load_workflow("welcome-first-pr.yml")
        for label, mutation in (
            ("move welcome to fabric", lambda job: job.__setitem__("runs-on", "corelink")),
            ("remove welcome timeout", lambda job: job.pop("timeout-minutes")),
        ):
            with self.subTest(mutant=label):
                mutant = copy.deepcopy(welcome)
                mutation(mutant["jobs"]["welcome"])
                with self.assertRaises(AssertionError):
                    assert_welcome_boundary(self, mutant)


if __name__ == "__main__":
    unittest.main()
