# GitHub issue snapshot — 2026-09-22 (R2)

Fresh authenticated readback:

```text
gh issue list --repo HuGR-dev/corelink-server --state all --limit 1000 \
  --json number,url,title,body,state
```

- Issues returned: **262**.
- Open: **81**.
- Closed: **181**.
- Ownership titles (`[ownership]`): **0**.
- Ownership markers (`corelink-ownership:v1:`): **0**.
- Direct package/manifest text hits: **23/105**; the same package set as the
  previous snapshot. The remaining 82 packages have no direct literal hit but
  still require alias/backlog review.

The additional issues since the previous snapshot are ordinary CI, migration,
release, audit and operational work. No issue is an ownership campaign issue.
This snapshot is read-only and does not authorize publication.
