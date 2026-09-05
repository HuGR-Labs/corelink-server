### B249 — Dependency-Track patched-locally sunset clock

Replaced the D01 test's stale fixed date with an injected clock while preserving
production sunset semantics. Added explicit day 90 suppression and day 91 alert
tests plus comment-safe mutation coverage; no production alert policy was
weakened.
