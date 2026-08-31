### Fixed

- **The only API contract CoreLink published in production described eight
  DevEnv endpoints and nothing else.** Measured against prod, not inferred:
  `GET https://corelink-api.humangr.com/openapi.json` returned 200, 7707 bytes,
  `info.title = "CoreLink DevEnv API"`, 8 paths, every one `/v1/customer/devenv*`.
  It was never a broken import — `/openapi.json` did not exist before the DevEnv
  package introduced it (#1432) and took the canonical public path with it, so
  the other 30 paths of the real contract had never reached a customer.
  `/openapi.json` now serves the CoreLink contract, generated from
  `openapi/corelink-v1.yaml` by `scripts/openapi_sync.py` and gated against
  drift by the same `--check` that already gated the JSON sibling.
- **The DevEnv spec is now published only where DevEnv is wired.** All eight of
  its endpoints answer 503 unless the `RUNNER_DEVENV_DO` binding exists, and
  that binding is absent from every deployed environment — read from the live
  Worker through the Cloudflare API, with the six real Durable Object bindings
  in the same response as the positive control. `GET /openapi/devenv.json`
  therefore 404s with a reason where the binding is missing and serves the spec
  where it is present, so a published contract cannot outlive the feature it
  describes, and it starts serving on its own once the binding is deployed.
