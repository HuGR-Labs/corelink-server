# Backlog deduplication decisions — 2026-09-22

The expanded lexical pass at `main=3445b217` found 20 additional packages
with a package/manifest-directory/skill-slug token in `BACKLOG.md` but no
direct GitHub issue hit in snapshot R2. Each occurrence was inspected in
context. All 20 are **`distinct`** from an ownership issue: the backlog text is
an implementation finding, source locator, CI/test recipe, deployment note or
architecture reference, not a request for the four ownership deliverables.

| Package | Backlog evidence | Decision | Scope boundary |
|---|---:|---|---|
| `corelink-ac` | 11048, 11066 | `distinct` | storage/region reference |
| `corelink-auth` | 9533, 9559, 9852 | `distinct` | auth jobs and test lanes |
| `corelink-cas` | 5057–5067 | `distinct` | CAS residency finding |
| `corelink-clerk`, `corelink-clerk-cf` | 781, 12971 | `distinct` | Worker/build evidence |
| `corelink-client-verify` | 2920, 3031 | `distinct` | CI/header job |
| `corelink-dual-approval` | 13162, 13169 | `distinct` | endpoint inventory |
| `corelink-enterprise-inquiry` | 13214, 13223 | `distinct` | implementation inventory |
| `corelink-failover-router` | 8401 | `distinct` | source locator |
| `corelink-handler-customer` | 528–532 | `distinct` | Cargo/lockfile defect |
| `corelink-hash` | 6767, 11684 | `distinct` | warning and implementation reference |
| `corelink-privacy` | 3265, 11086–11089 | `distinct` | privacy test/residency work |
| `corelink-privacy-erasure-worker` | 3265, 11091 | `distinct` | worker/backend check |
| `corelink-privacy-pseudonymize` | 11092 | `distinct` | privacy route reference |
| `corelink-region` | 1946 | `distinct` | source-locator test |
| `corelink-slack-real` | 13217 | `distinct` | dependency/implementation note |
| `corelink-slo` | 533, 3172 | `distinct` | feature/sink behavior |
| `corelink-turbo-bridge` | 7042 | `distinct` | source assertion |
| `e2e-pilot-onboarding` | 933–947 | `distinct` | test/clippy evidence |
| `e2e-tenant-isolation` | 302, 966–969 | `distinct` | adversarial test evidence |

The remaining **62** packages have neither a direct issue hit nor one of these
expanded backlog tokens. They still require semantic alias/backlog review; this
partial result does not promote the global backlog gate or authorize issue
publication.
