# B-134 — Docker-compatible backend observability

**Status:** open — candidate routed, runtime evidence not yet observed

**Base:** `b91ec17c8d559ae0323ba00a80eb309c900d599e`

The two release/canary surfaces are now candidate-routed to the `corelink`
fleet. The runner image documents a Docker-compatible `docker` drop-in backed by
nerdctl/containerd/buildkitd; that is not a Docker daemon. This record makes
the experiment explicit and prevents a skipped job, an unrelated direct
`buildctl` run, or a successful log-only placeholder from being counted as a
pass.

| workflow | candidate controls | runtime evidence | verdict |
|---|---|---|---|
| smoke-install | backend preflight, image build, install + `corelink --version`, authenticated `corelink doctor` | no `corelink` lease/run was dispatched from this branch | UNMEASURED |
| cosign-sign | backend preflight, buildx-compatible image build/push, keyless `cosign sign`, Rekor-backed `cosign verify`, deploy preflight | no `corelink` lease/run was dispatched from this branch | UNMEASURED |

## Truth boundary

This candidate is not a runtime pass. No remote workflow dispatch, release push,
registry push, OIDC exchange, Rekor entry, deploy webhook, or production secret
was used while preparing this branch. The static contract therefore keeps this
item `status: open` and both rows `UNMEASURED`. A future operator may promote a
row only with a GitHub run ID, job conclusion, runner label, step logs, and the
corresponding image/signature evidence. A skipped, queued, refused, or
placeholder-only run is not a pass.

## Gates and mutation controls

```sh
python3 scripts/check_b134_observability.py
bash scripts/test_b134_observability.sh
```

The mutation suite turns red when either workflow leaves `corelink`, when the
real `cosign verify` step disappears, when placeholder text is introduced, or
when this ledger is changed to `PASS` without runtime evidence. It is entirely
local and does not assert that the backend, registry, OIDC, PAT, or deploy
verifier is currently available.
