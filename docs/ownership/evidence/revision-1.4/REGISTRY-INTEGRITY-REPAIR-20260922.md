# Registry integrity repair — 2026-09-22

The 37 artifact-integrity failures reported by the prior registry readback were
repaired mechanically. The repair did not change document bodies or claims; it
normalized frontmatter metadata so every package's skill, reference, blast
radius and maintenance artifacts agree on the reference package, manifest,
source commit and evidence set.

## Scope

- Packages repaired: **37**.
- Artifact frontmatters changed: **63**.
- Metadata fields changed: package/manifest presence in skills, canonical
  `source-commit`/`evidence-set` skill keys, and cross-artifact
  `source_commit`/`evidence_set` values.
- Document bodies changed: **0**.
- Cargo, source, runtime and GitHub state changed: **0**.

The canonical values were taken from each package's `REFERENCE.md`; no source
commit or evidence set was invented. The registry was regenerated with:

```text
python3 docs/ownership/tools/generate_registry.py --root . --observed-main 0d1e85792bbe1b495bc8273ee63011417510140f --json-output docs/ownership/registry.json --markdown-output docs/ownership/index.md
```

## Result

- Population: **105**.
- Artifact integrity: **105 PASS / 0 FAIL**.
- Cold review: **105 UNVERIFIED**.
- Publication: **0**.
- Structural PASS is not semantic approval and does not authorize publication.

Because the 63 changed bytes are confined to administrative frontmatter, the
semantic body reviews remain usable under the metadata-only scope amendment;
identity/source/evidence readback is required. Fresh full cold review remains
required for semantic body changes and before final approval/publication. This
repair closes only the mechanical integrity gate.
