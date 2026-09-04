"""Bounded subprocess execution for capacity evidence and locked commands."""

from __future__ import annotations

import os
import selectors
import signal
import subprocess
import time
from pathlib import Path


PROCESS_TERM_GRACE_SECONDS = 0.25
PROCESS_GROUP_VERIFY_SECONDS = 0.5


class BoundedOutputError(RuntimeError):
    """The child exceeded the caller's captured-output budget."""


class ProcessBoundsError(RuntimeError):
    """The process group could not be terminated within the cleanup bound."""


def kill_process_group(process: subprocess.Popen[object]) -> None:
    """Terminate a bounded subprocess and all descendants in its session."""
    group_id = process.pid

    def group_alive() -> bool:
        if os.name != "posix":
            return process.poll() is None
        try:
            os.killpg(group_id, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True

    try:
        if os.name == "posix":
            os.killpg(group_id, signal.SIGTERM)
        else:
            process.terminate()
    except ProcessLookupError:
        return
    except PermissionError:
        try:
            process.terminate()
        except ProcessLookupError:
            return
    term_deadline = time.monotonic() + PROCESS_TERM_GRACE_SECONDS
    while group_alive() and time.monotonic() < term_deadline:
        try:
            process.wait(timeout=min(0.05, max(0.0, term_deadline - time.monotonic())))
        except subprocess.TimeoutExpired:
            pass
    if not group_alive():
        process.wait()
        return
    try:
        if os.name == "posix":
            os.killpg(group_id, signal.SIGKILL)
        else:
            process.kill()
    except ProcessLookupError:
        return
    except PermissionError:
        try:
            process.kill()
        except ProcessLookupError:
            return
    verify_deadline = time.monotonic() + PROCESS_GROUP_VERIFY_SECONDS
    while group_alive() and time.monotonic() < verify_deadline:
        try:
            process.wait(timeout=min(0.05, max(0.0, verify_deadline - time.monotonic())))
        except subprocess.TimeoutExpired:
            pass
    if group_alive():
        raise ProcessBoundsError("timed-out process group did not terminate")
    process.wait()


def run_bounded(command: list[str], *, cwd: Path, timeout: float,
                capture_output: bool = False,
                max_output_bytes: int | None = None) -> subprocess.CompletedProcess[object]:
    """Run in a private process group, streaming captured output to a hard cap."""
    deadline = time.monotonic() + timeout
    process = subprocess.Popen(
        command, cwd=cwd, start_new_session=(os.name == "posix"),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE if capture_output else None,
        stderr=subprocess.PIPE if capture_output else None,
        text=capture_output, errors="surrogateescape" if capture_output else None,
    )
    if capture_output and max_output_bytes is not None:
        assert process.stdout is not None and process.stderr is not None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ, "stdout")
        selector.register(process.stderr, selectors.EVENT_READ, "stderr")
        chunks = {"stdout": bytearray(), "stderr": bytearray()}
        try:
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not (events := selector.select(remaining)):
                    raise subprocess.TimeoutExpired(command, timeout)
                for key, _ in events:
                    data = os.read(key.fd, 64 * 1024)
                    if not data:
                        selector.unregister(key.fileobj)
                        continue
                    chunks[key.data].extend(data)
                    if sum(map(len, chunks.values())) > max_output_bytes:
                        raise BoundedOutputError(
                            f"command output exceeded {max_output_bytes} bytes"
                        )
            process.wait(timeout=max(0.0, deadline - time.monotonic()))
        except (subprocess.TimeoutExpired, BoundedOutputError):
            kill_process_group(process)
            raise
        finally:
            selector.close()
            process.stdout.close()
            process.stderr.close()
        return subprocess.CompletedProcess(
            command, process.returncode,
            bytes(chunks["stdout"]).decode(errors="surrogateescape"),
            bytes(chunks["stderr"]).decode(errors="surrogateescape"),
        )
    try:
        stdout, stderr = process.communicate(timeout=max(0.0, deadline - time.monotonic()))
    except subprocess.TimeoutExpired as error:
        try:
            kill_process_group(process)
        finally:
            if process.stdout is not None:
                process.stdout.close()
            if process.stderr is not None:
                process.stderr.close()
        raise subprocess.TimeoutExpired(command, timeout, output=error.output,
                                        stderr=error.stderr) from error
    # ``communicate`` only waits for the leader when stdout/stderr are not
    # pipes.  Keep the private process group bounded in that mode as well:
    # a successful shell which backgrounds a materializer must not release
    # the caller's lock while that descendant is still running.
    if os.name == "posix":
        while True:
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                break
            except PermissionError:
                raise ProcessBoundsError("cannot inspect timed-out process group")
            if time.monotonic() >= deadline:
                try:
                    kill_process_group(process)
                except ProcessBoundsError:
                    raise
                raise subprocess.TimeoutExpired(command, timeout)
            time.sleep(0.01)
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
