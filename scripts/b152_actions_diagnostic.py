#!/usr/bin/env python3
"""Read-only GitHub Actions evidence collector for backlog item B-152.

The collector deliberately does not mutate GitHub state.  It walks the complete
run set in a UTC time window (splitting intervals when GitHub's 1,000-result
search cap would otherwise make pagination incomplete), then walks every jobs
page for failed runs.  Job logs are fetched only when requested and a missing
or expired log is reported as indeterminate rather than treated as clean.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlencode


UTC = dt.timezone.utc
MAX_SEARCH_RESULTS = 1_000
PAGE_SIZE = 100
STEP_STATUSES = frozenset({"queued", "in_progress", "completed"})
FRACTIONAL_COMPONENTS = re.compile(r"[.,](\d+)")


class EvidenceUnavailable(RuntimeError):
    """The API response cannot establish a complete evidence set."""


def parse_time(value: str) -> dt.datetime:
    raw_value = value.strip()
    if any(len(component.group(1)) > 6 for component in FRACTIONAL_COMPONENTS.finditer(raw_value)):
        raise argparse.ArgumentTypeError("timestamps may contain at most six fractional digits")
    value = raw_value.replace("Z", "+00:00")
    try:
        parsed = dt.datetime.fromisoformat(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"invalid ISO-8601 time: {value}") from exc
    if parsed.tzinfo is None:
        raise argparse.ArgumentTypeError("times must include a UTC offset")
    return parsed.astimezone(UTC)


def iso(value: dt.datetime) -> str:
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")


def run_gh(repo: str, endpoint: str) -> Any:
    """Read one API page through gh, keeping credentials out of argv/output."""
    for attempt in range(3):
        try:
            proc = subprocess.run(
                ["gh", "api", endpoint],
                check=False,
                capture_output=True,
                text=True,
            )
        except OSError as exc:
            raise EvidenceUnavailable("gh api could not be started") from exc
        if proc.returncode == 0:
            try:
                return json.loads(proc.stdout)
            except json.JSONDecodeError as exc:
                raise EvidenceUnavailable("gh api returned non-JSON") from exc
        if attempt < 2:
            time.sleep(2**attempt)
    raise EvidenceUnavailable("gh api failed after retries")


def api_id(value: Any, kind: str) -> int:
    """Return a positive numeric API identifier or fail closed."""
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise EvidenceUnavailable(f"{kind} has malformed id")
    return value


def page_runs(repo: str, start: dt.datetime, end: dt.datetime, page: int) -> dict[str, Any]:
    query = urlencode({"per_page": PAGE_SIZE, "page": page, "created": f"{iso(start)}..{iso(end)}"})
    result = run_gh(repo, f"repos/{repo}/actions/runs?{query}")
    if not isinstance(result, dict) or not isinstance(result.get("workflow_runs"), list):
        raise EvidenceUnavailable("run page has no workflow_runs array")
    total = result.get("total_count")
    if isinstance(total, bool) or not isinstance(total, int) or total < 0:
        raise EvidenceUnavailable("run page has invalid total_count")
    if any(not isinstance(item, dict) for item in result["workflow_runs"]):
        raise EvidenceUnavailable("run page contains a malformed workflow run")
    return result


def collect_runs(repo: str, start: dt.datetime, end: dt.datetime) -> list[dict[str, Any]]:
    """Collect all runs, recursively avoiding GitHub's 1,000-result cap."""
    if start >= end:
        raise EvidenceUnavailable("start must be before end")
    first = page_runs(repo, start, end, 1)
    total = first["total_count"]
    # At the documented cap, the API cannot distinguish exactly 1,000 matches
    # from a larger result set truncated to 1,000.  Split that case too.
    if total >= MAX_SEARCH_RESULTS:
        midpoint = start + (end - start) / 2
        if midpoint <= start or midpoint >= end:
            raise EvidenceUnavailable("run search exceeds GitHub result cap at minimum interval")
        partitions = collect_runs(repo, start, midpoint) + collect_runs(repo, midpoint, end)
        selected: dict[int, dict[str, Any]] = {}
        for run in partitions:
            run_id = api_id(run.get("id"), "run")
            if run_id in selected:
                raise EvidenceUnavailable("run partitions contain a duplicate run id")
            selected[run_id] = run
        return list(selected.values())

    runs: list[dict[str, Any]] = []
    page = 1
    while True:
        payload = first if page == 1 else page_runs(repo, start, end, page)
        if payload["total_count"] != total:
            raise EvidenceUnavailable("run pagination total_count changed during collection")
        batch = payload["workflow_runs"]
        expected = min(PAGE_SIZE, total - len(runs))
        if len(batch) != expected:
            raise EvidenceUnavailable("run pagination is incomplete or inconsistent")
        runs.extend(batch)
        if len(runs) == total:
            break
        page += 1
        if page > MAX_SEARCH_RESULTS // PAGE_SIZE:
            raise EvidenceUnavailable("run pagination exceeded the known GitHub result cap")

    # The API's created filter is date-granular in some deployments. Post-filter
    # exact UTC bounds; duplicated run IDs are inconsistent evidence and fail closed.
    selected: dict[int, dict[str, Any]] = {}
    for run in runs:
        try:
            created_value = run["created_at"]
            if not isinstance(created_value, str):
                raise EvidenceUnavailable("run page contains malformed identity/timestamp")
            created = parse_time(created_value)
            run_id = api_id(run["id"], "run")
        except (KeyError, ValueError, argparse.ArgumentTypeError) as exc:
            raise EvidenceUnavailable("run page contains malformed identity/timestamp") from exc
        if run_id in selected:
            raise EvidenceUnavailable("run page contains a duplicate run id")
        if start <= created < end:
            selected[run_id] = run
    return list(selected.values())


def collect_jobs(repo: str, run: dict[str, Any]) -> list[dict[str, Any]]:
    try:
        run_id = api_id(run["id"], "run")
    except KeyError as exc:
        raise EvidenceUnavailable("run has malformed id") from exc
    jobs: list[dict[str, Any]] = []
    job_ids: set[int] = set()
    expected_total: int | None = None
    # A full page is not evidence that the listing ended.  In particular,
    # exactly 1,000 jobs requires a request for page 11 to distinguish a full
    # result from an API-side truncation.
    for page in range(1, 12):
        query = urlencode({"per_page": PAGE_SIZE, "page": page})
        payload = run_gh(repo, f"repos/{repo}/actions/runs/{run_id}/jobs?{query}")
        batch = payload.get("jobs") if isinstance(payload, dict) else None
        if not isinstance(batch, list):
            raise EvidenceUnavailable(f"run {run_id} jobs page has no jobs array")
        total = payload.get("total_count") if isinstance(payload, dict) else None
        if isinstance(total, bool) or not isinstance(total, int) or total < 0:
            raise EvidenceUnavailable(f"run {run_id} jobs page has invalid total_count")
        if expected_total is None:
            expected_total = total
        elif total != expected_total:
            raise EvidenceUnavailable(f"run {run_id} jobs total_count changed during collection")
        if total > MAX_SEARCH_RESULTS:
            raise EvidenceUnavailable(f"run {run_id} jobs exceed the API result cap")
        if any(not isinstance(item, dict) for item in batch):
            raise EvidenceUnavailable(f"run {run_id} jobs page contains a malformed job")
        for job in batch:
            job_id = api_id(job.get("id"), "job")
            if job_id in job_ids:
                raise EvidenceUnavailable(f"run {run_id} jobs page contains a duplicate job id")
            job_ids.add(job_id)
        expected = min(PAGE_SIZE, total - len(jobs))
        if len(batch) != expected:
            raise EvidenceUnavailable(f"run {run_id} jobs pagination is incomplete or inconsistent")
        jobs.extend(batch)
        if len(jobs) == total:
            # A full final page needs one explicit empty-page probe.  Otherwise
            # an API-side cap at exactly 100/…/1,000 is indistinguishable from
            # a complete listing.
            if len(batch) < PAGE_SIZE:
                return jobs
    raise EvidenceUnavailable(f"run {run_id} jobs pagination exceeded 1,100 jobs")


def whole_seconds(delta: dt.timedelta, evidence: str) -> int:
    """Reject negative or fractional evidence instead of silently truncating it."""
    if delta < dt.timedelta(0):
        raise EvidenceUnavailable(f"job has negative {evidence} duration")
    if delta.microseconds:
        raise EvidenceUnavailable(f"job has fractional {evidence} duration")
    return delta.days * 86_400 + delta.seconds


def duration(job: dict[str, Any]) -> int:
    try:
        started_value = job["started_at"]
        completed_value = job["completed_at"]
        if not isinstance(started_value, str) or not isinstance(completed_value, str):
            raise EvidenceUnavailable("job has malformed runtime timestamps")
        started = parse_time(started_value)
        completed = parse_time(completed_value)
    except (KeyError, ValueError, argparse.ArgumentTypeError) as exc:
        raise EvidenceUnavailable("job has malformed runtime timestamps") from exc
    return whole_seconds(completed - started, "runtime")


def queue_duration(job: dict[str, Any]) -> int:
    """Return runner queue wait as job.created_at -> job.started_at."""
    try:
        created_value = job["created_at"]
        started_value = job["started_at"]
        if not isinstance(created_value, str) or not isinstance(started_value, str):
            raise EvidenceUnavailable("job has malformed queue timestamps")
        created = parse_time(created_value)
        started = parse_time(started_value)
    except (KeyError, ValueError, argparse.ArgumentTypeError) as exc:
        raise EvidenceUnavailable("job has malformed queue timestamps") from exc
    return whole_seconds(started - created, "queue")


def summarize_queue(records: Iterable[dict[str, Any]]) -> dict[str, Any]:
    values = sorted(
        item["queue_duration_seconds"]
        for item in records
        if item.get("queue_duration_seconds") is not None
    )
    if not values:
        return {"count": 0, "min_seconds": None, "median_seconds": None, "max_seconds": None}
    middle = len(values) // 2
    median = values[middle] if len(values) % 2 else (values[middle - 1] + values[middle]) / 2
    return {
        "count": len(values),
        "min_seconds": values[0],
        "median_seconds": median,
        "max_seconds": values[-1],
    }


def classify_job(job: dict[str, Any], run: dict[str, Any], low: int, high: int) -> dict[str, Any]:
    steps = job.get("steps")
    if not isinstance(steps, list) or any(not isinstance(step, dict) for step in steps):
        raise EvidenceUnavailable("job has malformed step evidence")
    if any(
        not isinstance(step.get("name"), str)
        or step.get("status") not in STEP_STATUSES
        for step in steps
    ):
        raise EvidenceUnavailable("job has malformed step evidence")
    workflow = run.get("name")
    lane = job.get("name")
    runner_name = job.get("runner_name")
    if not isinstance(workflow, str) or not isinstance(lane, str):
        raise EvidenceUnavailable("job has malformed workflow or lane evidence")
    if runner_name is not None and not isinstance(runner_name, str):
        raise EvidenceUnavailable("job has malformed runner evidence")
    in_progress = [step["name"] for step in steps if step["status"] == "in_progress"]
    checkout = [
        s for s in steps
        if "checkout" in s["name"].lower()
        # Post-checkout cleanup is pending after an external kill and does
        # not mean that source checkout itself was incomplete.
        and not s["name"].lower().startswith("post ")
    ]
    checkout_incomplete = any(s.get("status") != "completed" for s in checkout)
    seconds = duration(job)
    return {
        "run_id": api_id(run.get("id"), "run"),
        "job_id": api_id(job.get("id"), "job"),
        "workflow": workflow,
        "lane": lane,
        "conclusion": job.get("conclusion"),
        "started_at": job.get("started_at"),
        "completed_at": job.get("completed_at"),
        "duration_seconds": seconds,
        "queue_duration_seconds": queue_duration(job),
        "runner_name": runner_name,
        "step_in_progress": in_progress,
        "checkout_incomplete": checkout_incomplete,
        "checkout_observed": bool(checkout),
        "in_window": seconds is not None and low <= seconds <= high,
    }


def collect_evidence(repo: str, start: dt.datetime, end: dt.datetime, low: int, high: int) -> dict[str, Any]:
    runs = collect_runs(repo, start, end)
    failed_runs = [r for r in runs if r.get("conclusion") == "failure"]
    records: list[dict[str, Any]] = []
    for run in failed_runs:
        for job in collect_jobs(repo, run):
            if job.get("conclusion") == "failure":
                records.append(classify_job(job, run, low, high))
    return {
        "repo": repo,
        "window": {"start": iso(start), "end_exclusive": iso(end)},
        "duration_window_seconds": {"low": low, "high": high},
        "run_count": len(runs),
        "run_ids": sorted(int(r["id"]) for r in runs),
        "failed_run_count": len(failed_runs),
        "failed_job_count": len(records),
        "window_jobs": [r for r in records if r["in_window"]],
        "failed_jobs": records,
        "lane_distribution": dict(Counter(r["lane"] for r in records if r["in_window"])),
        "queue_duration": {
            "measured_from": "job.created_at -> job.started_at",
            "not_run_started_at": True,
            "summary_for_window_jobs": summarize_queue(r for r in records if r["in_window"]),
        },
        "limitations": [
            "job duration is completed_at - started_at, not declared timeout-minutes",
            "run started_at is not runner queue wait; queue wait requires job created_at - started_at",
            "absence in this window is not proof of cure",
        ],
    }


def fetch_logs(repo: str, records: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    """Fetch logs without persisting them; 404/transport errors remain indeterminate."""
    result = []
    for record in records:
        job_id = record["job_id"]
        try:
            proc = subprocess.run(
                ["gh", "api", f"repos/{repo}/actions/jobs/{job_id}/logs"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
            )
        except OSError as exc:
            raise EvidenceUnavailable("gh api could not be started while fetching logs") from exc
        result.append({
            "job_id": job_id,
            "available": proc.returncode == 0,
            "status": "available" if proc.returncode == 0 else "indeterminate",
            "causal": False if proc.returncode == 0 else "indeterminate",
            "error": None if proc.returncode == 0 else "log retrieval failed",
        })
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "HuGR-Labs/corelink-server"))
    parser.add_argument("--start", required=True, type=parse_time)
    parser.add_argument("--end", required=True, type=parse_time)
    parser.add_argument("--low", type=int, default=594)
    parser.add_argument("--high", type=int, default=615)
    parser.add_argument("--known-run", action="append", type=int, default=[])
    parser.add_argument("--fetch-logs", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.low < 0 or args.high < args.low:
        parser.error("duration bounds are invalid")
    try:
        report = collect_evidence(args.repo, args.start, args.end, args.low, args.high)
        # Controls may be successful/cancelled, so compare against all selected
        # run IDs, not only failed jobs.  This also avoids a second API walk.
        missing = [run_id for run_id in args.known_run if run_id not in set(report.get("run_ids", []))]
        if missing:
            raise EvidenceUnavailable(f"known run(s) absent from complete window: {','.join(map(str, missing))}")
        if args.fetch_logs:
            report["logs"] = fetch_logs(args.repo, report["window_jobs"])
            unavailable = [item for item in report["logs"] if item["status"] == "indeterminate"]
            report["causal_classification"] = {
                "status": "indeterminate" if unavailable else "not_established",
                "causal": "indeterminate" if unavailable else False,
                "reason": (
                    "one or more job log blobs were unavailable; no causal conclusion is permitted"
                    if unavailable else "logs available, but availability alone does not establish a common cause"
                ),
                "unavailable_job_ids": [item["job_id"] for item in unavailable],
            }
        else:
            report["causal_classification"] = {
                "status": "indeterminate",
                "causal": "indeterminate",
                "reason": "logs were not requested; no causal conclusion is permitted",
                "unavailable_job_ids": [],
            }
    except EvidenceUnavailable as exc:
        print(f"INDETERMINATE: {exc}", file=sys.stderr)
        return 2
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    try:
        if args.output:
            args.output.write_text(rendered, encoding="utf-8")
        else:
            print(rendered, end="")
    except OSError:
        print("INDETERMINATE: report output could not be written", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
