import io
from pathlib import Path
import os
import subprocess
import tarfile
import tempfile
import unittest

from bin.release import TARGETS, archive_name, checksum, package, validate_archive


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "fmn"
        self.binary.write_bytes(b"release binary fixture")
        self.binary.chmod(0o755)
        self.source = self.root / "source"

    def test_packages_all_targets_with_notices_and_portable_checksums(self):
        self.assertEqual(len(TARGETS), 6)
        for target in TARGETS:
            with self.subTest(target=target):
                package(self.binary, target, self.source)
                archive = validate_archive(self.source, target)
                self.assertEqual(
                    Path(f"{archive}.sha256").read_text(),
                    f"{checksum(archive)}  {archive.name}\n",
                )

    def test_tampered_archive_is_rejected(self):
        target = next(iter(TARGETS))
        package(self.binary, target, self.source)
        (self.source / archive_name(target)).write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "mismatched checksum"):
            validate_archive(self.source, target)

    def test_installer_selects_architecture_and_rejects_tampering(self):
        target = "aarch64-unknown-freebsd"
        package(self.binary, target, self.source)
        archive = self.source / archive_name(target)
        (self.source / "sha256.sum").write_text(Path(f"{archive}.sha256").read_text())
        tools = self.root / "tools"
        tools.mkdir()
        stubs = {
            "uname": "case \"$1\" in -s) echo FreeBSD ;; -m) echo arm64 ;; *) exit 1 ;; esac",
            "curl": """for arg do
                case "$arg" in https://*) name=${arg##*/} ;; esac
            done
            for last do :; done
            cp "$FIXTURES/$name" "$last" """,
        }
        for name, body in stubs.items():
            path = tools / name
            path.write_text("#!/bin/sh\n" + body + "\n")
            path.chmod(0o755)
        destination = self.root / "bin"
        environment = {
            **os.environ,
            "FMN_INSTALL_DIR": str(destination),
            "FIXTURES": str(self.source),
            "PATH": f"{tools}:{os.environ['PATH']}",
        }
        installer = Path(__file__).resolve().parents[1] / "bin/install.sh"
        subprocess.run(["sh", installer], env=environment, check=True, capture_output=True)
        installed = destination / "fmn"
        self.assertEqual(installed.read_bytes(), self.binary.read_bytes())
        self.assertTrue(os.access(installed, os.X_OK))
        archive.write_bytes(b"tampered")
        result = subprocess.run(["sh", installer], env=environment, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"Checksum mismatch", result.stderr)
        self.assertEqual(installed.read_bytes(), self.binary.read_bytes())

    def rewrite_archive(self, transform):
        target = next(iter(TARGETS))
        package(self.binary, target, self.source)
        archive = self.source / archive_name(target)
        with tarfile.open(archive) as source:
            entries = [(entry, source.extractfile(entry).read() if entry.isfile() else b"")
                       for entry in source]
        with tarfile.open(archive, "w:xz") as output:
            for entry, contents in entries:
                if transform(entry):
                    output.addfile(entry, io.BytesIO(contents))
        Path(f"{archive}.sha256").write_text(f"{checksum(archive)}  {archive.name}\n")
        return target

    def test_missing_palette_notice_is_rejected_even_with_valid_checksum(self):
        target = self.rewrite_archive(lambda entry: not entry.name.endswith("catppuccin.txt"))
        with self.assertRaisesRegex(ValueError, "Missing archive contents"):
            validate_archive(self.source, target)

    def test_archive_path_traversal_is_rejected(self):
        def escape(entry):
            if entry.name.endswith("/fmn"):
                entry.name = "../fmn"
            return True
        target = self.rewrite_archive(escape)
        with self.assertRaisesRegex(ValueError, "Unsafe archive member"):
            validate_archive(self.source, target)

    def test_nonexecutable_binary_is_rejected(self):
        self.binary.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "not executable"):
            package(self.binary, next(iter(TARGETS)), self.source)


if __name__ == "__main__":
    unittest.main()
