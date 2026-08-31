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
| textual `Cargo.toml` parse | reachable in 6 hops | **every declared edge**, no target |
| `cargo tree … --target all` | reachable in 6 hops | **union of all platforms** |
| `cargo tree … --target x86_64-unknown-linux-gnu` | **not reachable** | ✅ the binary we ship |

Three correct answers, two irrelevant, zero warnings.

**Always pass an explicit `--target`** — the one the shipped artifact uses. Note
that `Dockerfile` reaches `x86_64-unknown-linux-gnu` **by the absence of
`--target`** (it builds for the host), so grepping the Dockerfile for `--target`
finds nothing and invites the wrong conclusion.

Two traps in `cargo tree` itself:

1. **`warning: nothing to print` is a RESULT, not an error.** When the target is
   right, that *is* the answer to "does it reach". It reads like a failed query.
2. **Cargo suggests `--target all` right underneath** — and `--target all` returns
   a path that exists in **no** build. The canonical tool pushes you toward the
   misleading output exactly when the correct one looks empty.

`cargo tree` does not compile; it resolves metadata in seconds. *"No build"* and
*"no cargo"* are not the same constraint.

## Feature flags: what is compiled in is decided at image build time

A cloud service delivered as a container image has "which implementation is
compiled in" frozen in the image. Check, in order:

1. **`Dockerfile`** — does the build line pass `--features` at all? With no
   `--features`, `default = []` wins and only the fake/stub compiles.
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
- [ ] `cargo tree -p <bin> -i <crate> --target <shipped target>`.
- [ ] Does the `Dockerfile` / release build pass the feature that compiles it in?
- [ ] Does the data schema already model it? (A table with the right columns is
      strong evidence the design anticipated it.)
- [ ] Are the call sites of the "selector" function actually dispatch, or reporting?
- [ ] Is the endpoint registered in the router, or only defined?

Answer these and the report becomes actionable — "built, not enabled; here is the
one line that enables it" instead of "not implemented; here is a rewrite."

## Related

`.claude/skills/verify-population/` — same root cause: a correct reading of the
wrong set.
