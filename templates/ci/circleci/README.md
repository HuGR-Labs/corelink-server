# CoreLink — CircleCI Integration

## Quick start (≤ 5 min)

1. Copy `corelink-cache.yml` to `.circleci/config.yml`:
   ```bash
   cp templates/ci/circleci/corelink-cache.yml .circleci/config.yml
   ```

2. Create a CircleCI context named `corelink-secrets`
   (Organization Settings → Contexts → Create Context).
   Add environment variable `CORELINK_PAT`.


3. Push — the workflow runs on `main` and `release/**` branches.

## Cache hit ratio

After each run, the `bazel-build` job logs:
```
CoreLink Cache Hit Ratio: 82.5% (198/240 actions)
```

The ratio is also exported to `BASH_ENV` as `CORELINK_CACHE_HIT_RATIO` for downstream steps.

## Security (CTRL-CRED-001)

`CORELINK_PAT` is stored in an encrypted CircleCI context and never appears in job arguments.

## Troubleshooting

Run `corelink doctor` locally to diagnose connectivity issues.
