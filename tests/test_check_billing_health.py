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


def fake_d1(subscriptions=(), payment_failed_count=0, dlq_count=0, tables=None):
    """Build a `d1_query` stand-in that dispatches on the SQL it is handed."""
    known = {"stripe_subscriptions", "stripe_webhook_events_processed", "stripe_webhook_events_dlq"}
    present = known if tables is None else set(tables)

    def _query(_account, _database, _token, sql):
        if "sqlite_master" in sql:
            name = sql.split("name='")[1].split("'")[0]
            return [{"name": name}] if name in present else []
        if "FROM stripe_subscriptions" in sql:
            return list(subscriptions)
        if "invoice.payment_failed" in sql:
            return [{"n": payment_failed_count}]
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
