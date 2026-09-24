# Registry readback — 2026-09-22 (`main=3445b217`)

The registry/index were regenerated after the `corelink-hash` ledger/blast
reconciliation:

```text
python3 docs/ownership/tools/generate_registry.py \
  --root . --observed-main 3445b217906443759092dc6e6306569ed5e354cb \
  --json-output docs/ownership/registry.json \
  --markdown-output docs/ownership/index.md
```

- Population: **105**.
- Structural: **105 PASS / 0 FAIL**.
- Artifact integrity: **105 PASS / 0 FAIL**.
- Cold review: **105 UNVERIFIED**.
- Publication: **0**.

The regenerated registry captures the current hash blast bytes and remains a
mechanical readback only; it does not promote any review, freeze the standard
or authorize issue creation.
