"""Offline tests for complete, fail-closed Stripe destination pagination."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from urllib.parse import urlencode

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts" / "ops"))

from stripe_webhook_inventory import InventoryError, collect_v1, collect_v2  # noqa: E402


class StripeWebhookInventoryTests(unittest.TestCase):
    def test_v1_collects_all_pages_using_last_object_cursor(self) -> None:
        calls: list[tuple[str, list[tuple[str, str]]]] = []

        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            calls.append((path, params))
            if len(calls) == 1:
                return {"data": [{"id": "we_first"}, {"id": "we_cursor"}], "has_more": True}
            return {"data": [{"id": "we_last"}], "has_more": False}

        result = collect_v1(fetch)

        self.assertEqual([row["id"] for row in result["data"]], ["we_first", "we_cursor", "we_last"])
        self.assertEqual(calls, [
            ("/v1/webhook_endpoints", [("limit", "100")]),
            ("/v1/webhook_endpoints", [("limit", "100"), ("starting_after", "we_cursor")]),
        ])

    def test_v2_follows_the_next_page_url_query(self) -> None:
        cursor = "opaque_cursor_2"
        next_url = "https://api.stripe.com/v2/core/event_destinations?" + urlencode(
            [
                ("limit", "100"),
                ("include[0]", "webhook_endpoint.url"),
                ("starting_after", cursor),
            ]
        )
        calls: list[tuple[str, list[tuple[str, str]]]] = []

        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            calls.append((path, params))
            if len(calls) == 1:
                return {"data": [{"id": "ed_first"}], "next_page_url": next_url}
            return {"data": [{"id": "ed_last"}], "next_page_url": None}

        result = collect_v2(fetch)

        self.assertEqual([row["id"] for row in result["data"]], ["ed_first", "ed_last"])
        self.assertEqual(calls, [
            (
                "/v2/core/event_destinations",
                [("limit", "100"), ("include[0]", "webhook_endpoint.url")],
            ),
            (
                "/v2/core/event_destinations",
                [
                    ("limit", "100"),
                    ("include[0]", "webhook_endpoint.url"),
                    ("starting_after", cursor),
                ],
            ),
        ])

    def test_v2_rejects_a_continuation_that_drops_the_webhook_url_include(self) -> None:
        next_url = "https://api.stripe.com/v2/core/event_destinations?starting_after=ed_next"

        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            return {"data": [{"id": "ed_first"}], "next_page_url": next_url}

        with self.assertRaisesRegex(InventoryError, "did not preserve the webhook URL include"):
            collect_v2(fetch)

    def test_v2_rejects_continuation_urls_outside_stripe_api(self) -> None:
        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            return {
                "data": [{"id": "ed_first"}],
                "next_page_url": "https://attacker.invalid/v2/core/event_destinations?page=x",
            }

        with self.assertRaisesRegex(InventoryError, "outside the expected API resource"):
            collect_v2(fetch)

    def test_v1_fails_closed_when_more_pages_have_no_cursor(self) -> None:
        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            return {"data": [], "has_more": True}

        with self.assertRaisesRegex(InventoryError, "without a cursor"):
            collect_v1(fetch)

    def test_v1_fails_closed_when_cursor_repeats(self) -> None:
        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            return {"data": [{"id": "we_same"}], "has_more": True}

        with self.assertRaisesRegex(InventoryError, "missing or repeated"):
            collect_v1(fetch)

    def test_v2_fails_closed_when_continuation_repeats(self) -> None:
        repeated = "https://api.stripe.com/v2/core/event_destinations?include%5B0%5D=webhook_endpoint.url&starting_after=ed_same"
        calls = 0

        def fetch(path: str, params: list[tuple[str, str]]) -> dict:
            nonlocal calls
            calls += 1
            return {"data": [{"id": "ed_same"}], "next_page_url": repeated}

        with self.assertRaisesRegex(InventoryError, "pagination URL repeated"):
            collect_v2(fetch)
        self.assertEqual(calls, 2)

if __name__ == "__main__":
    unittest.main()
