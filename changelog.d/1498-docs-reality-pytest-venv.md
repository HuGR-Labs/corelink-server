### Fixed

- **`docs-reality`'s regression suite depended on which machine picked up the job.**
  The lane invoked `python3 -m pytest tests/test_validate_docs_reality_hostnames.py`
  against whatever interpreter led the runner's `PATH`. That worked by accident of
  the machine on the mac fleet, which happens to carry an ambient pytest, and fails
  on the `corelink` Firecracker image, whose `/usr/bin/python3` carries none — so
  moving the lane off the owner's Mac (WP-CI) surfaced it immediately as
  `No module named pytest`, with the validator itself having already passed
  (`OK docs-reality: 322 CLI refs, 3198 hostname mentions … 0 drift`). The repo had
  already predicted this exact failure: `python-tests.yml` names *this* step as the
  one to port its venv idiom to, and `requirements-ci.txt` states outright that the
  `corelink` image has no pytest. Ported that idiom — a venv on the system python3
  installing `requirements-ci.txt`, with no `actions/setup-python` because
  `RUNNER_TOOL_CACHE` is unset on the fleet and the action dies on an unwritable
  path — plus the sibling lane's `command -v python3` / `pytest --version` probe so
  a future `PATH` surprise appears in the log instead of being inferred from a
  failure. The suite now declares its own dependency on both fleets rather than
  inheriting one from the host.
