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
| smoke-install | Docker-compatible backend preflight, runner provenance, credential-free local fixture, and structured receipt | no `corelink` lease/run was dispatched from this branch | UNMEASURED |
| cosign-sign | retired 2026-09-08 with B-118; the former OCI lane is not a candidate for the shim experiment | workflow removed before a `corelink` lease/run; no image, signature, or deploy evidence exists | RETIRED (B-118) |

## Truth boundary

This candidate is not a runtime pass. No remote workflow dispatch, release push,
registry push, OIDC exchange, Rekor entry, deploy webhook, or production secret
was used while preparing this branch. The credential-free workflow only observes
the backend boundary and a local fixture; it does not execute the separate image
build, installer, `corelink --version`, authenticated `corelink doctor`, publish,
or deploy acceptance path. The static contract therefore keeps this item
`status: open`; only `smoke-install` remains `UNMEASURED`. The former
`cosign-sign` row is retired by B-118, not treated as an unmeasured candidate.
A future operator may promote the smoke row only with a GitHub run ID, job
conclusion, runner label, backend log, structured receipt, and the additional
acceptance evidence required by the issue. A skipped, queued, refused, or
placeholder-only run is not a pass.

## Gates and mutation controls

```sh
python3 scripts/check_b134_observability.py
bash scripts/test_b134_observability.sh
```

The mutation suite turns red when the smoke workflow leaves `corelink`, loses
runner provenance, backend observation, helper wiring, fail-closed execution,
or receipt upload, or when this ledger is changed to `PASS` without runtime
evidence. If the former cosign workflow is recreated, the checker rejects it
because B-118 retired that lane. It is entirely local and does not assert that
the backend, registry, OIDC, PAT, or deploy verifier is currently available.
