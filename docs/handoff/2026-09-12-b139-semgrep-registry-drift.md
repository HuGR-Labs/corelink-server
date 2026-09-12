# B-139 Semgrep registry drift — partial local evidence

Date: 2026-09-12. Source: `origin/main` at `6be19a2e525dad045ad8404d722905afde7ad7bd`.
This is a redacted evidence record, not B-139 closure. No finding payloads, source snippets,
tokens, or raw SARIF are committed here.

## Locked bundle result

With Semgrep 1.164.0 on Python 3.12, `python3 scripts/run_b139_semgrep.py
--semgrep-bin .semgrep-venv/bin/pysemgrep --lock semgrep-bundled-lock.json
--output-dir <private temporary directory> --target .` exited 1 **before either scan**.
The runner verified the first four registry responses against their SHA-256 locks, then
rejected `p/owasp-top-ten`. A separate read-only digest check covered all six canonical
responses (no redirects):

| pack | locked SHA-256 | current SHA-256 | result |
| --- | --- | --- | --- |
| `p/security-audit` | `b109a039df712f30c6d3e25e1e8358053fd0f1c91b92d0e8d2871cd141fe602f` | same | match |
| `p/rust` | `3a769ea74a51ff71b66adc8de48af2baf7623db8940b08cba190b03cb7259ae7` | same | match |
| `p/typescript` | `63fbcca1826e787ca43282bf139ccec16745ad6551c87850b9ccee9ca9f98c0b` | same | match |
| `p/python` | `31c1dfa46e8ddd97f9ac98c607ddd77b20a2c3356d7ec987359961d47ec27035` | same | match |
| `p/owasp-top-ten` | `33ecaa276c934ba720203694ad8cf3053add773e79ad466793c9f9659e6303a7` | `2e2e1afa06df5e84d01f63372ac82b69ddcc4cd28efd45f375a1e8c910524146` | drift |
| `p/cwe-top-25` | `92faf729183898ca91e01d04f502f3abd8cbaac0cd5cc66390e81c6fbbdf9f4f` | `6e74bbac85db68786f2681264e5d69d3f6aeeb861092b6989e84dff30ac70758` | drift |

The current `p/owasp-top-ten` response contains 560 rule entries (365 WARNING,
170 ERROR, 14 INFO, 11 MEDIUM); the current `p/cwe-top-25` contains 216
(116 WARNING, 98 ERROR, 2 INFO). Across all six current responses there are
1,237 entries and 708 unique rule IDs. These are **current** populations, not a
deletion/severity delta: the repository locks digests but does not retain the old
YAML bytes, and no matching pinned response was found in local caches or Git.
The historical “474 rules run” is an applicable scan count, not directly
comparable to the current registry's unique-ID count. Repinning from mutable
aliases without the old bytes would silently accept an unreviewed rule change.

## Separate custom-pack scan

`pysemgrep scan --config ./semgrep.yml --sarif --output <private temporary
directory>/semgrep-custom.sarif --metrics off --jobs 1 --disable-version-check .`
completed with exit 0. It ran 3 applicable rules on 2,043 targets. The CLI
reported 2,688 findings; the canonicalized SARIF contains 2,689 WARNING
results: 2,657 `corelink.rust.no-unwrap-in-src` and 32
`corelink.rust.no-expect-in-byok-src`. One of the 2,689 is marked
`inSource`-suppressed, accounting for the CLI/SARIF count difference. No
custom-pack ERROR result was present. The canonicalized SARIF is 3,010,459
bytes, SHA-256 `0584599936627b43956fc93e49112669a480bd0fb787195af96645756d7835e4`;
the file remains outside Git in a private temporary directory. This standalone
scan is not a substitute for the two-population bundle or its report.

The workflow creates `.semgrep-venv/` at the repository root. This change adds
the root-anchored `/.semgrep-venv/` pattern to both ignore files so bundled
Python rules do not scan the scanner's own dependencies, while nested source
paths named `.semgrep-venv/` remain visible; a focused regression test enforces it.

## Closure boundary

Recent manual dispatches `34260711325` (2026-09-08) and `34317740889`
(2026-09-09) ended `startup_failure` with zero jobs, not successful scans.
B-139 stays parked. Before a repin, retrieve the historical pinned YAML from
a trustworthy retained artifact or registry revision and compare rule IDs,
patterns, severities, and deletions; then review any change with Security.
After that decision, rerun the complete locked bundle, triage all ERROR
findings, retain both SARIF files and the generated policy report, and obtain
a green real dispatch with an explicit SARIF-upload outcome. None of those
closure claims is made by this record.
