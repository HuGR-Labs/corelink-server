TB1 | CONFIRMED | worker/src/index.ts:1759 | for-loop awaits verifyPatHmac per key serially; Promise.all absent
TB2 | CONFIRMED | worker/src/lib/pat_verify_cache.ts:356 | all three caches call bare kv.get(key); no type/cacheTtl options passed
TB3 | CONFIRMED | worker/src/lib/clerk_auth.ts:298 | owner tenant query falls back to serial team_member query; mergeable via UNION ALL
