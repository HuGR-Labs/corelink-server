"""The watchdog's decision, branch by branch.

The branch that matters most is the interlock: a running job must NEVER be
restarted out from under itself. It is asserted first and asserted hardest,
including the case where GitHub simultaneously reports a long-queued backlog —
which is exactly the state a busy-but-slow fleet is in, and the state where a
naive watchdog would kill live work.
"""

import importlib.util
import pathlib
import sys

_SPEC = importlib.util.spec_from_file_location(
    "runner_wedge_watchdog",
    pathlib.Path(__file__).resolve().parents[1] / "scripts" / "runner_wedge_watchdog.py",
)
assert _SPEC and _SPEC.loader
wd = importlib.util.module_from_spec(_SPEC)
sys.modules["runner_wedge_watchdog"] = wd
_SPEC.loader.exec_module(wd)


def test_never_restarts_while_a_job_runs_here_even_with_a_long_backlog():
    v = wd.decide(queued_job_ages_s=[36_000.0, 7_200.0], local_worker_count=1, min_age_s=600)
    assert v.act is False
    assert "job is running here" in v.reason


def test_acts_when_work_is_stale_and_nothing_runs_here():
    v = wd.decide(queued_job_ages_s=[43_200.0], local_worker_count=0, min_age_s=600)
    assert v.act is True
    assert "43200" in v.reason.replace(",", "") or "43200s" in v.reason.replace(",", "")


def test_holds_while_the_queue_is_still_fresh():
    # A job queued 30 s ago is normal scheduling, not a wedge.
    v = wd.decide(queued_job_ages_s=[30.0], local_worker_count=0, min_age_s=600)
    assert v.act is False
    assert "none older than" in v.reason


def test_holds_on_an_idle_fleet():
    v = wd.decide(queued_job_ages_s=[], local_worker_count=0, min_age_s=600)
    assert v.act is False
    assert v.reason == "nothing queued for our labels"


def test_the_age_threshold_is_a_boundary_not_a_suggestion():
    exactly = wd.decide(queued_job_ages_s=[600.0], local_worker_count=0, min_age_s=600)
    just_under = wd.decide(queued_job_ages_s=[599.9], local_worker_count=0, min_age_s=600)
    assert exactly.act is True
    assert just_under.act is False


def test_the_service_list_is_explicit_and_scoped_to_this_project():
    # A wildcard over `actions.runner.*` would also match the runners of the
    # other projects that share this Mac. Restarting one of those mid-job is
    # cross-project damage, so the list is named, and it names only ours.
    assert len(wd.BUILDER_SERVICES) == 5
    assert all("corelink-server" in s and "corelink-builder-" in s for s in wd.BUILDER_SERVICES)
    assert wd.BUILDER_LABELS == frozenset({"self-hosted", "mac", "corelink-builder"})
