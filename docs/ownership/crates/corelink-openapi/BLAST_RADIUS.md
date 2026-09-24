---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-openapi
manifest: tools/openapi/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: corelink-openapi-source-static-20260921
---

# corelink-openapi — blast radius

Each item is one bounded SOURCE relation from the pinned manifest or Rust text. It is not evidence of generated documents, publication, resolved consumers, compilation, test execution, route serving, or runtime.

[Manifest](#b01) · [YAML input](#b02) · [JSON input](#b03) · [Paths](#b04) · [Parser](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Manifest → crate declaration

`tools/openapi/Cargo.toml` → `corelink-openapi`: the package declaration names the crate and declares `serde`/`serde_json` workspace dependencies. Change impact is limited to this declared static seam. **Evidence:** `Cargo.toml:1-18`. **Failure boundary:** no member selection, dependency resolution, compilation, or artifact exists in this evidence.

<a id="b02"></a>
## B02 — Source → YAML include path

`SPEC_YAML` → `../../../openapi/corelink-v1.yaml`: the public constant uses one `include_str!` path. Altering that exact declaration changes the static input reference. **Evidence:** `tools/openapi/src/lib.rs:44-45`. **Failure boundary:** the relationship does not prove YAML generation, contents, reading, synchronization, validation, publication, or consumption.

<a id="b03"></a>
## B03 — Source → JSON include path

`SPEC_JSON` → `../../../openapi/corelink-v1.json`: the public constant uses one `include_str!` path. Altering that exact declaration changes the static input reference. **Evidence:** `tools/openapi/src/lib.rs:47-49`. **Failure boundary:** the relationship does not prove JSON generation, contents, reading, synchronization, validation, publication, or consumption.

<a id="b04"></a>
## B04 — Path constants → aggregate slice

Named constants in `paths` → `paths::ALL`: the source lists named route constants in the public `&[&str]` aggregate. Altering a constant reference or aggregate membership changes this local source relation. **Evidence:** `tools/openapi/src/lib.rs:68-219`. **Failure boundary:** no external document, registered route, caller, client, or reachable endpoint is established.

<a id="b05"></a>
## B05 — Parser function → serde_json call

`parse_json` → `serde_json::from_str(SPEC_JSON)`: the function returns one named parser call over the local constant. Altering its call or argument changes the declared source flow. **Evidence:** `tools/openapi/src/lib.rs:221-233`. **Failure boundary:** no successful parse, error behavior, invocation, or consumer result is observed.

<a id="b06"></a>
## B06 — Test text → parser and local assertions

`json_parses`, `every_path_constant_is_present_in_spec`, and `package_version_matches_info_version` → `parse_json`: each test body directly calls the parser before asserting on its returned value. Separately, `api_major_version_is_distinct_from_package_semver` → `SPEC_VERSION`, `PACKAGE_VERSION`: that test directly asserts the two local constants. The parser → `SPEC_JSON` relation is recorded only in [B05](#b05). **Evidence:** `tools/openapi/src/lib.rs:235-289`. **Failure boundary:** declarations do not establish test execution, drift detection, CI result, generation, publication, or runtime reachability. The [Worker edge plane](../../../knowledge/planes/worker-edge.md) remains a designated canonical route only.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to manifest](#b01)
