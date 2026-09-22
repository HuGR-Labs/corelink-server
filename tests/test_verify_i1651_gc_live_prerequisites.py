from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from verify_i1651_gc_live_prerequisites import Blocked, _verify_observation_workflow


class StagingObservationWorkflowTest(unittest.TestCase):
    def test_image_build_reference_does_not_prove_staging_observation(self) -> None:
        workflows = {
            "container-build-push-prod.yml": (
                "verify image contains /usr/local/bin/corelink-gc-sweep-production"
            )
        }

        with self.assertRaisesRegex(Blocked, "missing staging observation workflow"):
            _verify_observation_workflow(workflows)

    def test_dedicated_staging_workflow_requires_read_only_runtime_fence(self) -> None:
        workflow = (
            'on:\n  workflow_dispatch:\n'
            'permissions:\n  contents: read\n'
            'jobs:\n  observe:\n'
            '    runs-on: ubuntu-24.04\n'
            '    environment: staging\n'
            '    timeout-minutes: 5\n'
            '    steps:\n'
            '      - name: Invoke production GC observation\n'
            '        env:\n'
            '          GC_OBSERVATION_ONLY: "true"\n'
            '          GC_LIVE_DELETE: "false"\n'
            '        run: /usr/local/bin/corelink-gc-sweep-production\n'
        )

        _verify_observation_workflow({"issue-1651-gc-staging-observation.yml": workflow})

    def test_comment_decoys_and_missing_runtime_controls_are_rejected(self) -> None:
        valid = (
            'on:\n  workflow_dispatch:\n'
            'permissions:\n  contents: read\n'
            'jobs:\n  observe:\n'
            '    runs-on: ubuntu-24.04\n'
            '    environment: staging\n'
            '    timeout-minutes: 5\n'
            '    steps:\n'
            '      - name: Invoke production GC observation\n'
            '        env:\n'
            '          GC_OBSERVATION_ONLY: "true"\n'
            '          GC_LIVE_DELETE: "false"\n'
            '        run: /usr/local/bin/corelink-gc-sweep-production\n'
        )
        for mutant in (
            valid.replace("runs-on: ubuntu-24.04", "# runs-on: ubuntu-24.04"),
            valid.replace("environment: staging", "# environment: staging"),
            valid.replace('GC_LIVE_DELETE: "false"', '# GC_LIVE_DELETE: "false"'),
            valid.replace("timeout-minutes: 5", "# timeout-minutes: 5"),
            valid.replace("workflow_dispatch:", "workflow_dispatch:\n  push:"),
            valid.replace("    environment: staging", "    if: false\n    environment: staging"),
            valid.replace(
                '          GC_LIVE_DELETE: "false"',
                '          GC_LIVE_DELETE: "false"\n          GC_LIVE_DELETE: "true"',
            ),
            valid.replace(
                "        run: /usr/local/bin/corelink-gc-sweep-production",
                "        run: /bin/true\n        run: /usr/local/bin/corelink-gc-sweep-production",
            ),
        ):
            with self.subTest(mutant=mutant):
                with self.assertRaises(Blocked):
                    _verify_observation_workflow({"issue-1651-gc-staging-observation.yml": mutant})


if __name__ == "__main__":
    unittest.main()
