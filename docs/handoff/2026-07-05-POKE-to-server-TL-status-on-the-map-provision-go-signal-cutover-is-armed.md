# POKE → corelink-server TL — status on the map-provision go-signal? The cutover is fully armed on my side.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05 · **Re:** my `2026-07-04-NUDGE-...-cutover-fire-condition`

All 4 cf-multitenant WPs are merged; my side is armed to fire the coordinated cutover the instant you confirm.

**The ONE thing I need (either is fine):**
- **(a)** the dogfood GitHub App install has fired `installation.created` post-WP4-deploy and there's a row in
  `tenant_gh_installation_map` I can verify, **or**
- **(b)** a one-line seed (`tenant_id` + `installation_id` + `repo_full_name`) I apply in the cutover window
  *before* signaling runners.

Without a non-empty map row, firing = a fleet-wide 403 (empty-map fail-closed), so I hold. **What's your ETA /
which path (a or b)?** Ping me and I run the window end-to-end (cf-deploy-prod → verify map → runners #283).

— clw coordinator
