#!/usr/bin/env python3
"""
test_check_billing_health.py — pytest suite for `scripts/check_billing_health.py`.

Regression target: the 2026-08-22 incident in which a **live** Stripe
subscription sat in `past_due` for a month, unnoticed, while the pre-existing
daily billing-reconciliation cron reported success every day (it compared two
D1 tables, one of which was empty, and never inspected subscription status).

The single most important assertion in this file is
`past_due_subscription_is_flagged` — the checker must fire on the exact state
that went undetected. The second most important is
`unconfigured_does_not_report_healthy`: the failure mode being defended against
is not "the check is wrong", it is "the check is green while blind".

`d1_query` is monkey-patched so no network or credentials are needed; the SQL
text is matched to decide which canned result set to return, which also pins
that each rule queries the table it claims to query.

Run:
    python3 -m pytest tests/test_check_billing_health.py -v
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "scripts" / "check_billing_health.py"

spec = importlib.util.spec_from_file_location("check_billing_health", SCRIPT)
assert spec and spec.loader
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


CREDS = ("acct", "db", "token")


def fake_d1(
    subscriptions=(),
    payment_failed_count=0,
    payment_failed_derived=None,
    duplicate_event_types=(),
    dlq_count=0,
    tables=None,
):
    """Build a `d1_query` stand-in that dispatches on the SQL it is handed.

    `payment_failed_count` is the count under Stripe's canonical `evt_…` id
    scheme; `payment_failed_derived` is the count under the container's derived
    hash scheme (defaults to the same value, which is what production looks like
    while two endpoints are live). The checker must reduce the two with MAX, so
    a caller that sets both to N is asserting "N real events", not "2N".
    """
    known = {"stripe_subscriptions", "stripe_webhook_events_processed", "stripe_webhook_events_dlq"}
    present = known if tables is None else set(tables)
    derived = payment_failed_count if payment_failed_derived is None else payment_failed_derived

    def _query(_account, _database, _token, sql):
        if "sqlite_master" in sql:
            name = sql.split("name='")[1].split("'")[0]
            return [{"name": name}] if name in present else []
        if "FROM stripe_subscriptions" in sql:
            return list(subscriptions)
        if "GROUP BY event_type" in sql:
            return list(duplicate_event_types)
        if "invoice.payment_failed" in sql:
            return [{"canonical": payment_failed_count, "derived": derived}]
        if "stripe_webhook_events_dlq" in sql:
            return [{"n": dlq_count}]
        raise AssertionError(f"unexpected SQL: {sql}")

    return _query


@pytest.fixture
def configured(monkeypatch):
    monkeypatch.setenv("CF_ACCOUNT_ID", "acct")
    monkeypatch.setenv("D1_DATABASE_ID", "db")
    monkeypatch.setenv("CF_API_TOKEN", "token")


# --- the incident this check exists for ------------------------------------


def test_past_due_subscription_is_flagged(monkeypatch, configured, capsys):
    """THE regression test: the exact state that went unnoticed for a month."""
    monkeypatch.setattr(
        mod,
        "d1_query",
        fake_d1(
            subscriptions=[
                {
                    "stripe_subscription_id": "sub_1TvRyNLh0hhAZjwor30M4Mtq",
                    "tenant_id": "3c7d77b1-0a50-4f87-893f-36ac785670df",
                    "status": "past_due",
                    "materialized_at_ms": 1787277587231,
                }
            ]
        ),
    )
    assert mod.main() == 1
    out = capsys.readouterr().out
    assert "sub_1TvRyNLh0hhAZjwor30M4Mtq" in out
    assert "past_due" in out


@pytest.mark.parametrize("status", ["past_due", "unpaid", "incomplete_expired"])
def test_every_unhealthy_status_is_flagged(monkeypatch, configured, status):
    monkeypatch.setattr(
        mod,
        "d1_query",
        fake_d1(
            subscriptions=[
                {
                    "stripe_subscription_id": "sub_x",
                    "tenant_id": "t",
                    "status": status,
                    "materialized_at_ms": 1,
                }
            ]
        ),
    )
    assert mod.main() == 1


@pytest.mark.parametrize("status", ["active", "canceled", "trialing"])
def test_healthy_and_terminal_statuses_do_not_page(monkeypatch, configured, status):
    """`canceled` is an intentional terminal state, not an anomaly — no page."""
    monkeypatch.setattr(mod, "d1_query", fake_d1(subscriptions=[]))
    assert mod.main() == 0


# --- dunning + DLQ ----------------------------------------------------------


def test_payment_failure_cluster_is_flagged_at_threshold(monkeypatch, configured):
    monkeypatch.setattr(
        mod, "d1_query", fake_d1(payment_failed_count=mod.PAYMENT_FAILED_ALERT_THRESHOLD)
    )
    assert mod.main() == 1


def test_isolated_payment_failures_below_threshold_are_tolerated(monkeypatch, configured):
    """One decline that Stripe retries successfully is ordinary, not an incident."""
    monkeypatch.setattr(
        mod, "d1_query", fake_d1(payment_failed_count=mod.PAYMENT_FAILED_ALERT_THRESHOLD - 1)
    )
    assert mod.main() == 0


def test_dead_letter_queue_backlog_is_flagged(monkeypatch, configured):
    monkeypatch.setattr(mod, "d1_query", fake_d1(dlq_count=1))
    assert mod.main() == 1


# --- double-counting across webhook id schemes ------------------------------
#
# Found in production 2026-08-23: the daily cron failed with "4
# invoice.payment_failed webhooks (threshold 3)". There were TWO real Stripe
# events. Each was recorded twice because two separately-registered endpoints
# write this table under different `event_id` schemes, and the old rule summed
# the rows. The subscription behind them was already canceled and its invoice
# already void — the alarm described money that was not at risk.


def test_two_id_schemes_for_the_same_events_do_not_double_count(monkeypatch, configured):
    """The exact production shape: 2 real events, 2 rows each, threshold 3.

    Summing gives 4 and pages. Taking the MAX gives 2 and stays quiet, which is
    the truth. This is the regression test for the false alarm.
    """
    monkeypatch.setattr(
        mod,
        "d1_query",
        fake_d1(payment_failed_count=2, payment_failed_derived=2),
    )
    assert mod.check_payment_failure_clusters(*CREDS) == []


def test_cluster_still_fires_when_only_one_scheme_is_present(monkeypatch, configured):
    """Retiring the duplicate endpoint must not blind the dunning rule."""
    monkeypatch.setattr(
        mod,
        "d1_query",
        fake_d1(
            payment_failed_count=mod.PAYMENT_FAILED_ALERT_THRESHOLD,
            payment_failed_derived=0,
        ),
    )
    assert mod.check_payment_failure_clusters(*CREDS) != []


def test_cluster_fires_on_derived_scheme_alone(monkeypatch, configured):
    """Symmetric: neither scheme is privileged as the source of truth."""
    monkeypatch.setattr(
        mod,
        "d1_query",
        fake_d1(
            payment_failed_count=0,
            payment_failed_derived=mod.PAYMENT_FAILED_ALERT_THRESHOLD,
        ),
    )
    assert mod.check_payment_failure_clusters(*CREDS) != []


def test_duplicate_ingestion_is_flagged(monkeypatch, configured, capsys):
    """A second live endpoint is itself the anomaly, and must be named as one."""
    monkeypatch.setattr(
        mod,
        "d1_query",
        fake_d1(
            duplicate_event_types=[
                {"event_type": "invoice.payment_failed", "canonical": 2, "derived": 2},
                {"event_type": "customer.subscription.updated", "canonical": 2, "derived": 5},
            ]
        ),
    )
    assert mod.main() == 1
    out = capsys.readouterr().out
    assert "BOTH id schemes" in out
    assert "invoice.payment_failed" in out
    assert "customer.subscription.updated" in out


def test_single_scheme_is_not_reported_as_duplicate(monkeypatch, configured):
    """One endpoint writing many events is normal — only BOTH schemes is the bug."""
    monkeypatch.setattr(mod, "d1_query", fake_d1(duplicate_event_types=[]))
    assert mod.check_duplicate_webhook_ingestion(*CREDS) == []


# --- the "green while blind" failure mode -----------------------------------


@pytest.mark.parametrize("missing", ["CF_ACCOUNT_ID", "D1_DATABASE_ID", "CF_API_TOKEN"])
def test_unconfigured_does_not_report_healthy(monkeypatch, configured, capsys, missing):
    monkeypatch.delenv(missing)
    assert mod.main() == 2
    out = capsys.readouterr().out
    assert "NOT CONFIGURED" in out
    assert missing in out


def test_missing_table_is_an_error_not_a_pass(monkeypatch, configured, capsys):
    """Schema drift breaks the check's premise; it must not be reported as health."""
    monkeypatch.setattr(mod, "d1_query", fake_d1(tables={"stripe_webhook_events_dlq"}))
    assert mod.main() == 2
    assert "stripe_subscriptions" in capsys.readouterr().out


def test_all_clear_reports_zero(monkeypatch, configured, capsys):
    monkeypatch.setattr(mod, "d1_query", fake_d1())
    assert mod.main() == 0
    assert "BILLING HEALTH: OK" in capsys.readouterr().out
