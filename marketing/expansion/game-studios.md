# Expansion candidate — game studios (derived-data cache as the wedge)

> **Status:** CONCEPT BRIEF — early. Drafted 2026-08-23 from sourced research.
> Nothing committed, no code written. **Not** a change to the launch route.
>
> **Why this exists:** every other expansion candidate examined on 2026-08-23
> (durable event streaming, the "universal build cache" positioning, Perforce
> replacement) turned out to be an occupied category. **This one is not**, and the
> reason is specific and checkable: the platform owner explicitly withdrew from it.
>
> **Confidence:** all claims sourced or marked **UNVERIFIED**. One central
> assumption — cross-tenant dedup of engine-derived data — remains unproven and is
> flagged throughout. Do not build the pitch on it until it is measured.

---

## The one-sentence thesis

**Game studios generate enormous volumes of expensive derived data (compiled
shaders, cooked assets, baked lighting) that is content-addressed by design;
Unreal's own cache protocol is CAS + namespace + bearer token, which is CoreLink's
exact shape; and nobody hosts it — Epic pulled its own hosted offering and left
studios to run a ScyllaDB cluster themselves.**

---

## Why this vertical, when the others were occupied

| Space | Platform owner | Third-party hosted competitor |
|---|---|---|
| Event streaming | Cloudflare Queues | Confluent, WarpStream, Pub/Sub |
| Software build cache | — | Depot, Cachely, Buildless, BuildFetch |
| Game VCS (Perforce) | **Epic open-sourced Lore (MIT)** | Diversion, Anchorpoint, Xet |
| **Game derived-data cache** | **Epic withdrew from hosting** | **none found** |

Epic's Unreal Cloud DDC shipped only as an Azure "managed application" the studio
deployed into **its own** cloud account, and that listing was **removed**: *"The
Unreal Cloud Derived Data Cache (DDC) offering has been removed from the Azure
Marketplace… retiring this documentation on March 31, 2024"* — the docs now say to
contact an Epic representative. Unity Accelerator is likewise self-host-only. No
third-party hosted DDC service was found. **UNVERIFIED as a certified absence** — a
niche vendor could exist unindexed.

---

## The technical fit

Unreal Cloud DDC's HTTP API:

```
PUT /api/v1/refs/{namespace}/{bucket}/{identifier}
GET /api/v1/refs/{namespace}/{bucket}/{identifier}.raw
```

*"the hash of the payload is used as its identifier"* (`X-Jupiter-IoHash`). Auth is
OIDC/JWT bearer with per-namespace ACLs (ReadObject / WriteObject / DeleteObject).

**That is CAS + namespace + bearer + per-namespace scope — CoreLink's architecture
under different names.**

The self-host burden is the opening: Epic's recommended deployment requires standing
up and operating a **ScyllaDB cluster** plus object storage. Most studios do not have
an infrastructure team for that.

---

## Why a studio would try a third party here specifically

**DDC is derived data.** If it is lost, the studio regenerates it. That is the
**lowest trust bar in the entire studio pipeline** — unlike source control (their
source of truth) or console builds (NDA-gated). It is the easiest thing in a
studio's stack to hand to an outside vendor, which makes it the right wedge.

Payload size also makes zero egress worth more here than in software CI: derived
data is orders of magnitude larger than software build artefacts, and every
developer and artist pulls it. **UNVERIFIED:** real per-studio DDC sizes. One forum
anecdote cites ~16 GB locally; no authoritative figures were found.

---

## Coexistence, not displacement — and why that matters

The Gradle surface examined the same day has the opposite property: Gradle's
`buildCache` block accepts **only one** remote cache, so winning a Gradle team means
**replacing Develocity** — an evaluate-and-switch decision.

Here a studio **adds** CoreLink alongside everything it already runs. Nothing is
cancelled, no contract is torn up: point the DDC at us and keep IncrediBuild,
Perforce, and the rest. The adoption bar drops from "run an evaluation" to "try it
this afternoon."

**Honest note:** we do not accelerate IncrediBuild, we shrink its job — every cache
hit is work it never performs. The two coexist comfortably today, but over time this
consumes their value. Better to know that before a commercial conversation.

---

## The pipeline, mapped to the three pillars

**Cache — the wedge (space empty).** Unreal DDC (shaders, cooked assets, baked
lighting) and Unity Accelerator. The C++ compilation half may also be reachable via
the sccache surface CoreLink already has — **under investigation; see open
questions.**

**Runners — expansion, not entry (space contested).** Cook farms, shader
compilation, automated tests. IncrediBuild is entrenched (used by Microsoft, Epic,
Nintendo, Disney) and Epic ships Unreal Build Accelerator free. Documented demand is
real: The Coalition deployed **700 Azure cores**, and their IT manager states
*"without it, development would grind to a halt"*; without acceleration *"opening a
level can take up to 30 minutes."*

**Workspaces — not the wedge (space contested).** The Perforce pain is severe and
sourced — *"7+ hour pull times"*, a 400 MB checkout taking two hours, *"Perforce
hell"*, and 45% of designers bypassing version control for plain cloud storage. But
**Epic open-sourced Lore** (MIT, Rust) to escape Perforce, and the switching barrier
is **exclusive file locking**, which artists require for unmergeable binaries and
which `clw` does not implement. Diversion, Anchorpoint, and Xet are already here.

**The correction this represents:** an earlier draft claimed all three pillars mapped
equally. They do not. The cache is empty space; the other two are contested. Enter
through the cache, and let the other two become expansion inside an account that
already trusts us.

---

## Console builds: treat as out of scope

Console SDKs are NDA-gated and hosting them requires per-relationship written
approval from the platform holder — the documented mechanism is the Xbox 360-era
"Third Party Hosting Agreement," and Microsoft runs its own sanctioned Azure path for
GDK. **No public evidence exists that any non-platform-holder shared cloud has ever
been approved**, and current PS5 / Xbox Series / Switch terms are confidential.

This costs less than it appears: the expensive work — C++ compilation, shader
compilation, asset cooking, lighting bakes, automated tests — is **not** gated.

---

## Open questions that could kill or resize this

1. **Is engine-derived data byte-identical across studios on the same engine
   version?** The cross-tenant dedup pitch depends entirely on this and **it is
   unproven** — DDC key composition is not public, and no source confirms or denies
   it. This was an assumption, not a finding. Measure before promising.
2. **Can the C++ half be taken via sccache, or has Epic's UBA already occupied it?**
   Under investigation.
3. **Licensing.** Implementing a compatible server from **public documentation** is
   the intended path. The Unreal Cloud DDC source is under the Unreal Engine licence
   and reading it risks contamination. This needs a conscious decision, not an
   improvisation.
4. **Market size.** No authoritative data on studio DDC volumes or on how many
   studios would pay for hosting.
5. **Would studios trust an outside vendor at all?** The derived-data argument says
   this is the easiest possible ask, but it is reasoning, not evidence.

---

## Recommended next step

Not code. **Answer question 1** — whether engine-derived data deduplicates across
studios — because it is the difference between "a hosted cache in an empty space"
(good) and "a hosted cache in an empty space with a structural network effect no
competitor can replicate" (much better). Everything else can wait on that answer.
