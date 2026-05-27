# GitHub Social Preview + Org Pinning Runbook

**Owner:** Gustavo Schneiter
**Created:** 2026-05-27
**Context:** GH-ORG-MARKETING agent set up `humangr-labs/humangr-labs/README.md` (renders on https://github.com/humangr-labs) and created the `.github` repo for default community health files. Two manual steps remain that require **owner UI access** (not exposed via the `repo`/`read:org` scoped token the agent uses).

---

## Why this is a Gustavo task (not agent task)

1. **Pin repos via UI** — GitHub's GraphQL `updateUserList` mutation requires `user` scope, which the agent's PAT does not have (it has `repo`, `read:org`, `workflow`, `gist`). Adding `user` scope to an automation PAT widens blast radius; recommend pinning manually instead.
2. **Upload social preview images** — repository social preview images can only be uploaded via the Settings UI; there is no REST/GraphQL endpoint for this.

Both steps take ~5 minutes total.

---

## Step 1 — Pin repos to `humangr-labs` profile (1 min)

1. Visit https://github.com/humangr-labs
2. Click **Customize your pins** (top-right of the profile, near "Popular repositories")
3. Select **exactly these two**:
   - `corelink-server`
   - `corelink-cli`
4. Click **Save pins**

(Currently the showcase shows `corelink-cli` + `.github`; swap `.github` for `corelink-server`.)

---

## Step 2 — Generate OG / social preview images (3 min)

GitHub renders the repo social preview as a 1280×640 (recommended) PNG when the repo URL is shared on Twitter / LinkedIn / Slack / Discord / HN. Default GitHub-generated previews look generic; custom previews signal craft.

### Recommended dimensions

- **1280 × 640 px** (GitHub's documented recommended size)
- **PNG**, < 1 MB
- Avoid putting critical text in the outer 10% margin (some platforms crop)

### Recommended brand palette (compliance / trust signal)

No brand color is yet defined in the codebase or docs site. Suggested palette to start (can iterate later):

| Token        | Hex         | Use                          |
|--------------|-------------|------------------------------|
| `primary`    | `#0F172A`   | Deep slate — background      |
| `accent`     | `#6366F1`   | Indigo — CTA / highlight     |
| `text`       | `#F8FAFC`   | Off-white — body on dark     |
| `muted`      | `#94A3B8`   | Slate-400 — subtitle / meta  |

Rationale: deep-slate + indigo reads "infrastructure / compliance / serious" rather than "consumer SaaS." Avoid bright purples/pinks (Vercel-aesthetic) — wrong signal for the audience.

### Text content per repo

**`corelink-server`** (https://github.com/humangr-labs/corelink-server/settings)

```
Title:    CoreLink
Subtitle: Content-addressable build cache for
          compliance-aware teams
Footer:   REAPI v2 · BYOK · Audit chain · 5 GB free
Brand:    humangr-labs
```

**`corelink-cli`** (https://github.com/humangr-labs/corelink-cli/settings)

```
Title:    corelink
Subtitle: Bazel · Buck2 · Cargo · Docker
          cache client
Footer:   curl -fsSL corelink-get.humangr.com | sh
Brand:    humangr-labs
```

### Tooling — fastest path

Pick ONE:

1. **og-image.vercel.app** (free, fast, no signup)
   - Edit URL params, screenshot at 1280×640
   - Pros: zero friction. Cons: limited layouts.
2. **bannerbear.com/og-images** (free tier)
   - More templates, browser-based editor
3. **Figma** (free)
   - File → New → 1280×640 frame → export PNG
   - Best for iterating brand later
4. **Canva** (free)
   - Search "GitHub social preview" templates → resize to 1280×640

Recommendation: **Figma** if you plan to define brand assets across docs/site/HN-launch images (one source of truth). **og-image.vercel.app** if you want this done in five minutes and refined later.

### Upload steps (per repo)

1. https://github.com/humangr-labs/corelink-server/settings (scroll to **Social preview**)
2. Click **Edit** → **Upload an image**
3. Repeat for `corelink-cli`

---

## What is already done (by GH-ORG-MARKETING agent)

- Created `humangr-labs/humangr-labs` (self-named profile repo, renders on https://github.com/humangr-labs). Commit `d1827c2`.
- Created `humangr-labs/.github` (for future default community health files: issue templates, default contributing guide, default code of conduct). Commit `84eea02`.
- Both repos contain the same `README.md` (org-level profile copy) with:
  - CoreLink + corelink-cli surfaced
  - Trust block (forbid(unsafe_code), TLA+, BLAKE3 audit chain, cargo-deny, DPA)
  - Solo-founder transparency (links to twitter.com/gschneiter)
  - No buzzwords, no overclaimed certifications

## Verification checklist (after Gustavo finishes the two manual steps)

- [ ] https://github.com/humangr-labs renders the profile README correctly
- [ ] Top-of-profile shows `corelink-server` + `corelink-cli` pinned (in that order)
- [ ] Share https://github.com/humangr-labs/corelink-server in Slack / Twitter and confirm OG card renders
- [ ] Repeat for corelink-cli

## Future work (not in scope here)

- Define `brand.json` (or `tokens/brand.css`) committed to repo so docs site + OG images + future launch assets share one source of truth
- Add issue templates + PR template to `humangr-labs/.github` once we have actual community
- Consider migrating `humangr-labs` from User -> Organization account (gives access to org-level settings, SSO, audit log) — but only after first paying customer to avoid premature GitHub Org billing
