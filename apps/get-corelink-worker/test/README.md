# smoke-install — CoreLink-fleet boundary observation

The i1672 lane is a manual workflow on the `corelink` runner label. It uses no
production endpoint or secret. Hosted campaign CI runs only static contract
checks; it is not runtime fleet evidence. The fleet job observes two boundaries
independently:

1. `docker info` records whether the runner's Docker daemon or shim is alive.
2. `scripts/smoke_install_observe.py` starts a disposable local HTTP process and
   records startup, readiness, correct and incorrect endpoint behavior, timeout,
   process death, cleanup, and receipt provenance.

The structured receipt is uploaded at
`artifacts/i1672/smoke-install-receipt.json`. A backend observation cannot turn
into a service success: the receipt keeps `backend` and `service.classification`
separate and fails the workflow when either boundary is unavailable.

The older `smoke-install.Dockerfile` remains a fixture for the historical B-134
static contract. It is not built, pushed, or deployed by the i1672 lane. This
lane does not prove the separate image build, installer/version, doctor,
published-image, or deploy/webhook acceptance paths.

## Contract suite

Run the local, contract-only checks with:

```sh
python3 -m pytest -q tests/test_i1672_smoke_install_contract.py
```

These checks do not dispatch Actions or contact production. Runtime evidence
requires a manual `workflow_dispatch` and its uploaded receipt.
