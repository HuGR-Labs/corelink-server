# SEAL — W35-P2-ADAPTER-HOST Absorption

**Branch:** `w35-p2-adapter-host`
**Base:** `ebaabf1a2edf262f9e9163a1aea72d6325e03e84` (Wave 32 Phase B+ SEALED)
**Date:** 2026-05-26
**Scope:** Wave 35 Phase 2 closure of Wave-33/34 follow-up §4 — physical
consolidation of the five Wave-34 adapter crates into the
`corelink-adapter-host` umbrella crate as inline `mod` submodules.

---

## 0 — Mandate (verbatim)

> Per `specs/_audits/2026-05-26-wave-33-34-closure-followups.md` §4: absorb
> 5 Wave-34 adapter crates into `corelink-adapter-host` via inline `mod X;`.

Absorbed crates:

| # | Absorbed                  | LOC   | Tests | Shim                                              |
| - | ------------------------- | ----- | ----- | ------------------------------------------------- |
| 1 | `corelink-adapter-brew`   | 1,157 | 20    | `crates/corelink-adapter-host/src/brew.rs`        |
| 2 | `corelink-adapter-cargo`  | 1,028 | 28    | `crates/corelink-adapter-host/src/cargo.rs`       |
| 3 | `corelink-adapter-npm`    | 1,801 | 46    | `crates/corelink-adapter-host/src/npm.rs`         |
| 4 | `corelink-adapter-oci`    | 3,551 | 45    | `crates/corelink-adapter-host/src/oci.rs`         |
| 5 | `corelink-adapter-pip`    | 2,336 | 53    | `crates/corelink-adapter-host/src/pip.rs`         |
|   | **Total**                 | **9,873** | **192** |                                          |

---

## 1 — Architecture (post-absorption)

Each absorbed adapter now lives as a submodule of
`corelink-adapter-host`:

```
crates/corelink-adapter-host/
├── Cargo.toml                       # superset of all absorbed deps
├── src/
│   ├── lib.rs                       # pub mod { brew, cargo, npm, oci, pip };
│   ├── brew.rs                      # absorbed brew lib.rs + pub mod bridge;
│   ├── brew/
│   │   ├── audit.rs auth.rs bottle.rs config.rs error.rs
│   │   ├── ports.rs server.rs upstream.rs
│   │   └── bridge.rs                # BrewCasBridge, BrewTenantBridge
│   ├── cargo.rs                     # absorbed cargo lib.rs
│   ├── cargo/
│   │   ├── audit.rs auth.rs config.rs error.rs
│   │   ├── ports.rs server.rs translate.rs
│   │   └── bridge.rs                # CargoCasBridge, CargoTenantBridge
│   ├── npm.rs                       # absorbed npm lib.rs
│   ├── npm/
│   │   ├── audit.rs auth.rs config.rs error.rs metadata.rs
│   │   ├── ports.rs server.rs tarball.rs upstream.rs
│   │   └── bridge.rs                # NpmCasBridge, NpmKvBridge, NpmTenantBridge
│   ├── oci.rs                       # absorbed oci lib.rs + run_oci_adapter
│   ├── oci/
│   │   ├── audit.rs auth.rs config.rs digest.rs error.rs
│   │   ├── ports.rs tags.rs
│   │   ├── pull.rs pull/{blob.rs, manifest.rs}
│   │   ├── push.rs push/{manifest.rs, upload.rs}
│   │   ├── server.rs server/{core.rs, dispatch.rs, handlers.rs}
│   │   └── bridge.rs                # OciBlobBridge, OciManifestKvBridge, OciTenantBridge
│   ├── pip.rs                       # absorbed pip lib.rs
│   └── pip/
│       ├── audit.rs auth.rs config.rs error.rs index.rs
│       ├── pep503_html.rs ports.rs server.rs upstream.rs wheel.rs
│       └── bridge.rs                # PipCasBridge, PipKvBridge, PipTenantBridge
└── tests/
    ├── brew_{adversarial,common,prop_url_normalize,smoke}.rs
    ├── cargo_{adversarial,common,prop_translate,smoke}.rs
    ├── npm_{adversarial,common,prop_metadata,smoke}.rs
    ├── oci_{adversarial,common,prop_digest,prop_manifest,smoke_pull,smoke_push}.rs
    └── pip_{adversarial,prop_index_parse,smoke}.rs
```

**Public surface preserved** — each `<mod>.rs` parent re-exports the
absorbed crate's prior `pub use config::*Config; pub use error::*Error;
pub use server::run_*_adapter;` AND the new `bridge` submodule's bridge
types. External callers that previously imported
`corelink_adapter_<mod>::BrewAdapterConfig` now import
`corelink_adapter_host::brew::BrewAdapterConfig` (zero call sites
existed outside the host crate per pre-absorption grep — see
parallel-safety §3).

---

## 2 — Mechanical changes

### 2.1 — File moves (per absorbed crate)

For each `<abs>` in `{brew, cargo, npm, oci, pip}`:

1. `git mv crates/<abs>/src/lib.rs` → DELETED (its `pub mod` declarations
   were rewritten into the new `crates/corelink-adapter-host/src/<mod>.rs`
   parent file; module map + crate-doc preserved).
2. `git mv crates/<abs>/src/*.rs` → `crates/corelink-adapter-host/src/<mod>/`
   (preserving sub-directories `pull/`, `push/`, `server/` for OCI).
3. `git mv crates/<abs>/tests/<f>.rs` →
   `crates/corelink-adapter-host/tests/<mod>_<f>.rs` (prefixed to avoid
   name collisions across siblings).
4. Inside absorbed source files: `crate::<m>` → `crate::<mod>::<m>` for
   every cross-module reference (audit, auth, config, digest, error,
   index, metadata, pep503_html, ports, pull, push, server, tags,
   tarball, translate, upstream, wheel).
5. Inside absorbed test files: `corelink_adapter_<mod>::` →
   `corelink_adapter_host::<mod>::`.
6. `mod common;` (in tests that had a shared `common.rs`) →
   `#[path = "<mod>_common.rs"] mod common;`.

### 2.2 — New bridge submodules

Each `crates/corelink-adapter-host/src/<mod>/bridge.rs` was created from
the corresponding pre-absorption shim file
`crates/corelink-adapter-host/src/<mod>.rs` (which contained
`<MOD>CasBridge`, `<MOD>KvBridge<K>` where applicable, and
`<MOD>TenantBridge`). The single semantic change is
`use corelink_adapter_<mod>::ports::…;` → `use super::ports::…;` so the
bridge resolves the port traits inside the absorbed submodule rather
than across a former external crate boundary.

### 2.3 — `crates/corelink-adapter-host/Cargo.toml`

Removed five `corelink-adapter-{brew,cargo,npm,oci,pip}` workspace
dependencies. Added the deps the absorbed crates depended on (union of
their five Cargo.toml `[dependencies]` sections):

- HTTP server stack: `axum`, `tower`, `http`, `http-body-util`
- Upstream clients: `reqwest`, `url` (brew/npm/pip)
- Hash + crypto: `blake3` (brew), `sha2`, `hmac` (oci),
  `subtle` (constant-time PAT compare; all five)
- Encoding: `hex`, `base64` (oci)
- Concurrency: `parking_lot` (oci port internals),
  `futures` (oci streaming)
- Secrets: `secrecy = "0.10"`
- Errors/Tracing/Serde: `thiserror`, `tracing`, `serde`, `serde_json`,
  `uuid`

`[dev-dependencies]` superset: `tokio-test`, `proptest`, `tower(util)`,
`wiremock`, `corelink-replication`, `corelink-core`, `corelink-audit`,
`async-trait`, `subtle`, `reqwest(json+rustls-tls,
default-features=false)`, `uuid`, `hex`, `http-body-util`, `axum`,
`http`, `bytes`.

The pre-existing strict lint block at the adapter-host crate root
(`unsafe_code=forbid`, `missing_docs=deny`,
`missing_debug_implementations=deny`, `unwrap_used=deny`,
`expect_used=deny`, `panic=deny`, `indexing_slicing=deny`, `todo=deny`,
`unimplemented=deny`, `dbg_macro=deny`, `print_stdout=deny`,
`print_stderr=deny`, `mod_module_files=deny`) is preserved and applies
to all absorbed code (this is why every parent module uses
`<mod>.rs`+`<mod>/` rather than `mod.rs`).

### 2.4 — ROOT `Cargo.toml`

- `workspace.members`: removed `crates/corelink-adapter-{brew,cargo,
  npm,oci,pip}` (5 entries).
- `workspace.dependencies`: removed `corelink-adapter-{brew,cargo,
  npm,oci,pip} = { path = … }` (5 entries).
- The pre-absorption comment block referencing the five Wave-34
  adapters was replaced with a single Wave-35 Phase 2 comment block
  referencing the consolidated `corelink-adapter-host` umbrella.
- No other workspace-level changes (sole `Cargo.toml` conflict surface,
  per parallel-safety §6).

---

## 3 — Parallel-safety

**Conflict surface verified pre-absorption** —
`grep -rn "corelink-adapter-\\(brew\\|cargo\\|npm\\|oci\\|pip\\)"
--include=*.rs --include=*.toml` over the entire repo found **zero**
hits outside `crates/corelink-adapter-{brew,cargo,npm,oci,pip}/` and
`crates/corelink-adapter-host/`. The ROOT `Cargo.toml` had the only
references (members + workspace.dependencies), which the orchestrator
union-merges.

**No W36 zones touched** — `crates/corelink-adapter-host/{src,tests}/`,
`crates/corelink-adapter-{brew,cargo,npm,oci,pip}/` (now deleted), and
ROOT `Cargo.toml` (workspace.members + workspace.dependencies blocks
only).

---

## 4 — Charter invariants preserved

| Invariant                              | Where                                                              |
| -------------------------------------- | ------------------------------------------------------------------ |
| `#![forbid(unsafe_code)]`              | adapter-host crate root (applies to all absorbed code)             |
| `#[non_exhaustive]` on every pub type  | preserved verbatim from absorbed sources                           |
| `subtle::ConstantTimeEq` for PAT       | preserved in `<mod>/auth.rs` for each adapter                      |
| `SecretWrap` / `SecretString` for PATs | preserved in `<mod>/auth.rs` + bridges                             |
| INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER     | preserved in `<mod>/audit.rs` + call sites in `server.rs`/`bottle.rs`/`tarball.rs`/`wheel.rs`/`push/upload.rs` |
| INV-NO-BODY-IN-LOGS                    | preserved — no `tracing` spans were edited                         |
| INV-NO-PII-IN-LOGS                     | preserved — no `tracing` spans were edited                         |
| ADR-0015 inline-ports pattern          | preserved — each `<mod>/ports.rs` retains adapter-local trait surface (`CasStore`, `KvStore`, `TenantResolver`, `BlobStore`, `ManifestKvStore`) |
| L2.10 file-size HARD CAP 500 LOC       | preserved — no source file exceeds the cap (the absorption didn't combine files) |
| Declared-digest fail-CLOSED on OCI PUT | preserved in `oci/push/upload.rs`                                  |
| Pre-CAS-store integrity check (npm)    | preserved in `npm/tarball.rs`                                      |
| Pre-CAS-store integrity check (pip)    | preserved in `pip/wheel.rs`                                        |

---

## 5 — Acceptance results

```
$ cargo build -p corelink-adapter-host
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.60s

$ cargo clippy -p corelink-adapter-host --tests -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 49s
  (no warnings, no errors)

$ cargo test -p corelink-adapter-host
  passed: 271  failed: 0  ignored: 0
```

**Test count delta:** baseline 44 → 271 ( +227 from absorbed adapters'
unit + integration tests; the 192 figure from the dispatch packet
counted only the unit `#[cfg(test)] mod tests` blocks inside each
adapter's `src/*.rs`; integration tests under `tests/*.rs` added the
remaining 35 over the spec target of ≥236).

**Conflict-marker sweep:** clean —
`grep -rEn "<<<<<<<|>>>>>>>" --include=*.{toml,rs,md}
crates/corelink-adapter-host Cargo.toml` returned zero matches.

**Absorbed-crate-dir absence:** all five
`crates/corelink-adapter-{brew,cargo,npm,oci,pip}` directories are
removed from the working tree.

---

## 6 — Wave-34 closure status

This SEAL physically closes Wave-34's "5 single-consumer adapter
crates" follow-up by collapsing those crates into the
`corelink-adapter-host` umbrella that Wave 35 prep
(`9d0f4284`) had already bridged via shim files. The umbrella now
contains both the absorbed adapter business logic and the bridge
types that map the adapter-local ports onto the canonical workspace
SPI traits in a single, locally-consistent crate.

Follow-up #4 in
`specs/_audits/2026-05-26-wave-33-34-closure-followups.md §4` is
**CLOSED** by this SEAL.

---

## 7 — DO-NOT verification

- ✅ No `--no-verify` flags used.
- ✅ No `#[allow(…)]` added beyond the test-scope `clippy::unwrap_used`
  / `clippy::expect_used` / `clippy::panic` blocks that already existed
  in the absorbed source files.
- ✅ No `merge` or `push` commands run.
- ✅ No workspace-wide `cargo build` triggered — only
  `cargo {build,test,clippy} -p corelink-adapter-host`.
- ✅ No W36 zones touched.
- ✅ Inline-ports pattern preserved verbatim (all five `<mod>/ports.rs`
  files are byte-for-byte identical to their pre-absorption
  predecessors, modulo the `crate::*` → `crate::<mod>::*` rewrite).

---

## 8 — Commit

Single commit on branch `w35-p2-adapter-host` (per §9 of the
dispatch packet):

```
seal(w35-p2-adapter-host): absorb 5 Wave-34 adapters into corelink-adapter-host
```
