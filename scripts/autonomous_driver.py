#!/usr/bin/env python3
"""CoreLink autonomous WI execution driver.

Invokes ``claude --print`` (Claude Code headless) once per work item in a
durable loop until either:
  • the queue is empty (CoreLink GA delivered), or
  • a hard inflection point is hit (stops, waits for user), or
  • too many consecutive failures occur (stops, waits for user).

State is persisted to ``scripts/autonomous_state.json``; progress logs to
``scripts/autonomous_driver.log``. A POSIX lock file at ``LOCK_PATH``
prevents two driver processes from executing in parallel.

Per-WI invocation is bounded to one ``claude --print`` call with a focused
prompt that points the spawned Claude instance at the autonomous-execution
charter in memory. Each invocation is its own session (no accumulating
context across WIs); auto-compact still applies within a single WI's
session if the work is large.

Usage:
  python3 scripts/autonomous_driver.py              # daemon loop
  python3 scripts/autonomous_driver.py --once WI    # one specific WI
  python3 scripts/autonomous_driver.py --bootstrap  # build queue + exit
  python3 scripts/autonomous_driver.py --status     # print state + exit

For true daemon mode:
  nohup python3 scripts/autonomous_driver.py > /dev/null 2>&1 &

Stop:
  touch /tmp/corelink_driver_stop   (graceful — finishes current WI then exits)
  kill <pid>                         (hard)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Optional

REPO = Path(__file__).resolve().parent.parent
STATE_PATH = REPO / "scripts" / "autonomous_state.json"
LOG_PATH = REPO / "scripts" / "autonomous_driver.log"
LOCK_PATH = Path("/tmp/corelink_driver.lock")
STOP_PATH = Path("/tmp/corelink_driver_stop")

# Bounded retries / failures
MAX_CONSECUTIVE_FAILS = 5
PER_WI_TIMEOUT_SECONDS = 7200  # 2 hr ceiling per WI invocation
INTER_WI_SLEEP_SECONDS = 5

# Output markers the driver inspects
SEAL_RE = re.compile(r"\bWI-S\d{2}-\d{3}\s+SEALED\b", re.IGNORECASE)
INFLECTION_MARKER = "INFLECTION:"
SPRINT_CLOSE_MARKER = "SPRINT-SEALED:"

# Hard inflection keywords (driver also checks for INFLECTION: prefix from
# the spawned Claude; these are belt-and-suspenders fallbacks)
INFLECTION_FALLBACK_KEYWORDS = [
    "needs Cloudflare credentials",
    "needs Stripe credentials",
    "needs Clerk credentials",
    "GitHub org permissions",
    "external pentest scheduling",
    "lighthouse customer recruitment",
    "GA launch button",
    "canonical-source bug detected",
]


# ----------------------------------------------------------------------------
# State model
# ----------------------------------------------------------------------------


@dataclass
class State:
    queue: list[str] = field(default_factory=list)
    done: list[str] = field(default_factory=list)
    failed: list[str] = field(default_factory=list)
    paused_at: Optional[str] = None
    paused_reason: Optional[str] = None
    consecutive_fails: int = 0
    bootstrap_at: Optional[str] = None
    last_seal_at: Optional[str] = None

    @classmethod
    def load(cls) -> "State":
        if STATE_PATH.exists():
            data = json.loads(STATE_PATH.read_text())
            return cls(**data)
        return cls()

    def save(self) -> None:
        STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
        STATE_PATH.write_text(json.dumps(asdict(self), indent=2))


# ----------------------------------------------------------------------------
# Logging
# ----------------------------------------------------------------------------


def log(msg: str) -> None:
    ts = time.strftime("%Y-%m-%d %H:%M:%S")
    line = f"[{ts}] {msg}"
    LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    with LOG_PATH.open("a", encoding="utf-8") as f:
        f.write(line + "\n")
    print(line, flush=True)


# ----------------------------------------------------------------------------
# Lock + stop signals
# ----------------------------------------------------------------------------


def claim_lock() -> None:
    """Atomically claim the lock or exit if another driver is already alive."""
    if LOCK_PATH.exists():
        try:
            stale_pid = int(LOCK_PATH.read_text().strip().split()[0])
            os.kill(stale_pid, 0)  # signal 0 = alive check
            log(f"Lock held by alive PID {stale_pid}; aborting.")
            sys.exit(11)
        except (ValueError, ProcessLookupError, PermissionError):
            log(f"Removing stale lock at {LOCK_PATH}")
            LOCK_PATH.unlink(missing_ok=True)
    LOCK_PATH.write_text(f"{os.getpid()} {time.time():.0f}\n")


def release_lock() -> None:
    LOCK_PATH.unlink(missing_ok=True)


def stop_requested() -> bool:
    return STOP_PATH.exists()


# ----------------------------------------------------------------------------
# Queue bootstrap
# ----------------------------------------------------------------------------


def bootstrap_queue(state: State) -> list[str]:
    """Scan ``specs/04_sprints/S*/work_items/`` and return WI IDs in order.

    Skips WIs already in ``state.done`` or ``state.failed``.
    """
    queue: list[str] = []
    sprints_dir = REPO / "specs" / "04_sprints"
    for sprint_dir in sorted(sprints_dir.glob("S*")):
        wi_dir = sprint_dir / "work_items"
        if not wi_dir.exists():
            continue
        for wi_file in sorted(wi_dir.glob("WI-S??-???-*.md")):
            stem = wi_file.stem
            parts = stem.split("-")
            if len(parts) < 3:
                continue
            wi_id = "-".join(parts[:3])  # WI-S01-001
            if wi_id in state.done or wi_id in state.failed or wi_id in queue:
                continue
            queue.append(wi_id)
    return queue


# ----------------------------------------------------------------------------
# Claude invocation
# ----------------------------------------------------------------------------


PROMPT_TEMPLATE = """You are continuing the CoreLink autonomous implementation under the user's standing mandate to deliver CoreLink GA without intervention. Read the charter and progress files in ``memory/`` BEFORE anything else:

  • ~/.claude/projects/-Users-gustavoschneiter-Documents-HuGR/memory/corelink_autonomous_execution_charter.md
  • ~/.claude/projects/-Users-gustavoschneiter-Documents-HuGR/memory/corelink_impl_progress.md

Your entire job right now is to drive ONE work item through the per-WI SEAL ceremony documented in those files, and then exit cleanly. Do NOT start the next WI; the driver picks the next one.

WORK ITEM: {wi_id}

Steps (cribbed from corelink_impl_progress.md "SEAL ceremony" — read that file for the canonical version):

  1. Read the WI spec and any canonical sources it inherits_from. Resolve every ambiguity BEFORE writing code, patching canonical sources in the same Lote if needed.
  2. Scaffold the crate (or reuse if WI says so). TDD: write canonical regression vectors + property tests FIRST.
  3. Implement under strict lints (#![forbid(unsafe_code)] + Cargo [lints] deny unwrap/expect/panic/indexing). Strategic #[allow(clippy::expect_used, reason="...")] only where Result Err is provably unreachable.
  4. Quality gates verde: clippy -D warnings (workspace), cargo test debug + release, cargo-mutants ≥80% kill rate, cargo-fuzz 60s smoke.
  5. Codex adversarial review (1-3 rounds). Codex command pattern: codex exec --sandbox read-only --output-last-message /tmp/gpt-codex-{wi_id_lower}.md --skip-git-repo-check "<focused prompt>". STOP looping at score ≥ 8.5/10. Apply ALL P1 (even stylistic).
  6. Spec drift detected during codex? Patch in same Lote. Bump _spec_contract.md version + WI version + ADR versions affected.
  7. Validators verde: validate_specs.py + validate_inv_promotion.py + validate_references.py.
  8. WI frontmatter: doc_status: DRAFT → FROZEN, work_status: READY → DONE, version bump, body header line and §31 changelog updated.
  9. Commit with message: "{wi_id} SEALED: <short> + codex <a→b/10>" plus body covering workspace changes, test coverage, codex findings, spec docs touched, and "Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>".
 10. If this WI is the LAST WI of its sprint (check the sprint contract WI list), also: update sprint.md doc_status DRAFT → FROZEN with sign-off table, bump _spec_contract.md major, and run ``git tag s<NN>-sealed -m "Sprint S-<NN> implementation SEALED"`` BEFORE the final commit. Output the marker line ``SPRINT-SEALED: S-<NN>`` somewhere in your text response so the driver can detect it.
 11. Update memory/corelink_impl_progress.md with the new WI status row, codex score progression, any new lessons.

If at any point you hit a hard inflection point that requires user intervention (Cloudflare credentials, GitHub org permissions, Stripe/Clerk credentials, external pentest scheduling, lighthouse customer recruitment, GA launch button, or canonical-source bug detected), output a single line beginning with ``INFLECTION:`` followed by a one-paragraph reason, save state, and stop.

Deliverable for this invocation: a clean ``git status`` (working tree clean, last commit is the SEAL commit) and the marker ``{wi_id} SEALED`` in your text response.

NEVER lower the quality bar to finish faster. NEVER skip codex. NEVER skip validators. The user's mandate is impeccable + sem gambiarras.

Begin.
"""


def run_claude(wi_id: str, timeout: int = PER_WI_TIMEOUT_SECONDS) -> tuple[bool, str]:
    """Invoke ``claude --print`` for one WI; return (success, full_output)."""
    if not shutil.which("claude"):
        return False, "claude CLI not found in PATH"

    prompt = PROMPT_TEMPLATE.format(wi_id=wi_id, wi_id_lower=wi_id.lower())

    cmd = [
        "claude",
        "--print",
        "--effort",
        "high",
        "--dangerously-skip-permissions",
        prompt,
    ]
    log(f"Spawning claude --print for {wi_id} (timeout={timeout}s)")
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=REPO,
        )
        out = proc.stdout + ("\n--STDERR--\n" + proc.stderr if proc.stderr else "")
        return proc.returncode == 0, out
    except subprocess.TimeoutExpired as exc:
        partial = (exc.stdout or "") + "\n--TIMEOUT--\n" + (exc.stderr or "")
        return False, f"TIMEOUT after {timeout}s\n{partial}"


# ----------------------------------------------------------------------------
# Output parsing
# ----------------------------------------------------------------------------


def detect_seal(output: str, wi_id: str) -> bool:
    if f"{wi_id} SEALED" not in output:
        return False
    # Verify: the spawned Claude should have ended with a clean working tree.
    # Cheap secondary check: look for the canonical phrase.
    return True  # Belt verification is the post-call git status anyway.


def detect_inflection(output: str) -> Optional[str]:
    if INFLECTION_MARKER in output:
        idx = output.index(INFLECTION_MARKER)
        snippet = output[idx : idx + 500].splitlines()[0]
        return snippet
    for kw in INFLECTION_FALLBACK_KEYWORDS:
        if kw in output:
            return f"keyword fallback: {kw}"
    return None


def detect_sprint_close(output: str) -> Optional[str]:
    m = re.search(r"SPRINT-SEALED:\s*S-?(\d{2})", output)
    return m.group(1) if m else None


def git_clean() -> bool:
    proc = subprocess.run(
        ["git", "status", "--porcelain"],
        capture_output=True,
        text=True,
        cwd=REPO,
        check=False,
    )
    return proc.returncode == 0 and not proc.stdout.strip()


# ----------------------------------------------------------------------------
# Driver loop
# ----------------------------------------------------------------------------


def execute_one(state: State, wi_id: str) -> bool:
    """Run one WI; mutate state in place; return True if SEALED."""
    success, output = run_claude(wi_id)

    inflection = detect_inflection(output)
    if inflection:
        log(f"INFLECTION at {wi_id}: {inflection}")
        state.paused_at = wi_id
        state.paused_reason = inflection
        state.save()
        return False

    sealed = success and detect_seal(output, wi_id) and git_clean()
    if sealed:
        log(f"SEALED {wi_id}")
        state.done.append(wi_id)
        if wi_id in state.queue:
            state.queue.remove(wi_id)
        state.consecutive_fails = 0
        state.last_seal_at = time.strftime("%Y-%m-%d %H:%M:%S")
        sprint = detect_sprint_close(output)
        if sprint:
            log(f"SPRINT-SEALED detected: S-{sprint}")
        state.save()
        return True

    log(
        f"FAILED {wi_id} (success={success}, "
        f"seal_marker={SEAL_RE.search(output) is not None}, "
        f"git_clean={git_clean()})"
    )
    log(f"...output tail: {output[-800:]}")
    state.consecutive_fails += 1
    state.save()
    return False


def loop() -> int:
    state = State.load()

    if not state.queue:
        log("Queue empty — bootstrapping from specs/04_sprints/")
        state.queue = bootstrap_queue(state)
        state.bootstrap_at = time.strftime("%Y-%m-%d %H:%M:%S")
        state.save()
        log(f"Bootstrapped queue: {len(state.queue)} WIs ahead, {len(state.done)} done")

    if state.paused_at and state.paused_at in state.queue:
        log(
            f"Resuming from previous pause at {state.paused_at} "
            f"(reason: {state.paused_reason})"
        )
        state.paused_at = None
        state.paused_reason = None
        state.save()

    while state.queue:
        if stop_requested():
            log("Stop signal received; exiting after current state save.")
            STOP_PATH.unlink(missing_ok=True)
            return 0
        if state.consecutive_fails >= MAX_CONSECUTIVE_FAILS:
            log(f"Hit {MAX_CONSECUTIVE_FAILS} consecutive fails — pausing.")
            state.paused_reason = (
                f"{MAX_CONSECUTIVE_FAILS} consecutive failures; user investigation required"
            )
            state.save()
            return 2

        wi_id = state.queue[0]
        log(f"--- Starting {wi_id} (queue: {len(state.queue)} ahead) ---")

        sealed = execute_one(state, wi_id)
        if state.paused_at:
            return 3
        if not sealed:
            time.sleep(INTER_WI_SLEEP_SECONDS * (1 + state.consecutive_fails))
            continue

        time.sleep(INTER_WI_SLEEP_SECONDS)

    log("All WIs SEALED. CoreLink DoD evaluation pending — see charter §DoD.")
    return 0


# ----------------------------------------------------------------------------
# CLI
# ----------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--once", metavar="WI", help="Execute one WI and exit")
    parser.add_argument(
        "--bootstrap",
        action="store_true",
        help="Bootstrap queue from spec corpus and exit",
    )
    parser.add_argument(
        "--status", action="store_true", help="Print state and exit"
    )
    parser.add_argument(
        "--reset",
        action="store_true",
        help="Wipe state.json (keeps memory/log) and exit",
    )
    args = parser.parse_args()

    if args.status:
        state = State.load()
        print(json.dumps(asdict(state), indent=2))
        return 0
    if args.reset:
        STATE_PATH.unlink(missing_ok=True)
        log("State reset.")
        return 0
    if args.bootstrap:
        state = State.load()
        state.queue = bootstrap_queue(state)
        state.bootstrap_at = time.strftime("%Y-%m-%d %H:%M:%S")
        state.save()
        log(f"Bootstrapped queue: {len(state.queue)} WIs.")
        return 0

    claim_lock()
    signal.signal(signal.SIGTERM, lambda *a: (release_lock(), sys.exit(0)))
    signal.signal(signal.SIGINT, lambda *a: (release_lock(), sys.exit(0)))

    try:
        if args.once:
            state = State.load()
            execute_one(state, args.once)
            return 0
        return loop()
    finally:
        release_lock()


if __name__ == "__main__":
    sys.exit(main())
