import json
import tomllib
import unittest
from pathlib import Path

from scripts import render_staging_wrangler as renderer


ROOT = Path(__file__).resolve().parents[1]
ENDPOINT = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com"


class StagingWranglerRendererTests(unittest.TestCase):
    def setUp(self):
        self.topology = json.loads(renderer.TOPOLOGY.read_text(encoding="utf-8"))

    def test_renders_three_isolated_configs_with_all_topology_bindings(self):
        renderer.validate_topology(self.topology, ENDPOINT)
        cf = self.topology["cloudflare"]
        output_path = ROOT / "infra/staging/.rendered/root.wrangler.toml"
        configs = {
            worker: tomllib.loads(renderer.render_worker(
                self.topology, worker, ENDPOINT, "final", output_path
            ))
            for worker in self.topology["outputs"]["worker_names"]
        }

        self.assertEqual(set(configs), {
            "corelink-staging",
            "corelink-signup-staging",
            "corelink-synthetic-pager-staging",
        })
        for worker, config in configs.items():
            self.assertEqual(config["name"], worker)
            self.assertFalse(config["workers_dev"])
            self.assertNotIn("env", config)
            self.assertNotIn("prod", json.dumps(config).lower())

        root = configs[cf["root_worker"]]
        self.assertEqual(root["vars"]["R2_S3_ENDPOINT"], ENDPOINT)
        self.assertEqual(
            {binding["binding"] for binding in root["r2_buckets"]},
            {binding["binding"] for binding in cf["r2"]},
        )
        self.assertEqual(
            {binding["binding"] for binding in root["kv_namespaces"]},
            {binding["binding"] for binding in cf["kv"]},
        )
        self.assertEqual(
            {binding["name"] for binding in root["durable_objects"]["bindings"]},
            {binding["binding"] for binding in cf["durable_objects"]},
        )
        self.assertEqual(len(root["routes"]), 1)
        self.assertEqual(len(configs[cf["synthetic_receiver_worker"]]["routes"]), 1)
        self.assertEqual(
            {binding["binding"] for binding in configs[cf["signup_worker"]]["d1_databases"]},
            {"CONFIG_DB", "BILLING_DB"},
        )
        signup = configs[cf["signup_worker"]]
        self.assertEqual(signup["queues"]["producers"][0]["queue"], "corelink-dsr-erasure-staging")
        self.assertEqual(
            {consumer["queue"] for consumer in signup["queues"]["consumers"]},
            {"corelink-dsr-erasure-staging", "corelink-dsr-erasure-dlq-staging"},
        )
        self.assertEqual(
            {binding["service"] for binding in signup["services"]},
            {cf["root_worker"]},
        )
        self.assertEqual(
            {binding["service"] for binding in root["services"]},
            {cf["synthetic_receiver_worker"]},
        )

    def test_rejects_missing_or_production_provider_endpoint(self):
        for endpoint in (
            "",
            "https://api.r2.cloudflarestorage.com",
            "https://production.r2.cloudflarestorage.com",
            "http://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com",
        ):
            with self.subTest(endpoint=endpoint), self.assertRaises(renderer.ContractError):
                renderer.validate_topology(self.topology, endpoint)

    def test_rejects_production_worker_target(self):
        mutated = json.loads(json.dumps(self.topology))
        mutated["cloudflare"]["signup_worker_settings"]["vars"]["CORELINK_API_BASE"] = (
            "https://api.corelink.production.humangr.com"
        )
        with self.assertRaises(renderer.ContractError):
            renderer.validate_topology(mutated, ENDPOINT)

    def test_renders_no_secret_values_or_secret_bindings(self):
        for worker in self.topology["outputs"]["worker_names"]:
            rendered = renderer.render_worker(
                self.topology,
                worker,
                ENDPOINT,
                "final",
                ROOT / "infra/staging/.rendered/root.wrangler.toml",
            )
            self.assertNotIn("[env.staging]", rendered)
            self.assertNotIn("secret", rendered.lower())
            for names in self.topology["required_secret_names"].values():
                for name in names:
                    self.assertNotIn(name, rendered)

    def test_bootstrap_configs_have_no_routes(self):
        for worker in self.topology["outputs"]["worker_names"]:
            config = tomllib.loads(renderer.render_worker(
                self.topology,
                worker,
                ENDPOINT,
                "bootstrap",
                ROOT / "infra/staging/.rendered/bootstrap.wrangler.toml",
            ))
            self.assertNotIn("routes", config)


if __name__ == "__main__":
    unittest.main()
