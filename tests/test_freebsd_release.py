import io
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

from scripts.freebsd_release import TARGETS, archive_name, checksum, collect, package, validate_archive


class FreeBSDReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "fmn"
        self.binary.write_bytes(b"release binary fixture")
        self.binary.chmod(0o755)
        self.source = self.root / "source"
        self.destination = self.root / "destination"

    def test_collects_both_architectures_with_notices_and_portable_checksums(self):
        for target in TARGETS:
            package(self.binary, target, self.source)
        collect(self.source, self.destination)
        for target in TARGETS:
            archive = validate_archive(self.destination, target)
            self.assertEqual(archive.read_bytes(), (self.source / archive.name).read_bytes())
            self.assertEqual(
                Path(f"{archive}.sha256").read_text(),
                f"{checksum(archive)}  {archive.name}\n",
            )

    def test_missing_architecture_prevents_collection(self):
        package(self.binary, TARGETS[0], self.source)
        with self.assertRaises(FileNotFoundError):
            collect(self.source, self.destination)
        self.assertFalse(self.destination.exists())

    def test_staged_inputs_survive_global_cleanup_and_are_excluded_from_publishing(self):
        for target in TARGETS:
            package(self.binary, target, self.source)
            for suffix in ("", ".sha256"):
                (self.source / f"{archive_name(target)}{suffix}").unlink()
        collect(self.source, self.destination, staged=True)
        for target in TARGETS:
            validate_archive(self.destination, target)
        self.assertFalse(any(path.name.startswith(".") for path in self.destination.iterdir()))
        for path in self.destination.iterdir():
            shutil.copyfile(path, self.source / path.name)
        # Python's Path.glob includes dotfiles; the generated shell glob does not.
        published = subprocess.check_output(
            ["sh", "-c", 'printf "%s\\n" "$1"/*', "sh", str(self.source)], text=True
        )
        self.assertNotIn(".input-", published)

    def test_tampered_archive_prevents_collection(self):
        for target in TARGETS:
            package(self.binary, target, self.source)
        (self.source / archive_name(TARGETS[1])).write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "mismatched checksum"):
            collect(self.source, self.destination)
        self.assertFalse(self.destination.exists())

    def rewrite_archive(self, transform):
        target = TARGETS[0]
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
            package(self.binary, TARGETS[0], self.source)


if __name__ == "__main__":
    unittest.main()
