### Fixed

- **Two TypeScript coverage floors existed on paper and had never once executed
  (WP-3).** `worker/vitest.config.mts` has carried a `coverage.thresholds` block
  since the file was written, but `.github/workflows/worker-vitest.yml` ran
  `npx vitest run` with **no `--coverage`** — and vitest evaluates `thresholds`
  ONLY when the coverage provider is enabled. The block was inert config, not a
  gate. Proof of what that cost: measured on `origin/main`, `src/index.ts` was
  written down as 90% lines and actually sat at **77.13%**. It rotted 13 points
  and nothing went red, because nothing was ever measured. Both PR lanes now run
  `vitest run --coverage`, and every threshold is re-baselined against a measured
  run and set BELOW it with slack, so the gate is green today and red on the next
  regression — a floor above reality is an ignored red check, not rigor, and this
  repo already carries five workflows with 1,057 lifetime runs and zero
  successes. `worker/` total 84.97% lines -> floor 80; `apps/signup-worker/`
  gains a floor for the first time at 79.09% lines -> 74. Net rigor goes UP: the
  per-file floors TIGHTEN where they were too loose to catch anything
  (`durable_object.ts` 35 -> 76 against a measured 81.69%;
  `rollout_controller.ts` 0 -> 90 against a measured 100%, since a `0` threshold
  cannot fail; global lines 65 -> 80), and the one value that drops —
  `index.ts` 90 -> 72 — was enforced zero times in its life, so it moves from
  unenforced-and-unmeetable to enforced-every-PR. That gap is recorded test debt,
  to be repaid by ADDING tests, never by editing the number again; no test was
  deleted or weakened. Both configs also set `coverage.include` EXPLICITLY,
  because v8 and istanbul default to reporting only files a test happened to
  import — under that default a newly added untested source file leaves the
  percentage untouched and the floor is blind to it (measured on signup-worker:
  88.23% imported-files-only vs 79.09% across all of `src/`, ~9 points the
  default could not see). All states proven with BARE exit codes, never through
  a pipe: green at the chosen floor (0), red when the floor is breached (1), and
  a newly added untested file moves the total (79.09% -> 78.86%). Rust is
  deliberately untouched: `coverage.yml` is `workflow_dispatch`-only with its
  cron commented out, and per the standing rule a heavy instrumented workspace
  build does not belong on a per-PR lane on the self-hosted Mac fleet.
