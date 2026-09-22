#!/usr/bin/env python3
"""Write a minimal receipt proving dispatch and environment approval were separate."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


APPROVAL_SCHEMA = "corelink.b054.custody-approval-receipt.v1"
ENVIRONMENT = "b054-key-custody-drill"


class ApprovalError(RuntimeError):
    pass


def required(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise ApprovalError(f"required workflow context is absent: {name}")
    return value


def main() -> int:
    try:
        token = required("GH_TOKEN")
        actor = required("GITHUB_ACTOR")
        repository = required("GITHUB_REPOSITORY")
        run_id = required("GITHUB_RUN_ID")
        receipt_path = Path(required("B054_APPROVAL_RECEIPT_PATH"))
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
            raise ApprovalError("repository context has an invalid shape")
        if not run_id.isdecimal():
            raise ApprovalError("workflow run id has an invalid shape")

        endpoint = f"https://api.github.com/repos/{repository}/actions/runs/{run_id}/approvals"
        request = Request(
            endpoint,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {token}",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        try:
            with urlopen(request, timeout=20) as response:
                approvals = json.load(response)
        except HTTPError as error:
            raise ApprovalError(f"workflow approval read failed with HTTP {error.code}") from None
        except (URLError, TimeoutError, json.JSONDecodeError):
            raise ApprovalError("workflow approval read failed") from None
        if not isinstance(approvals, list):
            raise ApprovalError("workflow approval API returned an unexpected shape")

        reviewers: set[str] = set()
        for approval in approvals:
            if not isinstance(approval, dict) or approval.get("state") != "approved":
                continue
            environments = approval.get("environments")
            if not isinstance(environments, list) or not any(
                isinstance(environment, dict) and environment.get("name") == ENVIRONMENT
                for environment in environments
            ):
                continue
            user = approval.get("user")
            login = user.get("login") if isinstance(user, dict) else None
            if isinstance(login, str) and login:
                reviewers.add(login)

        if not reviewers:
            raise ApprovalError("no approved review for the protected custody environment was recorded")
        if actor in reviewers:
            raise ApprovalError("dispatcher appears among environment approvers")

        receipt = {
            "schema": APPROVAL_SCHEMA,
            "environment": ENVIRONMENT,
            "state": "approved",
            "workflow_run_id": run_id,
            "dispatcher": actor,
            "approvers": sorted(reviewers),
            "distinct_people": True,
        }
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    except ApprovalError as error:
        print(f"B-054 custody approval receipt: FAIL ({error})", file=sys.stderr)
        return 1
    except OSError:
        print("B-054 custody approval receipt: FAIL (receipt could not be written)", file=sys.stderr)
        return 1

    print("B-054 custody approval receipt: PASS (independent reviewer recorded)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
