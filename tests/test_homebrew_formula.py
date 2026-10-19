import hashlib
import tempfile
import unittest
from pathlib import Path

from scripts.homebrew_formula import PLATFORMS, homebrew_formula, stable_version, validate_release


class HomebrewFormulaTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.filenames = []
        self.checksums = []
        for _, targets in PLATFORMS:
            for _, target in targets:
                filename = f"fmn-{target}.tar.xz"
                contents = target.encode()
                (self.directory / filename).write_bytes(contents)
                digest = hashlib.sha256(contents).hexdigest()
                self.filenames.append(filename)
                self.checksums.append(f"{digest}  {filename}\n")
        self.write_checksums(self.checksums)

    def write_checksums(self, lines):
        (self.directory / "sha256.sum").write_text("".join(lines))

    def test_formula_pins_every_platform_and_checksums(self):
        formula = homebrew_formula("v0.1.0", self.directory)
        self.assertIn('version "0.1.0"', formula)
        self.assertIn('license "MIT"', formula)
        self.assertIn('system bin/"fmn", "doctor"', formula)
        self.assertEqual(formula.count("sha256 "), 4)
        for filename, checksum in zip(self.filenames, self.checksums):
            self.assertIn(f"releases/download/v0.1.0/{filename}", formula)
            self.assertIn(checksum.split()[0], formula)

    def test_rejects_tampered_archive(self):
        (self.directory / self.filenames[0]).write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "mismatched checksum"):
            homebrew_formula("v0.1.0", self.directory)

    def test_requires_every_archive_and_checksum(self):
        self.write_checksums(self.checksums[1:])
        with self.assertRaisesRegex(ValueError, "Missing or mismatched"):
            homebrew_formula("v0.1.0", self.directory)
        self.write_checksums(self.checksums)
        (self.directory / self.filenames[0]).unlink()
        with self.assertRaises(FileNotFoundError):
            homebrew_formula("v0.1.0", self.directory)

    def test_rejects_duplicate_checksums(self):
        self.write_checksums(self.checksums + self.checksums[:1])
        with self.assertRaisesRegex(ValueError, "Duplicate checksum"):
            homebrew_formula("v0.1.0", self.directory)

    def test_accepts_binary_checksum_marker(self):
        self.write_checksums([line.replace("  ", " *") for line in self.checksums])
        self.assertIn("class Fmn < Formula", homebrew_formula("v0.1.0", self.directory))

    def test_accepts_dist_checksum_spacing(self):
        self.write_checksums(["\n"] + [line.replace("  ", " *") + "\n" for line in self.checksums])
        self.assertEqual(homebrew_formula("v0.1.0", self.directory).count("sha256 "), 4)

    def test_requires_stable_semver_tag(self):
        for tag in ("0.1.0", "v01.0.0", "v0.1.0-rc.1", "v0.1.0/other"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                stable_version(tag)

    def test_only_publishes_the_latest_stable_release(self):
        release = {"tagName": "v0.1.0", "isDraft": False, "isPrerelease": False}
        self.assertTrue(validate_release("v0.1.0", release, "v0.1.0"))
        with self.assertRaisesRegex(ValueError, "other than"):
            validate_release("v0.1.0", release, "v0.2.0")
        for invalid in ({**release, "isDraft": True}, {**release, "tagName": "v0.2.0"}):
            with self.assertRaises(ValueError):
                validate_release("v0.1.0", invalid, "v0.1.0")
        prerelease = {**release, "tagName": "v0.2.0-rc.1", "isPrerelease": True}
        self.assertFalse(validate_release("v0.2.0-rc.1", prerelease, "v0.1.0"))


if __name__ == "__main__":
    unittest.main()
