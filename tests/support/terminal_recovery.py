"""Drive a child through an isolated PTY and verify its terminal cleanup."""

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


def run(scenario, ui_stream, command, env, *, selection=b"", expected=None):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    before = termios.tcgetattr(slave)
    transcript = bytearray()
    child = subprocess.Popen(
        command,
        stdin=slave,
        stdout=slave if ui_stream == "stdout" else subprocess.PIPE,
        stderr=slave if ui_stream == "stderr" else subprocess.PIPE,
        start_new_session=True,
        env={**env, "TERM": "xterm-256color"},
    )

    def receive(timeout):
        if select.select([master], [], [], timeout)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                return False
            transcript.extend(chunk)
            return bool(chunk)
        return True

    def wait_for(marker, start=0):
        deadline = time.monotonic() + 10
        while transcript.find(marker, start) < 0:
            assert time.monotonic() < deadline, f"timed out waiting for {marker!r}"
            receive(0.05)
            assert child.poll() is None or transcript.find(marker, start) >= 0, "child exited early"

    try:
        wait_for(b"COMMANDS")
        active = termios.tcgetattr(slave)
        assert not active[3] & (termios.ECHO | termios.ICANON), "PTY never entered raw mode"

        if scenario == "enter":
            os.write(master, b"\r")
        elif scenario == "cancel":
            os.write(master, b"\x1b")
        elif scenario == "resize-paste":
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 110, 0, 0))
            child.send_signal(signal.SIGWINCH)
            wait_for(b"INSPECTOR", len(transcript))
            os.write(master, b"\x1b[200~literal?\x1b[201~")
            wait_for(b"literal?")
            os.write(master, b"\r")
        else:
            start = len(transcript)
            os.write(master, b"?")
            wait_for(b"Help", start)
            selected_signal = {
                "sigint": signal.SIGINT,
                "sighup": signal.SIGHUP,
                "sigterm": signal.SIGTERM,
                "repeated-sigterm": signal.SIGTERM,
            }[scenario]
            for _ in range(16 if scenario == "repeated-sigterm" else 1):
                try:
                    child.send_signal(selected_signal)
                except ProcessLookupError:
                    break
            expected = -selected_signal

        deadline = time.monotonic() + 10
        while child.poll() is None:
            assert time.monotonic() < deadline, "child did not exit"
            receive(0.05)
        while select.select([master], [], [], 0)[0] and receive(0):
            pass
        captured = child.communicate(timeout=2)
        assert child.returncode == expected, (child.returncode, expected, captured)
        after = termios.tcgetattr(slave)
        if sys.platform == "darwin":
            before[3] &= ~termios.PENDIN
            after[3] &= ~termios.PENDIN
        assert after == before, f"terminal attributes were not restored: {before!r} -> {after!r}"
        for mode in (2004, 1002, 1006, 1049):
            enabled = f"\x1b[?{mode}h".encode()
            disabled = f"\x1b[?{mode}l".encode()
            assert 0 <= transcript.rfind(enabled) < transcript.rfind(disabled), (
                f"mode {mode} was not disabled after its last enable"
            )
        assert transcript.rfind(b"\x1b[?25h") > transcript.rfind(b"\x1b[?25l")
        assert captured == ((None, selection) if ui_stream == "stdout" else (selection, None)), captured
        return bytes(transcript)
    except BaseException:
        print(repr(bytes(transcript[-2000:])), file=sys.stderr)
        raise
    finally:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=2)
        os.close(master)
        os.close(slave)
