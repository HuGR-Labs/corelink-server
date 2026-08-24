#!/usr/bin/env python3
"""Report the remote cache hit ratio of a Bazel execution log.

Reads the file written by `bazel --execution_log_json_file=<path>` and prints
`hits/total = ratio%`, exiting non-zero when the ratio is below `--min`.

Three things about that file are easy to get wrong, and this repo got all three
wrong for a year (BACKLOG B-017):

1. It is NOT newline-delimited JSON. Bazel pretty-prints each SpawnExec across
   many lines, so a line-by-line `json.loads` fails on every single line and
   reports "no entries" no matter how well the cache is working. We stream the
   file with `JSONDecoder.raw_decode` instead.
2. The hit field is `cacheHit` (with `runner` naming which cache answered).
   `remoteCacheHit` was the Bazel <= 6 name and no longer appears at all.
3. The denominator has to be the spawns that COULD have hit -- the ones Bazel
   marks `remoteCacheable` -- not "every entry that carries a hit field". The
   old code took its total from the presence of a field name, which silently
   redefines the ratio whenever Bazel changes what it emits.

A local disk-cache hit sets `cacheHit` too, so by default only spawns whose
`runner` says a REMOTE cache answered are counted (`--allow-disk-cache` opts
out of that, for local experiments). That is
the distinction the CI gate exists to make: a green ratio served by the local
disk cache would prove nothing about CoreLink.
"""

from __future__ import annotations

import argparse
import json
import sys

# `runner` values Bazel writes for a hit served over the network. "remote cache
# hit" is the REAPI action-cache answer this example's gate is about.
REMOTE_RUNNERS = ("remote cache hit", "remote")


def iter_spawns(text: str):
    """Yield each SpawnExec object in a pretty-printed execution log."""
    decoder = json.JSONDecoder()
    i, n = 0, len(text)
    while i < n:
        while i < n and text[i] in " \t\r\n":
            i += 1
        if i >= n:
            return
        obj, i = decoder.raw_decode(text, i)
        yield obj


def ratio(text: str, require_remote: bool = True):
    """Return (hits, total) over the spawns that were eligible to hit."""
    hits = total = 0
    for spawn in iter_spawns(text):
        if not spawn.get("remoteCacheable", False):
            continue
        total += 1
        if not spawn.get("cacheHit", False):
            continue
        runner = spawn.get("runner", "")
        if require_remote and not any(r in runner for r in REMOTE_RUNNERS):
            continue
        hits += 1
    return hits, total


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", help="path written by --execution_log_json_file")
    parser.add_argument("--min", type=float, default=80.0,
                        help="minimum acceptable hit ratio, in percent")
    parser.add_argument("--allow-disk-cache", action="store_true",
                        help="count local disk-cache hits too (default: remote only)")
    args = parser.parse_args(argv)

    try:
        with open(args.log) as handle:
            text = handle.read()
    except FileNotFoundError:
        print(f"FAIL: no execution log at {args.log} — did the build run?", file=sys.stderr)
        return 1

    try:
        hits, total = ratio(text, require_remote=not args.allow_disk_cache)
    except json.JSONDecodeError as exc:
        print(f"FAIL: {args.log} is not a Bazel JSON execution log ({exc}).", file=sys.stderr)
        return 1

    if total == 0:
        print("FAIL: the build ran no remote-cacheable spawns, so the cache was "
              "never asked anything.", file=sys.stderr)
        return 1

    percent = hits / total * 100
    print(f"Cache hit ratio: {hits}/{total} = {percent:.1f}%")
    if percent < args.min:
        print(f"FAIL: {percent:.1f}% < {args.min:.1f}% SLA.", file=sys.stderr)
        return 1
    print(f"PASS: hit ratio meets the >= {args.min:.0f}% SLA.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
