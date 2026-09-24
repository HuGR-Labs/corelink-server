from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from verify_i1651_gc_live_prerequisites import Blocked, _verify_observation_workflow


class StagingObservationWorkflowTest(unittest.TestCase):
    def test_image_build_reference_is_not_a_staging_observation(self) -> None:
        with self.assertRaisesRegex(Blocked, "missing dedicated staging observation workflow"):
            _verify_observation_workflow(
                {
                    "container-build-push-prod.yml": (
                        "verify /usr/local/bin/corelink-gc-sweep-production is in the image"
                    )
                }
            )

    def test_requires_manual_bounded_read_only_staging_lane(self) -> None:
        valid = """\
on:
  workflow_dispatch:
permissions:
  contents: read
jobs:
  observe:
    runs-on: ubuntu-24.04
    environment: staging
    timeout-minutes: 5
    steps:
      - name: Invoke production GC observation
        env:
          GC_OBSERVATION_ONLY: "true"
          GC_LIVE_DELETE: "false"
        run: /usr/local/bin/corelink-gc-sweep-production
"""
        _verify_observation_workflow({"issue-1651-gc-staging-observation.yml": valid})

        for mutant in (
            valid.replace("workflow_dispatch:", "workflow_dispatch:\n  push:"),
            valid.replace("environment: staging", "environment: production"),
            valid.replace('GC_LIVE_DELETE: "false"', 'GC_LIVE_DELETE: "true"'),
            valid.replace("timeout-minutes: 5", "timeout-minutes: 15"),
            valid.replace("contents: read", "contents: write"),
            valid.replace("run: /usr/local/bin/corelink-gc-sweep-production", "run: /bin/true"),
            valid.replace("    environment: staging", "    if: false\n    environment: staging"),
            valid
            + "      - name: mutate\n"
            + "        run: curl -X DELETE https://provider.example/objects\n",
            valid.replace(
                "        run: /usr/local/bin/corelink-gc-sweep-production",
                "        shell: bash -c 'source {0}; curl -X DELETE https://provider.example/objects'\n"
                "        run: /usr/local/bin/corelink-gc-sweep-production",
            ),
            valid.replace(
                "permissions:\n",
                "defaults:\n  run:\n    shell: bash -c 'source {0}; curl -X DELETE https://provider.example/objects'\n"
                "permissions:\n",
            ),
        ):
            with self.subTest(mutant=mutant):
                with self.assertRaises(Blocked):
                    _verify_observation_workflow(
                        {"issue-1651-gc-staging-observation.yml": mutant}
                    )


if __name__ == "__main__":
    unittest.main()
