### Fixed

- **The Stripe/Clerk/DSR worker deployed from an unpinned `npm install` (B-090).**
  `signup-worker-deploy.yml` ran `npm install --legacy-peer-deps`: the whole tree
  re-resolved on every deploy, every transitive `postinstall` executing, on a
  self-hosted runner that carries the Cloudflare deploy credential. Both lanes —
  deploy and vitest — now run
  `pnpm install --frozen-lockfile --filter @corelink/signup-worker... --ignore-scripts`,
  matching `admin-ui-deploy.yml:93` and `cf-deploy-prod.yml:307`.

  `--legacy-peer-deps` is gone with its cause: it existed only because a fresh
  npm install floated wrangler ≥4.108, whose OPTIONAL peer on
  `workers-types@^5` ERESOLVEd against this worker's pinned 4.x. A frozen install
  does not float.

  Measured: install completes in 5s (`Lockfile is up to date`),
  `wrangler deploy --dry-run` bundles 545.21 KiB with all six bindings resolved,
  and the vitest suite passes **250/250 across 15 files** with the coverage floor
  satisfied.

- **`scripts/check_signup_worker_pin.py`** (new) asserts the pin is real rather
  than merely present: every `dependency`/`devDependency` in the worker's
  package.json must carry a `specifier:` under the `apps/signup-worker` importer
  in `pnpm-lock.yaml`. Parses YAML/JSON instead of matching regexes against the
  lockfile. Currently 6/6.

### Notes

- **B-090's prescribed repair was based on an incomplete premise, and was not
  applied.** The item says *"`npm ci` cannot run — the lockfile is gitignored"*
  and concludes: commit a `package-lock.json` as an exception to `.gitignore:95`.
  True of the *npm* lockfile — and it hid the thing that matters: **`apps/signup-worker`
  is already a pnpm-workspace member with a pinned, committed entry in
  `pnpm-lock.yaml`** (importer at line 349, all five specifiers matching
  package.json). A pinned lockfile existed the whole time; this lane just did not
  use it. Committing a `package-lock.json` would have added a *second, divergent*
  lockfile for a package that already has one, against `.gitignore:93`'s own text
  ("this repo uses pnpm ... never commit package-lock.json").

- **This also closes the half B-090 called undecided.** What ships is now the same
  pnpm graph `pnpm-audit.yml` scans, instead of an npm-resolved tree no gate ever
  saw. No tooling decision was needed — the unification was already committed.

- **Recorded, deliberately not fixed here:** the deploy lane's
  `npm install -g wrangler@4.95.0` diverges from the lockfile's pinned
  `wrangler 4.111.0`. Changing which wrangler performs the deploy is a behaviour
  change on the money path and deserves its own item. That step does not carry the
  Cloudflare credential — it lives in the `Deploy Worker` step's `env:`.
