#!/usr/bin/env python3
"""Read every v1/v2 Stripe webhook destination page via the Stripe CLI.

This helper only issues GET requests. It fails closed when either pagination
contract is malformed or a v2 continuation URL leaves Stripe's API origin.
"""

from __future__ import annotations

import argparse
import json
import subprocess
from collections.abc import Callable, Iterable
from urllib.parse import parse_qsl, urlsplit

V1_PATH = "/v1/webhook_endpoints"
V2_PATH = "/v2/core/event_destinations"
PAGE_SIZE = "100"
MAX_PAGES = 1000
V2_WEBHOOK_URL_INCLUDE = ("include[0]", "webhook_endpoint.url")


class InventoryError(RuntimeError):
    """A paginated Stripe inventory could not be completed safely."""


PageFetcher = Callable[[str, list[tuple[str, str]]], dict]


def _data(page: dict) -> list[dict]:
    values = page.get("data")
    if not isinstance(values, list) or any(not isinstance(row, dict) for row in values):
        raise InventoryError("Stripe list page has no valid data array")
    return values


def collect_v1(fetch: PageFetcher) -> dict:
    rows: list[dict] = []
    cursor: str | None = None
    cursors: set[str] = set()
    for _ in range(MAX_PAGES):
        params = [("limit", PAGE_SIZE)]
        if cursor is not None:
            params.append(("starting_after", cursor))
        page = fetch(V1_PATH, params)
        rows.extend(_data(page))
        has_more = page.get("has_more")
        if not isinstance(has_more, bool):
            raise InventoryError("Stripe v1 list page has no boolean has_more")
        if not has_more:
            return {"data": rows}
        if not rows:
            raise InventoryError("Stripe v1 reports another page without a cursor")
        cursor = rows[-1].get("id")
        if not isinstance(cursor, str) or not cursor or cursor in cursors:
            raise InventoryError("Stripe v1 pagination cursor is missing or repeated")
        cursors.add(cursor)
    raise InventoryError("Stripe v1 pagination exceeded the page safety limit")


def _v2_next_params(next_url: str) -> list[tuple[str, str]]:
    parsed = urlsplit(next_url)
    if (
        parsed.scheme != "https"
        or parsed.netloc != "api.stripe.com"
        or parsed.path != V2_PATH
        or parsed.fragment
        or parsed.username
        or parsed.password
    ):
        raise InventoryError("Stripe v2 continuation URL is outside the expected API resource")
    params = parse_qsl(parsed.query, keep_blank_values=True)
    if not params:
        raise InventoryError("Stripe v2 continuation URL has no pagination query")
    includes = [value for key, value in params if key == V2_WEBHOOK_URL_INCLUDE[0]]
    if includes != [V2_WEBHOOK_URL_INCLUDE[1]]:
        raise InventoryError("Stripe v2 continuation URL did not preserve the webhook URL include")
    return params


def collect_v2(fetch: PageFetcher) -> dict:
    rows: list[dict] = []
    params = [("limit", PAGE_SIZE), V2_WEBHOOK_URL_INCLUDE]
    seen_pages: set[tuple[tuple[str, str], ...]] = set()
    for _ in range(MAX_PAGES):
        fingerprint = tuple(params)
        if fingerprint in seen_pages:
            raise InventoryError("Stripe v2 pagination URL repeated")
        seen_pages.add(fingerprint)
        page = fetch(V2_PATH, params)
        rows.extend(_data(page))
        next_url = page.get("next_page_url")
        if next_url is None:
            return {"data": rows}
        if not isinstance(next_url, str) or not next_url:
            raise InventoryError("Stripe v2 next_page_url is malformed")
        params = _v2_next_params(next_url)
    raise InventoryError("Stripe v2 pagination exceeded the page safety limit")


def collect_inventory(fetch: PageFetcher) -> tuple[dict, dict, bool, str | None]:
    v1 = collect_v1(fetch)
    try:
        return v1, collect_v2(fetch), True, None
    except InventoryError as exc:
        return v1, {"data": []}, False, str(exc)


def _cli_fetcher(binary: str, flags: Iterable[str]) -> PageFetcher:
    fixed_flags = list(flags)

    def fetch(path: str, params: list[tuple[str, str]]) -> dict:
        command = [binary, "get", path]
        for key, value in params:
            command.extend(("-d", f"{key}={value}"))
        command.extend(fixed_flags)
        try:
            result = subprocess.run(command, capture_output=True, text=True, check=False)
        except OSError as exc:
            raise InventoryError("Stripe CLI could not be started") from exc
        if result.returncode != 0:
            # Do not relay provider/CLI output: errors may include account details.
            raise InventoryError("Stripe CLI GET failed")
        try:
            page = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise InventoryError("Stripe CLI returned invalid JSON") from exc
        if not isinstance(page, dict):
            raise InventoryError("Stripe CLI returned a non-object list response")
        return page

    return fetch


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stripe-bin", required=True)
    parser.add_argument("--stripe-flag", action="append", default=[])
    args = parser.parse_args()
    fetch = _cli_fetcher(args.stripe_bin, args.stripe_flag)
    try:
        v1, v2, v2_ok, v2_error = collect_inventory(fetch)
    except InventoryError as exc:
        print(json.dumps({"error": str(exc)}))
        return 1
    print(json.dumps({"v1": v1, "v2": v2, "v2_sweep_ok": v2_ok, "v2_error": v2_error}, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
