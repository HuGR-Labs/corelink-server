# CoreLink — GitHub Actions Integration

## Quick start (≤ 5 min)

1. Copy `corelink-cache.yml` to `.github/workflows/`:
   ```bash
   cp templates/ci/github-actions/corelink-cache.yml .github/workflows/
   ```

2. Add `CORELINK_PAT` to your repository secrets (Settings → Secrets → Actions).

3. Replace `<YOUR_TENANT_ID>` in the workflow with your CoreLink tenant slug.

4. Push — the workflow runs on every push and PR.

## Cache hit ratio

After each run, open **Actions → \<run\> → Summary** to see the cache hit ratio table.

## Security (CTRL-CRED-001)

`CORELINK_PAT` is injected as an environment variable and read by the credential helper.
It is **never** passed as a command-line argument (prevents `ps aux` credential leak).

## Buck2

Uncomment the Buck2 step block and comment out the Bazel block. See `examples/buck2-starter`.

## Troubleshooting

Run `corelink doctor` locally to diagnose connectivity issues.
