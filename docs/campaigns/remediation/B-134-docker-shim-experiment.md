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
| cosign-sign | retired 2026-09-08 with B-118; the former OCI lane is not a candidate for the shim experiment | workflow removed before a `corelink` lease/run; no image, signature, or deploy evidence exists | RETIRED (B-118) |

## Truth boundary

This candidate is not a runtime pass. No remote workflow dispatch, release push,
registry push, OIDC exchange, Rekor entry, deploy webhook, or production secret
was used while preparing this branch. The static contract therefore keeps this
item `status: open`; only `smoke-install` remains `UNMEASURED`. The former
`cosign-sign` row is retired by B-118, not treated as an unmeasured candidate.
A future operator may promote the smoke row only with a GitHub run ID, job
conclusion, runner label, and step logs. A skipped, queued, refused, or
placeholder-only run is not a pass.

## Gates and mutation controls

```sh
python3 scripts/check_b134_observability.py
bash scripts/test_b134_observability.sh
```

The mutation suite turns red when the smoke workflow leaves `corelink`, when a
live cosign workflow loses its real `cosign verify` step, when placeholder text
is introduced, or when this ledger is changed to `PASS` without runtime
evidence. If the former cosign workflow is recreated, the checker requires its
full contract again. It is entirely local and does not assert that the backend,
registry, OIDC, PAT, or deploy verifier is currently available.
