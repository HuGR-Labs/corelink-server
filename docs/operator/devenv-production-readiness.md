# DevEnv production readiness gate

The `RUNNER_DEVENV_DO` cross-worker binding is intentionally inactive in this
repository. `wrangler.toml` must remain unchanged and must contain no live
binding until the sibling Runners deployment is independently verified.

NO live enable: the operator must confirm all of the following against
`corelink-runners` `origin/main`: the `RunnerDevEnvDO` class and deployment
verifier, the immutable runner image digest, and the signer/verifier public-key
map. The CoreLink Server migration preflight must cover the inclusive range
0118-0130. Absence of any preflight receipt is a hard failure; do not deploy or
flip the binding.

The optional Worker compute-grant verifier names are canonicalized as
`COMPUTE_GRANT_SIGNING_KEY` and `COMPUTE_GRANT_KEY_ID`; the obsolete
`COMPUTE_GRANT_SIGNING_KEY_ID` alias is forbidden.

Run the offline check before proposing a deployment:

```sh
python3 scripts/verify_devenv_deploy_contract.py --self-test
```

This check does not contact Cloudflare, read secrets, acquire leases, modify
Workers, or enable the binding. The exact inactive contract is recorded in
`tests/fixtures/deployment/devenv-cross-worker-binding.toml` for review.
