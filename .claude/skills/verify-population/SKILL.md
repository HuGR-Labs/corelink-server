---
name: verify-population
version: 1.0.0
description: Catch the dominant defect class in this repo — a number that is CORRECT about the set it measures, while the sentence it supports talks about a DIFFERENT set. Invoke before writing any claim that carries a number (audit finding, PR body, backlog verify, status report, "N ghosts fixed", "0 violations", "queue wait is zero"), and when reviewing someone else's numeric claim. Triggers — about to write "N of M"; a measurement came back 0 / 100% / all-identical; a report says "all gates ran"; reviewing a PR body with counts.
---

# verify-population — the number is right, the population is wrong

> **Tests check that the number is computed correctly. Review checks that the
> sentence is well written. Nobody checks the JOIN** — that the population the
> number describes is the population the sentence talks about.

That join exists only in the head of whoever wrote it. This is why the defect
survives green CI, peer review, **and even a careful re-measurement** —
re-measuring confirms the number, never the join.

## Measured instances (all from one campaign)

| the sentence promised | the number actually measured | consequence |
|---|---|---|
| disk cost of the image | **downloaded** (121 MiB xz), not **installed** (680 MB) | build blew the box |
| runner queue wait | `startedAt` of the **RUN**, not the **JOB** — 0 in 200/200 | nearly shipped "fleet healthy" |
| docs make no signing claim | **mentions** of the literal, not **uses** | gate punished the removal |
| ghost census 28 → 0 | **files touched**, not **surface** | ghost survived in a code fence |
| "every gate ran" | checks **scheduled** that finished, not checks that **should exist** | an absent check never becomes pending |
| which KMS serves a tenant | which provider is **compiled in**, for the health label | concluded "single-provider architecture" — wrong |

The last one is the trap in its purest form: `active_provider()` answers *"which
provider is compiled in, so /healthz can report it"*. It was read as *"which
provider serves this tenant"*. Both readings are legitimate English for the
identifier. Only one is what the function does.

## The rule

**Every numeric claim states its population inline.** Not in a footnote — in the
same sentence, where it cannot be separated from the number.

- ❌ "28 → 0 ghosts"  ✅ "28 → 0 ghosts **in the 8 files touched**"
- ❌ "121 MiB"        ✅ "121 MiB **compressed / as downloaded**"
- ❌ "wait is zero"   ✅ "wait **measured at job level** is zero"
- ❌ "all gates green" ✅ "**the 11 checks that were scheduled** are green"

A sentence with a number and no population is where the defect lives. If you
cannot name the population in a few words, you do not yet know what you measured.

## Three detectors, ordered by how much they demand of you

### 1. Degenerate result — property of the OUTPUT, needs no domain knowledge

Variance zero across a large sample. 100% pass. 0% failure. `X − Y` always 0.
Every value identical.

> **Zero variance in a large sample is a symptom of the INSTRUMENT, not the world.**

`X − Y == 0` in 100% of the sample is not a finding — it is a sign that X and Y
are the same field. When the result is the degenerate value, **suspect the field
before you suspect the world.**

This detector is first because it fires without you knowing anything about the
domain. Use it as a reflex on every measurement.

### 2. Control proves REACH, not COVERAGE

A passing control proves your instrument reached *something*. It does not prove
it reached *everything the claim covers*.

Enumerate the **surface** the sentence promises, then show the filter covers it.
A sweep with `--include` that omits an extension has a green control sitting next
to a blind spot — the control was real, the coverage was not.

### 3. Expectation written BEFORE measuring

Write down what you expect to see — order of magnitude, range, or at minimum
"not zero" — *before* running the measurement, so the result has something it can
contradict.

> **Without a written expectation, a clean measurement and a wrong measurement are
> identical.** A result that matches nothing in particular is not confirmation; it
> is the absence of a test.

This one is last because it requires domain knowledge to have an expectation at
all. Detectors 1 and 2 work without it.

## Why plausibility is the real enemy

Two zeros appeared in the same day. `git diff` returning **0 files** on two PRs
was absurd on its face and was caught in seconds. `startedAt − createdAt == 0` in
200/200 was **plausible** — zero queue wait is normal in a healthy fleet — and
almost shipped as "fleet healthy", backed by 200 samples and three statistics.

Same defect class. The only difference was whether the wrong answer happened to
look strange. **A plausible wrong answer is the dangerous one, and plausibility is
exactly what switches off human judgment.** That is why detector 1 keys on a
property of the output instead of waiting for someone to find the number odd.

## Reviewing someone else's claim

Ask one question per number: **"what set does this count, and is that the set the
sentence is about?"**

Two extra traps:
- **Self-accusing reports skip scrutiny.** A claim that indicts its own author does
  not trigger the suspicion a defensive claim triggers — and it is wrong just as
  often. Re-derive it anyway. (A working monitor was switched off this way.)
- **A caveat is not the same as not asserting.** Hedging the sentence does not fix
  a wrong population.

## Related

`.claude/skills/teeth-test/` — proving a gate can fail at all.
`.claude/skills/built-not-wired/` — reachability claims specifically.
