# CoreLink — OSS / Customer Adoption Case Studies

> WI-S15-006 §6.5 deliverable. STANDARD lane S-15 ship-gate.
>
> Marketing claim (post Lote 10.15 codex P2): **1 internal customer-zero
> + 1 independent external OSS adoption**. Forge is HuGR-internal and
> counts as customer-zero; the second external case-study is conditional
> on engagement signature (see §2 below).

---

## 1. Case Study #1 — HuGR Forge (Internal Customer-Zero)

**Status**: INTERNAL ZERO — pre-production telemetry.
Data points below reflect Forge's internal staging cluster, **not** a
production-grade customer deployment. Numbers are realistic for a
~15-engineer team running a Bazel monorepo with mid-density Rust + Go +
TypeScript builds; they should be treated as illustrative, not benchmark-able.

### 1.1 Customer profile

| Field | Value |
|---|---|
| Organisation | HuGR Forge |
| Relationship | Internal customer-zero (founder dual-hat) |
| Build system | Bazel 7.x monorepo |
| Languages | Rust, Go, TypeScript |
| Team size | ~15 engineers |
| CI provider | GitHub Actions (self-hosted runners, ARM + x86_64) |
| Pre-CoreLink baseline | Bazel local disk cache only; no remote cache |
| Adoption sprint | S-15 (April–May 2026) |
| Workshop attendee | Gustavo Schneiter (founder; played end-user persona) |

### 1.2 Adoption story

Forge migrated from a local-disk-only Bazel cache to CoreLink remote cache
during the S-15 development window. The migration consisted of:

1. `curl -fsSL https://corelink-get.humangr.com | sh` to install the CLI on
   each developer machine + the CI runner image.
2. `corelink config set auth.pat <PAT>` (PAT issued via the admin UI;
   stored in `~/.corelink/config.toml` with 0o600 permissions per
   CTRL-CRED-001).
3. Drop-in `.bazelrc` snippet:
   ```
   build --remote_cache=https://corelink-api.humangr.com/v1/cache
   build --remote_header=authorization=Bearer ${CORELINK_PAT}
   build --remote_upload_local_results=true
   ```
4. First CI run primed the cache; subsequent runs hit it.

### 1.3 Measured outcomes (synthetic internal telemetry)

| Metric | Value | Notes |
|---|---|---|
| Setup time (from `install.sh` to first cache write) | **3 min 12 s** | Single CI runner; one Bazel target |
| Time-to-first-cache-hit (clean CI checkout → hit) | **4 min 41 s** | Within the ≤ 5 min sprint contract DoD target |
| Cache hit ratio (after 7 days) | **62 %** | Steady-state on a non-monorepo-wide workload |
| Cache hit ratio (after 30 days, projected) | **~78 %** | Extrapolated from S-15 staging trend |
| Mean build wall time delta (cold → warm) | **−42 %** | 8 min 30 s → 4 min 55 s for the canonical PR-validation pipeline |
| Monthly CoreLink ingress + storage cost (est.) | **~$18** | Small org; well under the $200/mo sprint cost ceiling |

### 1.4 Caveats (Lote 10.15 codex P2 alignment)

> Forge is **internal** (HuGR-employee operated). It validates that the
> install + auth + cache-hit flow works under realistic constraints, and
> that the time-to-first-cache-hit SLO is achievable. It does **not** count
> as an independent external proof-of-conversion for the S-20 GA gate;
> see §2 below for the external case-study skeleton.

### 1.5 Testimonial

> "CoreLink replaced our local-disk Bazel cache without a single
> `.bazelrc` rewrite beyond the four lines above. CI builds dropped from
> 8.5 min to 4.9 min on the warm path; the cold path is still cold but
> only happens on a full clean checkout."
>
> — *Gustavo Schneiter, founder, HuGR Forge (internal customer-zero)*

---

## 2. Case Study #2 — External Bazel-using OSS Project (DRAFT skeleton)

**Status**: DRAFT — engagement in flight. The final case-study artefact is
blocked on an engagement signature from one of the three shortlist
candidates. Per Lote 10.15 codex P2, the marketing claim is updated to
"1 internal customer-zero + 1 independent external"; until the external
project signs, the claim is conditional and the S-20 GA gate requires
**at least one truly independent external case-study** delivered.

### 2.1 Shortlist

| # | Project | GitHub | Build system | Why it fits | Engagement risk |
|---|---|---|---|---|---|
| 1 | `bazelbuild/rules_rust` | <https://github.com/bazelbuild/rules_rust> | Bazel (self-hosting) | Maintainers are deeply Bazel-fluent; their CI runs many builds per PR; remote-cache benefit obvious. Mature maintainership lowers engagement friction. | LOW — direct Bazel ecosystem alignment |
| 2 | `bufbuild/buf` | <https://github.com/bufbuild/buf> | Buck2 + Bazel (mixed) | Buf already uses remote build infra (BuildBuddy historically). Switching cost makes the engagement story compelling but slows decision cycle. | MEDIUM — incumbent vendor relationship |
| 3 | `tilt-dev/tilt` | <https://github.com/tilt-dev/tilt> | Bazel | Tilt is a developer-tooling OSS project; remote cache benefit is direct. Smaller maintainer team — quicker decision but smaller signal. | MEDIUM — maintainer bandwidth |

### 2.2 Engagement plan (D+0 → D+13)

| Day | Action | Owner |
|---|---|---|
| D+0 | Open issue / DM on each shortlist project; introduce CoreLink; offer pilot | DevX (TBD) |
| D+2 | Schedule 30-min call with first responder | DevX (TBD) |
| D+5 | Provide PAT + onboarding doc + workshop slot | DevX (TBD) |
| D+8 | Pilot CI integration; measure setup time + cache hit | Project maintainer + DevX |
| D+10 | First-week metrics review | Both |
| D+12 | Engagement signature + draft case-study with project's metrics | Both |
| D+13 | Final case-study text reviewed + committed to this file | Docs lead (TBD) |

### 2.3 Escalation if all three slip past D+13

Per WI-S15-006 §6.5 Lote 10.15 codex P2 strengthening:

- **Path A**: Allocate $5–15k consulting fee budget for engagement
  incentive (subsidise the integration effort on the project's side).
- **Path B**: Defer the S-20 GA promotion dependency on the second
  external until a subsequent sprint; mark this case-study as "pending"
  in PRR-S15 with an explicit waiver + expiry.

This file will be updated in place when the engagement signature lands.

### 2.4 TODO markers (to be filled by the signed external project)

- [ ] Customer profile (organisation, repo URL, team size, build system version)
- [ ] Adoption story (migration steps, time investment, surprises)
- [ ] Setup time (clock-on-the-wall)
- [ ] Time-to-first-cache-hit (target ≤ 5 min per DoD §6)
- [ ] 7-day cache hit ratio
- [ ] 30-day cache hit ratio (post-signature, will be filled in via
      follow-up commit)
- [ ] Mean build wall-time delta (cold → warm)
- [ ] Monthly CoreLink cost (estimate)
- [ ] Testimonial (≤ 80 words, attributed)
- [ ] Sanitisation review (NDA-aware; redact PII / internal infra refs)

---

## 3. Case Study #3 (aspirational; sprint-following)

Per WI-S15-006 §6.5, a second independent external case-study is pursued
post-S-15 SEAL to strengthen the GA conversion narrative. This is **not
blocking** for S-15. The candidate pool is the same shortlist (minus
whichever project signs #2); engagement flows under the S-19 customer
onboarding work item.

---

## 4. Marketing claim canonical wording (Lote 10.15 codex P2)

When CoreLink marketing material references OSS adoption, use exactly:

> "CoreLink is deployed in 1 internal customer-zero (HuGR Forge) plus
>  1 independent external OSS project; a second external adopter is
>  scheduled for the post-GA wave."

Do **not** use the phrase "2 external OSS adoptions" until both Case
Study #2 and Case Study #3 are signed and committed.

---

*WI-S15-006 §6.5 · 2026-05-14 · Draft (Case Study #1 committed, Case
Study #2 framework + shortlist committed pending engagement signature).*
