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
            'on:\n  workflow_dispatch:\njobs:\n  observe:\n'
            '    environment: staging\n    steps:\n'
            '      - run: /usr/local/bin/corelink-gc-sweep-production\n'
            '        env:\n          GC_OBSERVATION_ONLY: "true"\n'
            '          GC_LIVE_DELETE: "false"\n'
        )

        _verify_observation_workflow({"issue-1651-gc-staging-observation.yml": workflow})


if __name__ == "__main__":
    unittest.main()
