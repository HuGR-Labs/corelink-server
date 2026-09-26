"""Credentialless quarantine-baseline tests for the #1700 staging plan."""

from __future__ import annotations

import unittest

from scripts import plan_staging_provider as plan


class StagingPlanTests(unittest.TestCase):
    def test_any_exact_canonical_dns_record_blocks_quarantined_bootstrap(self) -> None:
        self.assertFalse(plan.canonical_dns_present([]))
        self.assertFalse(plan.canonical_dns_present([{
            "name": "other.humangr.com",
            "proxied": True,
            "type": "A",
        }]))
        self.assertTrue(plan.canonical_dns_present([{
            "name": plan.HOSTNAME,
            "proxied": False,
            "type": "TXT",
        }]))

    def test_any_canonical_route_blocks_quarantined_bootstrap(self) -> None:
        self.assertFalse(plan.canonical_route_present([]))
        self.assertFalse(plan.canonical_route_present([{
            "pattern": "other.humangr.com/*",
            "script": "other-worker",
        }]))
        self.assertTrue(plan.canonical_route_present([{
            "pattern": f"{plan.HOSTNAME}/v1/webhooks/pagerduty",
            "script": "unexpected-worker",
        }]))


if __name__ == "__main__":
    unittest.main()
