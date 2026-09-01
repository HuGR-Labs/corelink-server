---
name: built-not-wired
version: 1.0.0
description: Decide whether code that EXISTS in this repo actually RUNS in the shipped artifact, before claiming a capability is missing, present, or broken. Invoke when a feature "does not work" but the code is clearly there, when docs promise something the service returns 501/404 for, when auditing whether a crate/endpoint/flag is reachable, and before writing "X is not implemented". Triggers — "we never built X" / "X is not implemented"; a 501 or 404 on a documented capability; asking whether a crate is reachable; a marketing or docs claim about a capability.
---

# built-not-wired — existing is not shipping

Three different states get reported with the same sentence, "X doesn't work":

| state | what is true | the fix |
|---|---|---|
| **not built** | no implementation exists | write it |
| **built, not wired** | implementation exists, nothing reaches it in the shipped build | enable / connect it |
| **built, wired, broken** | it runs and misbehaves | debug it |

Reporting the second as the first is the common failure. It throws away work that
already exists and points the plan at the wrong task.

## Reachability is a property of the TARGET, not of the workspace

This repo has **19 `[target.'cfg(...)'.dependencies]` blocks**. Parsing
`Cargo.toml` textually is blind to all of them and **invents edges no build has**.

| measurement | answer | population it measures |
|---|---|---|
| textual `Cargo.toml` parse | reachable in 6 hops | **every declared edge** for no particular package/binary/build event, no target |
| `cargo tree -p <bin> … --target all` | reachable in 6 hops | **union of all platforms** for package `<bin>`, not one shipped build event |
| `cargo tree -p <bin> … --target <shipped-target>` | **not reachable** | ✅ reverse graph for binary `<bin>`, package `<bin>`, and the named image/CI build event |

Three correct answers, two irrelevant, zero warnings.

**For every Cargo query, pass an explicit `--target`** — the one the shipped
artifact uses. Note that `Dockerfile` reaches `x86_64-unknown-linux-gnu` **by the
absence of `--target`** (it builds for the host), so grepping the Dockerfile for
`--target` finds nothing and invites the wrong conclusion. Worker/Wrangler and
CI checks identify their runtime target from their entrypoint/build event; do
not invent a Cargo target for those non-Cargo measurements.

Two traps in `cargo tree` itself:

1. **`warning: nothing to print` is a RESULT, not an error.** When the target is
   right, that *is* the answer to "does it reach". It reads like a failed query.
2. **Cargo suggests `--target all` right underneath** — and `--target all` returns
   a path that exists in **no** build. The canonical tool pushes you toward the
   misleading output exactly when the correct one looks empty.

`cargo tree` does not compile; it resolves metadata in seconds. *"No build"* and
*"no cargo"* are not the same constraint.

## Select the shipped target before checking reachability

Use exactly one branch below, based on the artifact that is actually shipped;
never substitute the workspace, `--target all`, or a convenient host build for
that artifact. Record the target and the population in the conclusion.

### Cargo/container artifact

1. Identify the final image stage and its build command in `Dockerfile` or the
   release script. If no `--target` is passed, record the host target used by
   that build; absence of a flag is an explicit host-target decision, not an
   unknown target. Also record the Cargo package and binary, image tag/digest,
   and build event that produced the shipped image.
2. Run `cargo tree -p <bin> -i <crate> --target <that-target>` and, where useful,
   `cargo metadata --filter-platform <that-target>`. Read the complete output.
3. The population is the selected Cargo package/binary's dependency graph for
   that target and image build event, not every declared manifest edge. Unknown
   target, package, binary, build event, command error, truncated
   output, or an empty/unparsed package inventory is **HALT**. `warning: nothing
   to print` with a successful, complete query means “not reachable” for that
   target; it is not permission to retry with `--target all`.

### Worker/Wrangler artifact

1. Identify `wrangler.toml`/`wrangler.json`, the package deploy script, the
   `main` entrypoint, and any Rust build command. Record the runtime target
   (`wasm32-unknown-unknown` for a Rust Worker, or the generated JS bundle for a
   JS Worker), package/script, deployment event, and exact output artifact.
2. Trace the entrypoint's imports/exports and bindings into the generated
   Wrangler bundle (a dry-run/build inspection is preferable to a live deploy),
   then verify the route or binding reaches the implementation. For Rust, use
   `cargo tree -p <worker> -i <crate> --target wasm32-unknown-unknown`.
3. The population is the complete entrypoint/bundle and binding set for this
   package's Wrangler deployment event, not all source files or all environments.
   Missing
   entrypoint, empty bundle, truncated build output, or an unparsed binding is
   **HALT**; do not call the feature “not wired”.

### CI/workflow artifact

1. Select the workflow and the job that creates or deploys the artifact. Read its
   full `on:`, checkout ref, matrix, `runs-on`, build command, and upload/deploy
   step. Derive the shipped target from those steps (including any explicit
   Cargo target, container platform, or Wrangler command), not from the runner
   label alone.
2. Trace the exact checkout and build inputs to the artifact. A scheduled check
   that runs successfully is not evidence that a missing PR/push lane or a
   different workflow ships the capability.
3. The population is the complete jobs/steps and artifact paths of this named
   workflow, package/binary (if applicable), event, and target. Empty, truncated,
   or partly unparsed YAML is **HALT**. Qualify the conclusion with workflow,
   event, job, target, build command, and artifact; never generalize it to “CI”
   or “the repo”.

## Feature flags: what is compiled in is decided at image build time

A cloud service delivered as a container image has "which implementation is
compiled in" frozen in the image. Check, in order:

1. **`Dockerfile`** — does the build line pass `--features` at all? With no
   `--features`, `default = []` selects the package's default implementation
   set. Tie the exact crate/provider, package/binary, image tag/digest, and build
   event to the conclusion; do not generalize from a workspace default.
2. **The crate's `[features]`** — which flags exist, and are they mutually
   exclusive (a `compile_error!` guard)? Read *what the guard actually checks* —
   a guard over the **public** features may leave internal flags combinable.
3. **The construction site** — `#[cfg(feature = ...)]` around where the real type
   is built is usually the true limiter, more than the guard.

## Do not mistake a LABEL for the DISPATCH

The most expensive misread of this class: a function named for the active
implementation may exist only to *report* it.

Measured case: `active_provider()` is a `const fn` returning one enum value,
selected by `#[cfg]`. It was read as "which provider serves this tenant". Its
actual call sites were `/healthz`, a metrics label, and a log field. The real
dispatch was `Arc<dyn Provider>` — already dynamic, already type-erased, and a
sibling module already held a `Vec<Arc<dyn Provider>>`, several at once, at
runtime. The data schema was already keyed per tenant per provider.

The conclusion drawn — "single-provider architecture, needs a rewrite" — was
wrong. The truth was "four implementations built, one compiled in, dispatch layer
already dynamic."

**Before concluding from a symbol's name: read its call sites.** If they are all
health, metrics, and logging, it is a label. Find where the trait object is
constructed — that is the dispatch.

## Checklist before writing "X is not implemented"

- [ ] Grep for the implementation — count the lines, name the files.
- [ ] Is there a common trait, and does anything return `Box<dyn T>` / `Arc<dyn T>`?
- [ ] Select the Cargo/container, Worker/Wrangler, or CI/workflow branch above.
- [ ] `cargo tree -p <bin> -i <crate> --target <shipped target>` where that branch applies.
- [ ] Does the `Dockerfile` / release build pass the feature that compiles it in?
- [ ] Does the data schema already model it? (A table with the right columns is
      strong evidence the design anticipated it.)
- [ ] Are the call sites of the "selector" function actually dispatch, or reporting?
- [ ] Is the endpoint registered in the router, or only defined?

Answer these and write a qualified conclusion: “built, not enabled in
`<artifact>`, target `<target>`, population `<set>`; here is the one line that
enables it.” If the target or population is unknown, truncated, empty, or
unparsed, report **HALT: INSTRUMENT BROKEN**, not “not implemented”.

## Campaign routing examples

- **B-081:** on the pre-fix baseline, use the Worker/Wrangler branch for the
  deployed DO revision and deployment event, then trace its
  `container.start({ env })` payload into the named container package/binary and
  image. The conditional claim is “transition-key forwarding is built in that
  container but not wired in that Worker revision”; it is not a claim about all
  deployments and not “PAT rotation is not implemented”. Re-check after the
  forwarding repair or secret-binding change.
- **B-133:** use the CI/workflow branch for the named `dependabot-policy` job and
  its `pull_request_target` event. The population is its executable checkout and
  `run:` steps; comments elsewhere in the repository do not count. State whether
  the PR merge ref or a base-ref script is executed for this workflow only.
- **B-167:** route the backlog-ID question to `verify-population` and
  `teeth-test`; inspect the named CI/workflow only if the claim is about whether
  that checker ships. The population is all parsed fenced `backlog` blocks, and
  a zero/truncated/unparsed parse is **HALT**, not proof that all IDs are wired.

## Related

`.claude/skills/verify-population/` — same root cause: a correct reading of the
wrong set.
