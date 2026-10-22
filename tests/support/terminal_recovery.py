"""Exercise a child in a real PTY without changing the developer's terminal."""

import errno
import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time


def run(scenario, output_stream, command):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    before = termios.tcgetattr(slave)
    transcript = bytearray()
    control_read, control_write = os.pipe()

    child = subprocess.Popen(
        command,
        stdin=slave,
        stdout=slave if output_stream != "stderr" else subprocess.PIPE,
        stderr=slave if output_stream != "stdout" else subprocess.PIPE,
        start_new_session=True,
        pass_fds=(control_read,),
        env={
            **os.environ,
            "TERM": "xterm-256color",
            "FMN_TERMINAL_TEST_CONTROL": str(control_read),
        },
    )
    os.close(control_read)

    def receive(timeout):
        if select.select([master], [], [], timeout)[0]:
            try:
                chunk = os.read(master, 65536)
                transcript.extend(chunk)
                return bool(chunk)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                return False
        return True

    def wait_for(marker):
        deadline = time.monotonic() + 10
        while marker not in transcript:
            assert time.monotonic() < deadline, f"timed out waiting for {marker!r}"
            receive(0.05)
            assert child.poll() is None or marker in transcript, "child exited early"

    try:
        wait_for(b"\x1b[?2004h" if scenario == "setup-panic" else b"\x1b[?25l")
        active = termios.tcgetattr(slave)
        assert not active[3] & (termios.ECHO | termios.ICANON), "PTY never entered raw mode"
        assert active != before

        if scenario in ("sigterm", "repeated-sigterm", "sigint", "sighup"):
            # Exercise shutdown with an overlay open and auto-execution enabled.
            os.write(master, b"?")  # Help
            wait_for(b"Help")
            selected_signal = {
                "sigint": signal.SIGINT,
                "sighup": signal.SIGHUP,
            }.get(scenario, signal.SIGTERM)
            for _ in range(16 if scenario == "repeated-sigterm" else 1):
                try:
                    child.send_signal(selected_signal)
                except ProcessLookupError:
                    break
            expected_status = -selected_signal
        elif scenario == "after-close":
            os.write(control_write, b"c")
            wait_for(b"FMN_SESSION_CLOSED")
            child.send_signal(signal.SIGTERM)
            expected_status = -signal.SIGTERM
        elif scenario == "cleanup-sigterm":
            os.write(control_write, b"c")
            wait_for(b"FMN_CLEANUP_PAUSED")
            child.send_signal(signal.SIGTERM)
            child.send_signal(signal.SIGTERM)
            os.write(control_write, b"c")
            expected_status = -signal.SIGTERM
        elif scenario in ("panic", "setup-panic"):
            os.write(control_write, b"p")
            expected_status = 101
        else:
            raise AssertionError(f"unknown scenario: {scenario}")

        deadline = time.monotonic() + 10
        while child.poll() is None:
            assert time.monotonic() < deadline, "child did not exit"
            receive(0.05)
        while select.select([master], [], [], 0)[0]:
            if not receive(0):
                break
        captured = child.communicate(timeout=2)

        assert child.returncode == expected_status, (child.returncode, expected_status)
        after = termios.tcgetattr(slave)
        # Darwin sets PENDIN as transient state when returning to canonical
        # input. Compare every setting, excluding only this kernel status bit.
        if sys.platform == "darwin":
            before[3] &= ~termios.PENDIN
            after[3] &= ~termios.PENDIN
        assert after == before, f"terminal attributes were not restored: {before!r} -> {after!r}"
        for mode in (2004, 1000, 1002, 1003, 1015, 1006, 1049):
            enabled = f"\x1b[?{mode}h".encode()
            disabled = f"\x1b[?{mode}l".encode()
            assert transcript.rfind(disabled) > transcript.rfind(enabled), (
                f"mode {mode} was not disabled after its last enable"
            )
        assert transcript.rfind(b"\x1b[?25h") > transcript.rfind(b"\x1b[?25l"), (
            "cursor was not restored after its last hide"
        )
        if scenario in ("panic", "setup-panic"):
            assert b"fmn terminal recovery test panic" in transcript
        else:
            # The shell captures the other stream as a command. Even with
            # execute_on_select=true, shutdown must never emit a selection.
            assert all(stream in (None, b"") for stream in captured), captured
    except BaseException:
        print(repr(bytes(transcript[-2000:])), file=sys.stderr)
        raise
    finally:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=2)
        os.close(master)
        os.close(slave)
        os.close(control_write)


if __name__ == "__main__":
    run(sys.argv[1], sys.argv[2], sys.argv[3:])
