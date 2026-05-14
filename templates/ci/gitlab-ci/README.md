# CoreLink — GitLab CI Integration

## Quick start (≤ 5 min)

1. Copy `corelink-cache.yml` to your project root as `.gitlab-ci.yml`:
   ```bash
   cp templates/ci/gitlab-ci/corelink-cache.yml .gitlab-ci.yml
   ```

2. Add `CORELINK_PAT` as a **masked, protected** CI/CD variable
   (Settings → CI/CD → Variables → Add variable).

3. Replace `<YOUR_TENANT_ID>` in the file with your CoreLink tenant slug.

4. Push — the pipeline runs on every push.

## Cache hit ratio

After each pipeline, the `cache-hit-report` job logs the cache hit ratio:
```
CoreLink Cache Hit Ratio: 87.3% (214/245 actions)
```

## Security (CTRL-CRED-001)

`CORELINK_PAT` is a GitLab masked variable. The credential helper reads it from the
environment — never from argv.

## Troubleshooting

Run `corelink doctor` locally to diagnose connectivity issues.
