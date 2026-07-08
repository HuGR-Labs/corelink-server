# OG (Open Graph) Social Preview Images

Three 1280×640 PNG social-preview images for HuGR CoreLink repos.

## Brand tokens

| Token      | Hex       | Use                              |
|------------|-----------|----------------------------------|
| `bg`       | `#0B0F14` | Background (brand dark)          |
| `accent`   | `#22D3EE` | Cyan — accent line / bar         |
| `title`    | `#F8FAFC` | Off-white — title text           |
| `subtitle` | `#94A3B8` | Slate-400 — subtitle / muted     |
| `watermark`| `#475569` | Slate-600 — humangr.com mark     |

## Files

| File                        | Repo                                     | Title              | Subtitle                                           |
|-----------------------------|------------------------------------------|--------------------|----------------------------------------------------|
| `corelink-server.png`       | `HumanGuardrail/corelink-server`           | CoreLink           | REAPI v2 build cache. BYOK. Verifiable audit log.  |
| `corelink-cli.png`          | `HumanGuardrail/corelink-cli`              | corelink CLI       | Bazel · Buck2 · Cargo · npm · OCI                  |
| `corelink-bazel-example.png`| `HumanGuardrail/corelink-bazel-example`    | CoreLink + Bazel   | 5-minute setup walkthrough                         |

## Regen from scratch (macOS)

Requirements:
- `librsvg` (`brew install librsvg`) — provides `rsvg-convert`

```bash
cd marketing/og

# Convert each SVG to PNG at exact 1280×640
rsvg-convert -w 1280 -h 640 --keep-aspect-ratio corelink-server.svg        -o corelink-server.png
rsvg-convert -w 1280 -h 640 --keep-aspect-ratio corelink-cli.svg           -o corelink-cli.png
rsvg-convert -w 1280 -h 640 --keep-aspect-ratio corelink-bazel-example.svg -o corelink-bazel-example.png

# Verify
for f in *.png; do
  echo "$f: $(file "$f" | grep -oE '[0-9]+ x [0-9]+'), $(du -h "$f" | cut -f1)"
done
```

## Uploading to GitHub (manual step)

GitHub does **not** expose a REST or GraphQL API for repository social
preview images. The `PUT /repos/:owner/:repo/og-image` endpoint returns
404. Upload must be done via the Settings UI:

1. https://github.com/HumanGuardrail/corelink-server/settings → **Social preview** → Edit → Upload an image
2. https://github.com/HumanGuardrail/corelink-cli/settings → **Social preview** → Edit → Upload an image
3. `corelink-bazel-example` — repo does not exist yet; upload after creating it

Reference: `specs/_audits/2026-05-27-gh-social-preview-runbook.md` §2.

## Design notes

- Background uses a subtle diagonal gradient from `#0B0F14` → `#111827`
  for depth without being garish.
- A 40px grid pattern (0.5px stroke, `#1a2235`) at 40% opacity adds
  subtle infrastructure texture.
- A single 4px cyan top bar and a vertical left accent bar follow the
  "single accent, not the whole thing" constraint.
- Title font is `system-ui` — no external font CDN; renders consistently
  across platforms without a network call.
- All PNGs are < 200 KB (60–132 KB range).
