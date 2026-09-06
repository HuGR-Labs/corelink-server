# B-255 cross-tenant handler TLC evidence — 2026-09-06

This is the bounded, machine-readable dispatch record for
`INV-CROSS-TENANT-DENIED`. The repository convention for retained TLC dispatch
evidence is a Markdown audit under `specs/_audits/sealed/`; the JSON block is
the load-bearing record consumed by `verify_b255_cross_tenant_denied.py`.

```json
{
  "schema": "corelink.tla-verification-evidence.v1",
  "invariant": "INV-CROSS-TENANT-DENIED",
  "model": {
    "path": "specs/tla/cross_tenant_handler_audit.tla",
    "sha256": "b3bd84a1a0f393be274d9d6d9f016fc8a2b19dd6df4df86cc30e87544606ce45"
  },
  "config": {
    "path": "specs/tla/cross_tenant_handler_audit.cfg",
    "sha256": "58759cf2dda809f8afbfa0eafb6036a24df681680c6878bd1a5416186c3e52b2"
  },
  "tool": {
    "name": "tla2tools.jar",
    "version": "1.8.0",
    "sha256": "eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a",
    "size_bytes": 4487757,
    "pin_source": "specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md §A1"
  },
  "command": "TLC_JAR=/path/to/tla2tools.jar bash scripts/run_tla_suite.sh --only cross_tenant_handler_audit --timeout 60 --workers 1",
  "bounds": {
    "Tenants": ["tenant_a", "tenant_b"],
    "Groups": ["overview", "usage", "billing", "keys", "team", "audit_query"],
    "MaxOps": 6
  },
  "result": {
    "status": "PASS",
    "states_generated": 134,
    "distinct_states": 134,
    "depth": 7,
    "invariants": [
      "InvAuditBeforeCrossTenantDenied",
      "InvCrossTenantReturnHasDistinctTenants",
      "InvAllSixGroupsCovered",
      "InvAuditFailureNeverReturnsCrossTenantDenied"
    ]
  }
}
```

The canonical runner was executed with the pinned jar and reported one
verified spec, zero failures. The direct TLC cross-check reported 134 generated
and distinct states at depth 7; both healthy and failed-audit initial states
are reachable in this bounded model.
