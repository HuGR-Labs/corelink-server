#!/usr/bin/env python3
"""
CoreLink R-prep: cross-region replication lag verifier.

Daily-runnable. Queries replica heartbeat from each region (or fakes
via mocked InMemory replica when staging endpoints aren't reachable),
reports lag per data domain, and exits non-zero if any domain exceeds
its RPO budget.

Wire-in:
  - Cron: daily 02:00 UTC alongside `scripts/backup-daily.sh`.
  - Output: stdout summary + Prometheus pushgateway emit
    `corelink_replication_verifier_status{result, domain}`.
  - Exit codes:
      0 = all domains within RPO budget
      1 = one or more domains exceeded RPO budget (SEV ladder applies)
      2 = config / network error (treat as inconclusive — do NOT page)
  - SEV ladder mirrors SLO-BACKUP-VERIFICATION (slo_catalog.md §4.22):
      1 fail              → SEV-3
      2 consecutive fails → SEV-2
      3 consecutive fails → SEV-1

Anchors:
  - specs/_audits/2026-05-15-replication-audit.md §7
  - specs/03_architecture/slo_catalog.md §4.23..4.26
  - crates/corelink-replica-worker (REPLICATION_LAG_P99_SLO_SECS = 60)

Modes:
  --mode=staging     query staging endpoints (default in CI cron)
  --mode=inmemory    deterministic in-memory fixture (default for local
                     dev + GitHub Actions PR gates — no network)
  --mode=prod        query production endpoints (manual; requires
                     CORELINK_PROD_TOKEN env)

Usage:
    python3 scripts/verify-replication-lag.py --help
    python3 scripts/verify-replication-lag.py --mode=inmemory
    python3 scripts/verify-replication-lag.py --mode=inmemory --json
    python3 scripts/verify-replication-lag.py --mode=staging --domain=r2
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field, asdict
from typing import Callable, Iterable

# ---------------------------------------------------------------------------
# Constants — RPO budgets per domain (seconds).
# Sourced from specs/03_architecture/slo_catalog.md §4.18..4.26 +
# specs/_audits/2026-05-15-replication-audit.md §3.
# ---------------------------------------------------------------------------

# Cross-region replication-lag SLO budgets (p99 ceilings, continuous SLI).
# These are TARGET budgets the daily verifier enforces; the canonical SLO
# definitions live in slo_catalog.md §4.23..4.26.
RPO_BUDGET_SECS: dict[str, int] = {
    # R2 hot blobs replicated by corelink-replica-worker cron.
    # REPLICATION_LAG_P99_SLO_SECS = 60 per
    # crates/corelink-replica-worker/src/region.rs L180.
    "r2_hot": 60,
    # R2 cold + AC + audit via Cloudflare R2 platform CRR.
    # SLO-BACKUP-VERIFICATION §4.22 row "R2 (RPO 24h)".
    "r2_crr": 24 * 3600,
    # D1 read-replica lag — live ceiling at DR-16 declaration trigger
    # (specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md §1.3 — replication
    # lag within RPO 5 min). We tighten to 60 s p99 for continuous SLI.
    "d1": 60,
    # KV global eventual — "typical ≤ 60 s" per Cloudflare KV docs.
    # specs/03_architecture/storage_semantics_matrix.md §4.3.
    "kv": 60,
    # DO state — sync-to-D1 age for TenantQuota / ConfigSingleton.
    # RateLimiter is excluded (intentional reset on failover, see audit
    # §3.5 edge case (a)).
    "do": 300,
    # Neon read-replica — soft SLO, informational (post-GA polish).
    "neon": 5,
}

# SLO definition IDs in slo_catalog.md (for traceability in the JSON output).
SLO_IDS: dict[str, str] = {
    "r2_hot": "SLO-REPLICATION-LAG-R2",
    "r2_crr": "SLO-REPLICATION-LAG-R2",  # extension; see ticket P1-004
    "d1": "SLO-REPLICATION-LAG-D1",
    "kv": "SLO-REPLICATION-LAG-KV",
    "do": "SLO-REPLICATION-LAG-DO",
    "neon": "SLO-REPLICATION-LAG-NEON (soft)",
}

# Canonical Prometheus metric names per domain.
# LOAD-BEARING: these must match the Rust constants in
#   - crates/corelink-replica-worker/src/metrics.rs::METRIC_REPLICATION_LAG_SECONDS
#   - crates/corelink-region/src/replica_lag.rs::METRIC_D1_REPLICA_LAG_SECONDS
# Renaming on either side requires updating both.
PROM_METRIC: dict[str, str] = {
    "r2_hot": "corelink_replication_lag_seconds",
    "r2_crr": "corelink_r2_crr_lag_seconds",          # P1-004 (probe indirect)
    "d1": "corelink_d1_replica_lag_seconds",
    "kv": "corelink_kv_propagation_lag_seconds",      # P1-001
    "do": "corelink_do_sync_age_seconds",             # P1-003
    "neon": "corelink_neon_replica_lag_seconds",      # P2-001 (soft)
}

# Domain-keyed Prometheus label filter (LOAD-BEARING for the histogram_quantile
# query). r2_hot uses `domain="r2_hot"` because it shares the histogram name
# across multiple domains; other domains use the metric name alone.
PROM_LABELS: dict[str, str] = {
    "r2_hot": 'domain="r2_hot"',
    "r2_crr": "",
    "d1": "",
    "kv": "",
    "do": "",
    "neon": "",
}

# Canonical 4-region GA list.
# Mirrors crates/corelink-replica-worker::Region::ALL.
REGIONS = ("wnam", "enam", "weur", "sam")

# Acyclic residency pairs (write-pinned sibling).
# Mirrors crates/corelink-replica-worker::ResidencyGraph.
SIBLING: dict[str, str] = {
    "wnam": "enam",
    "enam": "wnam",
    "weur": "sam",
    "sam": "weur",
}


# ---------------------------------------------------------------------------
# Data structures.
# ---------------------------------------------------------------------------

@dataclass
class DomainResult:
    """Per-(domain, region) lag measurement + verdict."""

    domain: str
    primary_region: str
    replica_region: str
    measured_lag_seconds: float
    rpo_budget_seconds: int
    slo_id: str
    within_budget: bool
    note: str = ""


@dataclass
class VerifierReport:
    """Aggregate report for one verifier run."""

    mode: str
    timestamp_ms: int
    results: list[DomainResult] = field(default_factory=list)
    exit_code: int = 0
    error: str = ""

    def add(self, r: DomainResult) -> None:
        self.results.append(r)

    @property
    def fails(self) -> list[DomainResult]:
        return [r for r in self.results if not r.within_budget]

    def finalize(self) -> None:
        self.exit_code = 1 if self.fails else 0


# ---------------------------------------------------------------------------
# Probes — one per domain. Each returns measured lag in seconds.
#
# `mode == "inmemory"` produces deterministic fixture lags (under budget
# everywhere) so the script can run in CI without network. The InMemory
# values match the InMemoryReplicationWorker semantics in
# crates/corelink-replica-worker/src/replication.rs (lag = 0 for freshly
# replicated synthetic blobs).
#
# `mode == "staging"` / `"prod"` would query Prometheus
# (corelink_replication_lag_seconds histogram p99 over the last 1h) and
# extract the value per domain/region pair. We stub those paths with a
# clear NotImplementedError until the staging metrics endpoint is wired
# (R-PREP-REPL-P0-001 in replication-followup-tickets.md).
# ---------------------------------------------------------------------------

Probe = Callable[[str, str, str], float]


def probe_inmemory_r2_hot(_mode: str, primary: str, replica: str) -> float:
    """Synthetic in-memory R2-hot lag — InMemoryReplicationWorker reports 0."""
    # Sibling check mirrors ResidencyGraph::is_allowed.
    if SIBLING.get(primary) != replica:
        raise ValueError(
            f"ResidencyViolation: primary={primary} replica={replica}"
        )
    # In-memory worker replicates synchronously; lag is effectively 0.
    # Use a tiny non-zero value (50ms) to differentiate from "unmeasured".
    return 0.05


def probe_inmemory_r2_crr(_mode: str, primary: str, replica: str) -> float:
    """R2 platform CRR — simulate sub-hour lag in the in-memory fixture."""
    if SIBLING.get(primary) != replica:
        raise ValueError(f"ResidencyViolation: {primary} → {replica}")
    return 600.0  # 10 min — well under 24h budget


def probe_inmemory_d1(_mode: str, primary: str, replica: str) -> float:
    """D1 read-replica lag — simulate sub-second."""
    if SIBLING.get(primary) != replica:
        raise ValueError(f"ResidencyViolation: {primary} → {replica}")
    return 0.5


def probe_inmemory_kv(_mode: str, primary: str, replica: str) -> float:
    """KV global eventual — simulate 5s typical lag in fixture."""
    # KV is global — every region pair is valid, including non-siblings.
    return 5.0


def probe_inmemory_do(_mode: str, primary: str, _replica: str) -> float:
    """DO state — sync-to-D1 age (single-region; replica == primary)."""
    return 120.0  # 2 min sync-age, under 5min budget


def probe_inmemory_neon(_mode: str, _primary: str, _replica: str) -> float:
    """Neon replica lag — simulate sub-second."""
    return 0.8


# --- Staging / prod Prometheus probe ---------------------------------------
#
# Wire mode:
#   - HTTP: `CORELINK_PROMETHEUS_URL` env (e.g. `https://prom.staging.corelink.humangr.com`).
#     Queries `histogram_quantile(0.99, sum by (le, primary_region, replica_region)
#     (rate({metric}_bucket{{labels}}[1h])))` and reads per-pair p99 lag.
#   - Fixture: `CORELINK_VERIFIER_FIXTURE` env points to a JSON file with shape
#     `{"<domain>": {"<primary>-><replica>": <lag_seconds>, ...}, ...}`. This
#     path is the SOTA-rigor test surface for the staging/prod code path
#     without requiring a live Prometheus endpoint — CI can exercise the same
#     parser used in production.
#
# If neither env is set, exit-code 2 (inconclusive) is returned per the
# script header's exit-code contract.

PROMETHEUS_URL_ENV = "CORELINK_PROMETHEUS_URL"
VERIFIER_FIXTURE_ENV = "CORELINK_VERIFIER_FIXTURE"
PROMETHEUS_HTTP_TIMEOUT_SECS = 10.0


class ProbeUnwired(Exception):
    """Neither Prometheus URL nor fixture is configured for staging/prod."""


class ProbeQueryError(Exception):
    """The Prometheus query failed (network / parse / no data)."""


def _load_fixture() -> dict | None:
    path = os.environ.get(VERIFIER_FIXTURE_ENV)
    if not path:
        return None
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _query_prometheus_p99(metric: str, labels: str) -> dict[tuple[str, str], float]:
    """Query Prometheus for per-region-pair p99 lag.

    Returns a map ``{(primary, replica): lag_seconds}`` for every region pair
    the histogram emitted at least one observation for during the last 1h.

    Raises ``ProbeUnwired`` if `CORELINK_PROMETHEUS_URL` is not set.
    Raises ``ProbeQueryError`` on network / decode / "no data" failures.
    """
    base = os.environ.get(PROMETHEUS_URL_ENV)
    if not base:
        raise ProbeUnwired(
            f"{PROMETHEUS_URL_ENV} not set; cannot run staging/prod probe"
        )
    bucket_metric = f"{metric}_bucket"
    label_filter = f"{{{labels}}}" if labels else ""
    promql = (
        f"histogram_quantile(0.99, sum by (le, primary_region, replica_region) "
        f"(rate({bucket_metric}{label_filter}[1h])))"
    )
    url = base.rstrip("/") + "/api/v1/query?" + urllib.parse.urlencode({"query": promql})
    try:
        with urllib.request.urlopen(url, timeout=PROMETHEUS_HTTP_TIMEOUT_SECS) as resp:
            payload = json.load(resp)
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        raise ProbeQueryError(f"prometheus query failed: {e}") from e
    except json.JSONDecodeError as e:
        raise ProbeQueryError(f"prometheus response decode failed: {e}") from e

    if payload.get("status") != "success":
        raise ProbeQueryError(
            f"prometheus query non-success: {payload.get('error', '<no error field>')}"
        )
    out: dict[tuple[str, str], float] = {}
    for series in payload.get("data", {}).get("result", []):
        m = series.get("metric", {})
        primary = m.get("primary_region")
        replica = m.get("replica_region")
        value = series.get("value")
        if not primary or not replica or not value or len(value) != 2:
            continue
        try:
            out[(primary, replica)] = float(value[1])
        except (TypeError, ValueError):
            continue
    return out


# Cache so multiple region-pair calls per domain hit Prometheus only once.
_PROM_CACHE: dict[str, dict[tuple[str, str], float]] = {}
_FIXTURE_CACHE: dict | None = None
_FIXTURE_LOADED = False


def _staging_probe(domain: str) -> Probe:
    """Build the staging/prod probe for `domain`.

    Resolution order:
      1. `CORELINK_VERIFIER_FIXTURE` (deterministic test fixture).
      2. `CORELINK_PROMETHEUS_URL` (real Prometheus query — production path).
      3. Raise `ProbeUnwired` → verifier exits 2 (inconclusive, do NOT page).
    """

    def _p(_mode: str, primary: str, replica: str) -> float:
        global _FIXTURE_CACHE, _FIXTURE_LOADED
        # Fixture path (CI / deterministic regression).
        if not _FIXTURE_LOADED:
            _FIXTURE_CACHE = _load_fixture()
            _FIXTURE_LOADED = True
        if _FIXTURE_CACHE is not None:
            dom = _FIXTURE_CACHE.get(domain) or {}
            key = f"{primary}->{replica}"
            if key in dom:
                value = dom[key]
                if not isinstance(value, (int, float)):
                    raise ProbeQueryError(
                        f"fixture lag for {domain}/{key} is not numeric: {value!r}"
                    )
                return float(value)
            # Some domains (e.g. `do`) use primary==replica or sparse keys;
            # fall through to the cross-pair default of 0.0 only if explicitly
            # marked; otherwise treat missing key as inconclusive.
            raise ProbeQueryError(
                f"fixture missing entry for {domain}/{key}; "
                f"add it or remove the domain from the run"
            )

        # Real Prometheus path.
        if domain not in _PROM_CACHE:
            metric = PROM_METRIC[domain]
            labels = PROM_LABELS[domain]
            _PROM_CACHE[domain] = _query_prometheus_p99(metric, labels)
        per_pair = _PROM_CACHE[domain]
        if (primary, replica) in per_pair:
            return per_pair[(primary, replica)]
        # No data for the pair — most likely the histogram has not yet
        # received an observation for that pair (fresh deploy / cold pair).
        raise ProbeQueryError(
            f"prometheus has no observations for {domain} "
            f"primary={primary} replica={replica} in the last 1h"
        )

    return _p


PROBES_INMEMORY: dict[str, Probe] = {
    "r2_hot": probe_inmemory_r2_hot,
    "r2_crr": probe_inmemory_r2_crr,
    "d1": probe_inmemory_d1,
    "kv": probe_inmemory_kv,
    "do": probe_inmemory_do,
    "neon": probe_inmemory_neon,
}


def probes_for_mode(mode: str) -> dict[str, Probe]:
    if mode == "inmemory":
        return PROBES_INMEMORY
    # staging + prod share the same Prometheus / fixture probe per domain.
    return {d: _staging_probe(d) for d in PROBES_INMEMORY}


# ---------------------------------------------------------------------------
# Region-pair iteration.
# ---------------------------------------------------------------------------

def region_pairs_for_domain(domain: str) -> Iterable[tuple[str, str]]:
    """Yield (primary, replica) pairs to probe for the domain."""
    if domain in {"r2_hot", "r2_crr", "d1"}:
        # Active-passive sibling-pair only (residency-acyclic).
        for primary, replica in SIBLING.items():
            yield primary, replica
    elif domain == "kv":
        # KV is global — probe every (write, read) inter-region pair.
        for w in REGIONS:
            for r in REGIONS:
                if w != r:
                    yield w, r
    elif domain == "do":
        # DO is single-region; "replica" is conceptually the new
        # primary at failover. Probe each region's sync-age.
        for primary in REGIONS:
            yield primary, primary
    elif domain == "neon":
        # Neon: US primary → EU read replicas. Approximate: primary
        # `enam` (US-east) → other 3 regions.
        for replica in REGIONS:
            if replica != "enam":
                yield "enam", replica
    else:
        raise ValueError(f"unknown domain: {domain}")


# ---------------------------------------------------------------------------
# Main verification loop.
# ---------------------------------------------------------------------------

def run_verifier(
    mode: str,
    domains: Iterable[str],
) -> VerifierReport:
    report = VerifierReport(
        mode=mode,
        timestamp_ms=int(time.time() * 1000),
    )

    # Reset module-level caches so repeated calls in the same process are
    # idempotent (important for tests).
    global _PROM_CACHE, _FIXTURE_CACHE, _FIXTURE_LOADED
    _PROM_CACHE = {}
    _FIXTURE_CACHE = None
    _FIXTURE_LOADED = False

    probes = probes_for_mode(mode)

    for domain in domains:
        if domain not in RPO_BUDGET_SECS:
            report.error = f"unknown domain: {domain}"
            report.exit_code = 2
            return report
        probe = probes[domain]
        budget = RPO_BUDGET_SECS[domain]
        slo_id = SLO_IDS[domain]

        for primary, replica in region_pairs_for_domain(domain):
            try:
                lag = probe(mode, primary, replica)
            except (NotImplementedError, ProbeUnwired) as e:
                # Mode unwired — inconclusive, not a fail.
                report.error = str(e)
                report.exit_code = 2
                return report
            except ProbeQueryError as e:
                # Query failed (network / no data) — inconclusive, not SEV-paged.
                report.error = str(e)
                report.exit_code = 2
                return report
            except ValueError as e:
                # Residency violation: should NEVER happen — fail hard.
                report.results.append(
                    DomainResult(
                        domain=domain,
                        primary_region=primary,
                        replica_region=replica,
                        measured_lag_seconds=-1.0,
                        rpo_budget_seconds=budget,
                        slo_id=slo_id,
                        within_budget=False,
                        note=f"residency_violation: {e}",
                    )
                )
                continue

            within = lag <= budget
            report.add(
                DomainResult(
                    domain=domain,
                    primary_region=primary,
                    replica_region=replica,
                    measured_lag_seconds=round(lag, 3),
                    rpo_budget_seconds=budget,
                    slo_id=slo_id,
                    within_budget=within,
                    note="" if within else "OVER_BUDGET",
                )
            )

    report.finalize()
    return report


# ---------------------------------------------------------------------------
# Output formatting.
# ---------------------------------------------------------------------------

def format_text(report: VerifierReport) -> str:
    lines: list[str] = []
    lines.append(
        f"CoreLink replication-lag verifier — mode={report.mode}  "
        f"ts={report.timestamp_ms}"
    )
    lines.append("-" * 78)
    header = f"{'domain':10}{'primary':8}{'replica':8}{'lag_s':>10}{'budget_s':>10}  status"
    lines.append(header)
    lines.append("-" * 78)
    for r in report.results:
        status = "OK" if r.within_budget else "FAIL"
        lines.append(
            f"{r.domain:10}{r.primary_region:8}{r.replica_region:8}"
            f"{r.measured_lag_seconds:>10}{r.rpo_budget_seconds:>10}  {status}"
            + (f"  ({r.note})" if r.note else "")
        )
    lines.append("-" * 78)
    if report.error:
        lines.append(f"ERROR: {report.error}")
    if report.fails:
        lines.append(
            f"FAIL: {len(report.fails)} domain/region pair(s) "
            "exceeded RPO budget."
        )
    else:
        lines.append("PASS: all probed pairs within RPO budget.")
    return "\n".join(lines)


def format_json(report: VerifierReport) -> str:
    payload: dict = {
        "mode": report.mode,
        "timestamp_ms": report.timestamp_ms,
        "results": [asdict(r) for r in report.results],
        "fails": len(report.fails),
        "exit_code": report.exit_code,
    }
    if report.error:
        payload["error"] = report.error
    return json.dumps(payload, indent=2, sort_keys=True)


# ---------------------------------------------------------------------------
# CLI.
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="verify-replication-lag",
        description=(
            "Verify cross-region replication lag per data domain (R2 / "
            "D1 / KV / DO / Neon). Daily cron entry point for the "
            "R-prep replication audit (2026-05-15-replication-audit.md)."
        ),
    )
    p.add_argument(
        "--mode",
        choices=("inmemory", "staging", "prod"),
        default=os.environ.get("CORELINK_VERIFIER_MODE", "inmemory"),
        help=(
            "inmemory = deterministic fixture (default; no network). "
            "staging/prod = query live Prometheus (not yet wired; "
            "see follow-up tickets P0-001..004)."
        ),
    )
    p.add_argument(
        "--domain",
        action="append",
        choices=tuple(RPO_BUDGET_SECS.keys()),
        help=(
            "Restrict probe to one or more domains (repeat flag). "
            "Default: all domains."
        ),
    )
    p.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON instead of human-readable text.",
    )
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    domains = args.domain or list(RPO_BUDGET_SECS.keys())
    report = run_verifier(mode=args.mode, domains=domains)
    out = format_json(report) if args.json else format_text(report)
    print(out)
    return report.exit_code


if __name__ == "__main__":
    sys.exit(main())
