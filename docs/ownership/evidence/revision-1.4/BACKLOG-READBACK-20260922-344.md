# Backlog readback at `main=3445b217` — 2026-09-22

The lexical census was rerun against the actual `BACKLOG.md` blob at
`3445b217906443759092dc6e6306569ed5e354cb`, not the campaign checkout:

- registry population: **105**;
- packages with a literal package/manifest occurrence: **37**;
- packages without a direct literal occurrence: **68**;
- semantic duplicate decisions made: **0**.

The 37/68 partition is identical to `BACKLOG-TEXT-CENSUS-20260922.md`.
This confirms mechanical stability across the latest main drift; it does not
resolve aliases, intent or the required `reuse`/`expand`/`distinct` decisions.
The backlog publication gate remains `PENDING`.
