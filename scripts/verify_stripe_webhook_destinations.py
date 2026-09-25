#!/usr/bin/env python3
"""Read-only, fail-closed inventory of Stripe webhook destinations.

Stripe exposes snapshot destinations at ``/v1/webhook_endpoints`` and thin/event
destinations at ``/v2/core/event_destinations``. They are separate collections
with different pagination contracts, so a v1-only read cannot prove that a v2
destination is absent.

Only ``stripe get`` is invoked. Both collections are walked to the end, records
are normalized, IDs are masked in output, and malformed/incomplete pages fail
closed. No signing secret or event payload is printed. This is inventory only;
delivery history and owner authorization remain B-065 evidence gates.

Exit codes: 0 complete inventory and required URLs present; 1 duplicate or
missing required destination; 2 unavailable/incomplete inventory or bad input.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from typing import Any, Sequence
from urllib.parse import parse_qs, urlparse, urlsplit

PAGE_SIZE = 100
COMMAND_TIMEOUT_SECONDS = 45
ACTIVE_STATUSES = frozenset({"active", "enabled"})


class VerificationError(RuntimeError):
    """The verifier could not establish a complete, trustworthy readback."""


@dataclass(frozen=True)
class Destination:
    """Normalized subset of a v1 or v2 destination safe for evidence output."""

    api: str
    id: str
    url: str | None
    status: str
    event_types: tuple[str, ...]
    event_payload: str | None
    created: Any
    updated: Any
    name: str | None

    @property
    def active(self) -> bool:
        """Whether Stripe reports this destination as able to receive events."""
        return self.status.lower() in ACTIVE_STATUSES


def mask_id(value: str) -> str:
    """Keep an ID recognizable without copying the full account inventory."""
    return value[:3] + "…" if len(value) <= 10 else f"{value[:6]}…{value[-4:]}"


def text(value: Any, field: str, *, nullable: bool = False) -> str | None:
    if value is None and nullable:
        return None
    if not isinstance(value, str) or not value:
        raise VerificationError(f"destination field {field!r} is missing or invalid")
    return value


def normalize_destination(raw: dict[str, Any], api: str) -> Destination:
    """Normalize one API resource and reject ambiguous rows."""
    if api not in {"v1", "v2"}:
        raise VerificationError(f"unsupported destination API: {api}")
    identifier = text(raw.get("id"), "id")
    prefix = "we_" if api == "v1" else "ed_"
    if not identifier.startswith(prefix):
        raise VerificationError(f"{api} destination has unexpected id scheme")
    status = text(raw.get("status"), "status")
    events = raw.get("enabled_events")
    if not isinstance(events, list) or not all(isinstance(e, str) and e for e in events):
        raise VerificationError(f"{api} destination has invalid enabled_events")
    if api == "v1":
        url, payload = text(raw.get("url"), "url"), "snapshot"
    else:
        if text(raw.get("type"), "type") == "webhook_endpoint":
            endpoint = raw.get("webhook_endpoint")
            if not isinstance(endpoint, dict):
                raise VerificationError("v2 webhook destination has no webhook_endpoint object")
            url = text(endpoint.get("url"), "webhook_endpoint.url", nullable=True)
        else:
            url = None
        payload = text(raw.get("event_payload"), "event_payload")
    return Destination(
        api, identifier, url, status, tuple(sorted(events)), payload,
        raw.get("created"), raw.get("updated"), text(raw.get("name"), "name", nullable=True),
    )


def stripe_get(binary: str, path: str, flags: Sequence[str]) -> dict[str, Any]:
    """Run exactly one read-only Stripe CLI GET and parse its data page."""
    try:
        result = subprocess.run(
            [binary, "get", path, *flags], check=False, capture_output=True,
            text=True, timeout=COMMAND_TIMEOUT_SECONDS,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise VerificationError(f"Stripe {path} read failed") from error
    if result.returncode != 0:
        raise VerificationError(f"Stripe {path} read failed (exit {result.returncode})")
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise VerificationError(f"Stripe {path} returned invalid JSON") from error
    if not isinstance(payload, dict) or not isinstance(payload.get("data"), list):
        raise VerificationError(f"Stripe {path} returned no complete data page")
    return payload


def list_v1(binary: str, flags: Sequence[str]) -> list[Destination]:
    """Read every v1 endpoint using its starting-after cursor."""
    rows: list[Destination] = []
    cursor: str | None = None
    seen: set[str] = set()
    while True:
        page_flags = [*flags, "-d", f"limit={PAGE_SIZE}"]
        if cursor:
            page_flags += ["-d", f"starting_after={cursor}"]
        payload = stripe_get(binary, "/v1/webhook_endpoints", page_flags)
        page = payload["data"]
        for raw in page:
            if not isinstance(raw, dict):
                raise VerificationError("v1 destination page contains a non-object")
            rows.append(normalize_destination(raw, "v1"))
        has_more = payload.get("has_more")
        if not isinstance(has_more, bool):
            raise VerificationError("v1 page has no boolean has_more")
        if not has_more:
            return rows
        if not page or not isinstance(page[-1], dict):
            raise VerificationError("v1 page says has_more without a final row")
        cursor = page[-1].get("id")
        if not isinstance(cursor, str) or not cursor or cursor in seen:
            raise VerificationError("v1 pagination cursor is missing or repeated")
        seen.add(cursor)


def list_v2(binary: str, flags: Sequence[str]) -> list[Destination]:
    """Read every v2 destination using its next-page URL token."""
    rows: list[Destination] = []
    page_token: str | None = None
    seen: set[str] = set()
    while True:
        page_flags = [*flags, "-d", f"limit={PAGE_SIZE}", "-d", "include[0]=webhook_endpoint.url"]
        if page_token:
            page_flags += ["-d", f"page={page_token}"]
        payload = stripe_get(binary, "/v2/core/event_destinations", page_flags)
        for raw in payload["data"]:
            if not isinstance(raw, dict):
                raise VerificationError("v2 destination page contains a non-object")
            rows.append(normalize_destination(raw, "v2"))
        next_url = payload.get("next_page_url")
        if next_url in (None, ""):
            return rows
        if not isinstance(next_url, str):
            raise VerificationError("v2 next_page_url is invalid")
        parsed = urlsplit(next_url)
        if (
            parsed.scheme != "https"
            or parsed.netloc != "api.stripe.com"
            or parsed.path != "/v2/core/event_destinations"
            or parsed.fragment
            or parsed.username
            or parsed.password
        ):
            raise VerificationError("v2 continuation URL is outside the expected API resource")
        query = parse_qs(parsed.query, keep_blank_values=True)
        if query.get("include[0]") != ["webhook_endpoint.url"]:
            raise VerificationError("v2 continuation URL did not preserve the webhook URL include")
        token = query.get("page", [])
        if len(token) != 1 or not token[0] or token[0] in seen:
            raise VerificationError("v2 pagination token is missing or repeated")
        seen.add(token[0])
        page_token = token[0]


def findings(rows: Sequence[Destination], required_urls: Sequence[str]) -> list[str]:
    """Find active duplicate URLs and missing required active destinations."""
    active: dict[str, list[Destination]] = {}
    for row in rows:
        if row.active and row.url:
            active.setdefault(row.url, []).append(row)
    result = [
        f"{len(items)} active destinations share {url} ({', '.join(mask_id(r.id) for r in items)})"
        for url, items in sorted(active.items()) if len(items) > 1
    ]
    result += [
        f"required active destination missing: {url}"
        for url in required_urls if not any(r.url == url and r.active for r in rows)
    ]
    return result


def print_inventory(rows: Sequence[Destination]) -> None:
    """Print redacted, payload-free evidence lines."""
    if not rows:
        print("DESTINATION INVENTORY: empty")
        return
    for row in rows:
        url = row.url or "<non-webhook destination>"
        if row.url:
            parsed = urlparse(row.url)
            url = parsed._replace(query="", fragment="").geturl()
        print(
            f"[{row.api}] id={mask_id(row.id)} status={row.status} "
            f"types={','.join(row.event_types)} payload={row.event_payload or 'snapshot'} "
            f"url={url} created={row.created!s} updated={row.updated!s}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stripe-bin", default=os.environ.get("STRIPE_BIN", "stripe"))
    parser.add_argument("--live", action="store_true", help="pass live-mode intent to Stripe CLI")
    parser.add_argument("--account", help="pin the Stripe account through the CLI")
    parser.add_argument("--require-url", action="append", default=[],
                        help="required active URL; repeat for each preserved destination")
    args = parser.parse_args(sys.argv[1:] if argv is None else argv)
    if not args.require_url:
        print("NOT CONFIGURED — at least one --require-url is required; inventory alone is not closure evidence.")
        return 2
    flags: list[str] = []
    if args.live and not os.environ.get("STRIPE_API_KEY", "").startswith(("rk_live_", "sk_live_")):
        flags.append("--live")
    if args.account:
        flags += ["--account", args.account]
    try:
        rows = [*list_v1(args.stripe_bin, flags), *list_v2(args.stripe_bin, flags)]
    except VerificationError as error:
        print(f"NOT CONFIGURED — destination inventory is UNKNOWN: {error}")
        print("  Both paginated Stripe collections are required; no clean result is inferred.")
        return 2
    print_inventory(rows)
    print("  v1 pages: complete")
    print("  v2 pages: complete")
    print("  delivery history: not queried (inventory only)")
    problems = findings(rows, args.require_url)
    if problems:
        print(f"DESTINATION INVENTORY: {len(problems)} finding(s)")
        for problem in problems:
            print(f"  - {problem}")
        return 1
    print("DESTINATION INVENTORY: complete; required active destinations present")
    return 0


if __name__ == "__main__":
    sys.exit(main())
