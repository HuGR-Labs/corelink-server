# B-315: serialize dense BACKLOG id allocation at merge time

The mandatory merge authority now holds a crash-safe process lock while it
allocates and revalidates dense `BACKLOG.md` ids. It captures exact base/main/head
bytes, creates a signed DCO merge commit with parent order `(base, head)` and the
candidate head tree, and performs a normal non-force push to `refs/heads/main`.
The server therefore rejects a moved main atomically. A unique create-only remote
lease and owner-safe conditional release serialize authorities; stale leases fail
closed with a manual recovery packet. The gate polls GitHub's `MERGED` state and
verifies the resulting main OID/tree before making any merged claim.
The repository permits merge commits; branch-protection/ruleset enforcement is
unavailable on this GitHub plan (API returns 403), so this gate is the mandatory
cooperating authority. It cannot stop an out-of-policy administrator/UI merge.
