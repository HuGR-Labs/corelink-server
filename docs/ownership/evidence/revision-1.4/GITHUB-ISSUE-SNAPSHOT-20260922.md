# GitHub issue snapshot — 2026-09-22

Read-only snapshot from the authenticated `gh` session against the canonical
remote `HuGR-dev/corelink-server`:

```text
gh issue list --repo HuGR-dev/corelink-server --state all --limit 1000 --json number,url,title,body,state
```

- Issues returned: **250**.
- Open: **66**.
- Closed: **184**.
- Ownership titles (`[ownership]`): **0**.
- Ownership markers (`corelink-ownership:v1:`): **0**.
- Pull requests were not treated as issues by `gh issue list`.

A package/manifest text scan found direct name or manifest hits for **23/105**
packages. Those hits are not automatically ownership duplicates: they include
broader migration, audit, CI and implementation issues and require an explicit
`reuse`, `expand` or `distinct` decision. The remaining 82 packages had no
direct name/manifest hit in this snapshot, but still require the canonical
backlog check before any publication gate can pass.

This snapshot proves only read completeness of the issue listing at capture
time and absence of ownership markers. It does not authorize issue creation,
freeze the standard, or resolve package-specific backlog decisions.
