# Final current-state readback — 2026-09-23

Read back after the final documentary verification wave. The campaign checkout
is `/private/tmp/corelink-ownership-campaign`, branch
`codex/corelink-ownership-campaign`, at `HEAD=3df52eb71acdd4e00084d42f0b64191674bf42eb`.

The latest direct remote check returned the same value from both commands:

```text
git rev-parse origin/main
7f966dda8234f54b58766f33e5fca2ad02cc898b

git ls-remote origin refs/heads/main
7f966dda8234f54b58766f33e5fca2ad02cc898b	refs/heads/main
```

The generated registry was then read back as:

- population: `105`;
- calibration: `PASS`;
- structural package results: `105 PASS`;
- cold review: `105 UNVERIFIED`;
- publication: `105 NOT_PUBLISHED`;
- publication count: `0`.

The documentary suite completed with `157` tests passing; the adversarial probe
completed `13/13` expected cases; `git diff --check` passed. These checks do
not freeze the standard, create package review records, merge the candidate to
`main`, certify Cargo/runtime execution, or publish GitHub issues.

Some independent review reports captured a different live-main response while
the remote was changing. This readback is the latest direct observation and
does not erase those historical reports; any freeze or publication attempt must
re-read and reconcile `main` again.
