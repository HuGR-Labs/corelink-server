# Issue #1687 bounded endurance lane

This change proves the hosted lane contract while the owner approved staging
boundary remains blocked by #1700. The workflow has `workflow_dispatch` only,
accepts `30s` or `2h`, and rejects every target except
`https://staging.corelink.humangr.com` after removing one trailing slash.

Each dispatch records the merged commit SHA, run and attempt identity, runner
identity, canonical target, lifecycle checkpoints, a 30 second heartbeat, and
SHA-256 digests for result artifacts. The k6 command has a 130 minute graceful
timeout inside the 145 minute job bound. The receipt and teardown checkpoint
run with `if: always()` so a timeout or runner failure remains attributable.

Contract and mutation checks:

```sh
python3 scripts/test_i1687_endurance_lane.py
```

No live dispatch or staging evidence is claimed here. Issue #1687 stays open
until #1700 approves and provisions the canonical target and one hosted run
retains the resulting receipt.
