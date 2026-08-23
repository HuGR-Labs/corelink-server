# Expansion candidate — game studios (derived-data cache as the wedge)

> **Status:** 🗄️ **SHELVED 2026-08-23 — on sequencing and on a deflated moat.**
> The market gap is real and verified. The *moat* is not: cross-tenant dedup was
> the reason this looked like it could obliterate someone, and it does not hold
> (see "The dedup question — ANSWERED, downward"). What remains is a normal
> business opportunity in an empty space — good, but defensible only by execution.
> Combined with CoreLink having no paying customer on its existing product, and
> this vertical requiring domain knowledge and sales into a conservative industry,
> the sequencing argument says not now.
>
> **Revisit when:** the core product has paying customers, OR a game studio asks
> for hosted DDC by name, OR someone with games-industry access joins.
>
> **Kill early if:** Epic resumes hosting Unreal Cloud DDC, or a third-party
> hosted DDC vendor appears.
>
> **CONCEPT BRIEF — drafted 2026-08-23 from sourced research.**
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
| Game C++ compile cache | **Epic ships UBA cache, free** | octobuild, FASTBuild |
| Game build distribution | **Epic ships UBA** | IncrediBuild (entrenched) |
| **Game derived-data cache (hosted)** | **Epic withdrew from hosting** | **none found** |

The pattern across the whole day is that the platform owner is already sitting in
almost every adjacent space — Cloudflare Queues, Epic's Lore, Epic's UBA. The
hosted-DDC gap is the documented exception, and it exists because Epic actively
**left**: the offering was removed from the marketplace rather than never built.

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

**Cache — the wedge, but only the derived-data half (space empty).** Unreal DDC
(shaders, cooked assets, baked lighting) and Unity Accelerator.

**The C++ compilation half is contested — investigated and closed.** Epic's Unreal
Build Accelerator ships its own action-level compile cache (`Cache`, `WriteCache`,
`CacheProviders`, `bResetCas`), Beta in UE 5.4 and Production on Windows since 5.5.
It is free and increasingly the default. It is also **LAN-scoped and not pluggable**:
entries live at `C:\ProgramData\Epic\UbaCli\cas\casdb`, are served by a dedicated
`UbaCache.exe` addressed by IP, and cross-machine hits require UBA's own VFS path
virtualization — there is no documented way to point it at third-party HTTP.

Beyond Epic, **UBT has no official ccache/sccache hook** (community patches only);
the tool studios actually use is **octobuild**, with measured warm-cache results of
Linux 3m54s→36s and Windows 8m4s→2m15s; and **FASTBuild** is a long-standing
incumbent with mature PCH and unity-build cache-safety fixes.

Two structural obstacles compound this. Unreal's **unity/jumbo builds** bundle many
`.cpp` files into large translation units, so editing one file invalidates the whole
blob — this fights fine-grained compile caching by construction. And **MSVC
determinism** is a real problem: clang-cl embeds timestamps by default, and UE's own
`bDeterministic` flag *"disables codegen multithreading so compiling will be
slower."*

**Conclusion: do not plan on the C++ half.** It is not empty space, and the technical
headwinds are real.

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

1. ~~Is engine-derived data byte-identical across studios?~~ **ANSWERED —
   PARTIAL, and the answer removes the moat.** See below.
2. ~~Can the C++ half be taken via sccache?~~ **ANSWERED — no, treat it as
   contested.** See the pillar mapping above.
3. **Licensing.** Implementing a compatible server from **public documentation** is
   the intended path. The Unreal Cloud DDC source is under the Unreal Engine licence
   and reading it risks contamination. This needs a conscious decision, not an
   improvisation.
4. **Market size.** No authoritative data on studio DDC volumes or on how many
   studios would pay for hosting.
5. **Would studios trust an outside vendor at all?** The derived-data argument says
   this is the easiest possible ask, but it is reasoning, not evidence.

---

## The dedup question — ANSWERED, downward

This was the brief's central open question, and answering it is what moved this
document from "candidate" to "shelved."

**The evidence is decisive and comes from Epic itself.** Engine downloads from the
Epic Games Store ship with a **DDC Pak (`.ddp`)**, which *"contains derived data for
all engine content, so you can start working without compiling shaders and other
engine assets that use derived data."* Engine-derived data is therefore provably
identical and reusable across every install of a given engine version, independent
of project. Cross-tenant dedup on that slice is real, not speculative.

**But the second-order reading removes the value:**

- **The slice that deduplicates, Epic already ships for free.** A studio downloading
  the engine already has it locally. There is nothing left to capture.
- **The slice that is expensive does not deduplicate.** Material shader-map keys are
  built from that material's own `ShaderMapId` — permutations, static switches,
  material attributes. Materials are studio-authored assets; two studios essentially
  never collide by coincidence.

**So the network-effect argument does not hold.** The shareable part is free and the
valuable part is private by construction.

**Three unverified possibilities partially rescue it**, and would need measuring
before anyone revives this:

- **Engine built from source.** AAA studios frequently build Unreal with their own
  modifications, in which case the Epic DDC Pak does not apply and the engine slice
  must be compiled per studio again.
- **Target platforms.** The Pak plausibly covers editor/desktop only; console and
  mobile targets would still compile.
- **Engine patches.** Each one invalidates the Pak.

**No public estimate exists** of what fraction of a real project's DDC is
engine-derived versus project-derived — this was searched for and not found.

**The definitive experiment**, if this is ever revived: two clean installs of the
same exact UE build, two unrelated real projects with their own rendering settings,
local DDC deleted, build both, and diff the resulting **global-shader** DDC keys and
blobs byte-for-byte. Global shaders — not material shader maps — are where sharing
would occur.

---

## Recommended next step

~~Answer question 1.~~ **Done — and the answer is why this is shelved rather than
pursued.** The gap is real; the moat is not. If revived, the first step is the
definitive dedup experiment described above, because the three unverified rescues
(source-built engines, non-desktop platforms, engine patches) are the only path back
to a structural advantage.
