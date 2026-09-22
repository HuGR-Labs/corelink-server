"""Focused mutation coverage for the #1700 provider route postflight gate."""
import unittest

from scripts import plan_staging_provider as planner


class StagingProviderRouteTests(unittest.TestCase):
    def setUp(self):
        self.cloudflare = {
            "root_worker": "corelink-staging",
            "synthetic_receiver_worker": "corelink-synthetic-pager-staging",
        }
        self.exact_routes = [
            {
                "pattern": "staging.corelink.humangr.com/*",
                "script": "corelink-staging",
            },
            {
                "pattern": "staging.corelink.humangr.com/v1/webhooks/pagerduty",
                "script": "corelink-synthetic-pager-staging",
            },
        ]

    def test_normalizes_provider_host_presentation_without_retaining_routes(self):
        routes = [
            {
                "pattern": " STAGING.CORELINK.HUMANGR.COM./* ",
                "script": "corelink-staging",
            },
            self.exact_routes[1],
            {"pattern": "api.corelink.humangr.com/*", "script": "unrelated"},
        ]
        self.assertTrue(planner.has_exact_staging_route_pairs(routes, self.cloudflare))

    def test_route_mutations_fail_closed(self):
        mutations = {
            "absent": self.exact_routes[:1],
            "extra": self.exact_routes
            + [{"pattern": "staging.corelink.humangr.com/private/*", "script": "corelink-staging"}],
            "duplicate": self.exact_routes + [self.exact_routes[0]],
            "mismapped": [
                self.exact_routes[0],
                {
                    "pattern": "staging.corelink.humangr.com/v1/webhooks/pagerduty",
                    "script": "corelink-staging",
                },
            ],
        }
        for mutation, routes in mutations.items():
            with self.subTest(mutation=mutation):
                self.assertFalse(planner.has_exact_staging_route_pairs(routes, self.cloudflare))

    def test_catchall_zone_route_that_matches_staging_fails_closed(self):
        routes = self.exact_routes + [
            {"pattern": "*.corelink.humangr.com/*", "script": "unexpected"}
        ]
        self.assertFalse(planner.has_exact_staging_route_pairs(routes, self.cloudflare))


if __name__ == "__main__":
    unittest.main()
