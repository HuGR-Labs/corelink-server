TB6 | CONFIRMED | worker/src/event_log_do.ts:277 | handleAppend awaits put(entryKey(seq), entry) then put(HEAD_KEY, seq) serially; one multi-key put would be atomic/faster.
TB7 | CONFIRMED | worker/src/lib/tenant_suspend_gate.ts:87 | Separate prefixes: tsusp: (suspend_gate:87), tres: (residency_cache:62), ttier: (tier_cache:72); no tmeta: key exists.
TB8 | CONFIRMED | worker/src/lib/runner_mint.ts:454 | Offboarding (:454), allowlist (:464), entitlement (:478) awaited serially; no CONFIG_DB.batch call in file.
TB10 | CONFIRMED | worker/src/replication_coordinator_do.ts:474 | alarm()'s finally re-arms +30s unconditionally forever; stale-primary ticks warn replication_no_eligible_replica every tick (:496), unsuppressed.
